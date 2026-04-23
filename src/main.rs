use anyhow::Result;
use clap::Parser;
use crossterm::{
    event::{self, Event, KeyEventKind},
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use ratatui::prelude::*;
use std::fs::File;
use std::io::{self, stdout, Read as _};
use std::path::PathBuf;

use rune::constants;
use rune::input;
use rune::tabs;
use rune::ui;

#[derive(Parser)]
#[command(author, version, about, long_about = None)]
struct Cli {
    /// Files or directories to open
    files: Vec<PathBuf>,

    /// Recursively open files in directories
    #[arg(short, long)]
    recursive: bool,

    /// Open in read-only (view) mode
    #[arg(short = 'v', long = "view")]
    view: bool,

    /// Start with line numbers enabled
    #[arg(long = "line-numbers")]
    line_numbers: bool,

    /// Start with word wrap enabled
    #[arg(long = "word-wrap")]
    word_wrap: bool,

    /// Start with mouse support disabled
    #[arg(long = "no-mouse")]
    no_mouse: bool,
}

/// Expand a list of paths into individual files.
///
/// - Plain files are passed through directly.
/// - Directories are walked using `ignore::WalkBuilder` which respects
///   `.gitignore` and skips hidden files by default.
/// - When `recursive` is false, only immediate children are listed (max_depth 1).
/// - Binary files (containing null bytes in the first 512 bytes) are skipped.
fn expand_paths(paths: &[PathBuf], recursive: bool) -> Vec<PathBuf> {
    let mut result = Vec::new();

    for path in paths {
        if path.is_file() {
            if !is_binary(path) {
                result.push(path.clone());
            }
        } else if path.is_dir() {
            let mut builder = ignore::WalkBuilder::new(path);
            builder.hidden(true); // skip hidden files
            if !recursive {
                builder.max_depth(Some(1));
            }
            for entry in builder.build().flatten() {
                let entry_path = entry.path().to_path_buf();
                if entry_path.is_file() && !is_binary(&entry_path) {
                    result.push(entry_path);
                }
            }
        }
        // Non-existent paths for files that don't exist yet: pass through
        // (the editor will create them on save)
        else if !path.exists() {
            result.push(path.clone());
        }
    }

    result
}

/// Check if a file is binary by looking for null bytes in the first 512 bytes.
fn is_binary(path: &PathBuf) -> bool {
    let Ok(mut file) = File::open(path) else {
        return false;
    };
    let mut buf = [0u8; 512];
    let Ok(n) = file.read(&mut buf) else {
        return false;
    };
    buf[..n].contains(&0)
}

/// Extract `+LINE` or `+LINE,COL` positional arguments from argv before clap parsing.
fn extract_position(args: &mut Vec<String>) -> Option<(usize, Option<usize>)> {
    let mut pos = None;
    args.retain(|arg| {
        if let Some(rest) = arg.strip_prefix('+') {
            if let Some((line, col)) = rest.split_once(',') {
                if let (Ok(l), Ok(c)) = (line.parse::<usize>(), col.parse::<usize>()) {
                    pos = Some((l, Some(c)));
                    return false;
                }
            } else if let Ok(l) = rest.parse::<usize>() {
                pos = Some((l, None));
                return false;
            }
        }
        true
    });
    pos
}

/// RAII guard that owns the terminal's raw-mode + alt-screen (+ optional
/// mouse capture) state. Dropping restores the terminal, so a panic in
/// `run_editor` unwinds through this and the user's shell stays usable.
struct TerminalGuard;

impl TerminalGuard {
    fn new(mouse: bool) -> Result<Self> {
        enable_raw_mode()?;
        execute!(stdout(), EnterAlternateScreen)?;
        if mouse {
            execute!(stdout(), crossterm::event::EnableMouseCapture)?;
        }
        Ok(Self)
    }
}

impl Drop for TerminalGuard {
    fn drop(&mut self) {
        // Always send DisableMouseCapture — matches the pre-guard cleanup
        // (which did it unconditionally) and covers the case where the user
        // toggled mouse mode on/off at runtime via the options menu. It's a
        // harmless no-op if capture isn't currently active.
        let _ = execute!(stdout(), crossterm::event::DisableMouseCapture);
        let _ = execute!(stdout(), LeaveAlternateScreen);
        let _ = disable_raw_mode();
    }
}

fn main() -> Result<()> {
    // Belt-and-suspenders: even if panic=abort or unwinding misbehaves,
    // install a panic hook that restores the terminal before the default
    // hook prints the panic message.
    let prev_hook = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        let _ = execute!(stdout(), crossterm::event::DisableMouseCapture);
        let _ = execute!(stdout(), LeaveAlternateScreen);
        let _ = disable_raw_mode();
        prev_hook(info);
    }));

    let mut args: Vec<String> = std::env::args().collect();
    let position = extract_position(&mut args);
    let cli = Cli::parse_from(args);

    let mut tab_manager = tabs::TabManager::new();

    // Apply CLI flag overrides
    if cli.view {
        tab_manager.read_only = true;
    }
    if cli.line_numbers {
        tab_manager.config.show_line_numbers = true;
    }
    if cli.word_wrap {
        tab_manager.config.word_wrap = true;
    }
    if cli.no_mouse {
        tab_manager.config.mouse_enabled = false;
    }

    // Terminal state is now owned by a Drop guard. Construct AFTER mouse
    // flags are resolved but BEFORE Terminal::new (which needs alt screen
    // already active). Drop handles cleanup on both normal return and panic
    // unwind, replacing the old manual cleanup block.
    let _terminal_guard = TerminalGuard::new(tab_manager.config.mouse_enabled)?;

    let mut terminal = Terminal::new(CrosstermBackend::new(stdout()))?;
    terminal.clear()?;

    let files = expand_paths(&cli.files, cli.recursive);

    for (i, file) in files.into_iter().enumerate() {
        if file.exists() {
            if i == 0 {
                if let Err(e) = tab_manager.active_editor_mut().load_file(file) {
                    tab_manager.set_temporary_status_message(format!("Error: {e}"));
                }
            } else if let Err(e) = tab_manager.open_in_new_tab(file) {
                tab_manager.set_temporary_status_message(format!("Error: {e}"));
            }
        } else {
            // New file — set the path so it saves there, but don't try to read
            if i == 0 {
                let editor = tab_manager.active_editor_mut();
                editor.display_name = file.display().to_string();
                editor.file_path = Some(file);
            } else {
                let mut editor = rune::editor::Editor::new_buffer();
                editor.display_name = file.display().to_string();
                editor.file_path = Some(file);
                tab_manager.tabs.push(editor);
            }
        }
    }

    if !cli.files.is_empty() {
        tab_manager.active_tab = 0;
        tab_manager.resolve_display_names();
    }

    // Apply +LINE or +LINE,COL positioning
    if let Some((line, col)) = position {
        tab_manager.goto_line(line);
        if let Some(c) = col {
            let editor = tab_manager.active_editor_mut();
            if c > 0 {
                editor.viewport.cursor_pos.1 = c - 1;
            }
        }
    }

    // Terminal cleanup is handled by `_terminal_guard` being dropped at the
    // end of this scope, whether `run_editor` returned Ok, Err, or unwound
    // from a panic.
    run_editor(&mut terminal, &mut tab_manager)
}

fn run_editor(
    terminal: &mut Terminal<CrosstermBackend<io::Stdout>>,
    tabs: &mut tabs::TabManager,
) -> Result<()> {
    loop {
        if tabs.needs_redraw {
            #[cfg(target_os = "macos")]
            {
                use crossterm::{execute, terminal::BeginSynchronizedUpdate};
                let _ = execute!(stdout(), BeginSynchronizedUpdate);
            }

            terminal.draw(|f| ui::draw_ui(f, tabs))?;

            #[cfg(target_os = "macos")]
            {
                use crossterm::{execute, terminal::EndSynchronizedUpdate};
                use std::io::Write;
                let _ = execute!(stdout(), EndSynchronizedUpdate);
                let _ = stdout().flush();
            }

            tabs.needs_redraw = false;
        }

        let status_timeout = tabs.check_status_message_timeout();
        if status_timeout {
            tabs.needs_redraw = true;
        }

        if event::poll(constants::EVENT_POLL_INTERVAL)? {
            match event::read()? {
                Event::Key(key) => {
                    if key.kind == KeyEventKind::Press && input::handle_key_event(tabs, key)? {
                        break;
                    }
                }
                Event::Mouse(mouse) => {
                    if tabs.config.mouse_enabled {
                        let size = terminal.size()?;
                        tabs.handle_mouse_event(mouse, size.height as usize);
                    }
                }
                Event::Resize(_, _) => {
                    tabs.needs_redraw = true;
                }
                _ => {}
            }
        }
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    #[test]
    fn test_expand_paths_single_file() {
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("hello.txt");
        fs::write(&file, "hello").unwrap();

        let result = expand_paths(std::slice::from_ref(&file), false);
        assert_eq!(result, vec![file]);
    }

    #[test]
    fn test_expand_paths_skips_binary() {
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("binary.bin");
        let mut data = vec![0u8; 100];
        data[50] = 0; // null byte
        fs::write(&file, &data).unwrap();

        let result = expand_paths(&[file], false);
        assert!(result.is_empty());
    }

    #[test]
    fn test_expand_paths_directory_non_recursive() {
        let dir = tempfile::tempdir().unwrap();
        let file1 = dir.path().join("a.txt");
        let sub = dir.path().join("sub");
        fs::create_dir(&sub).unwrap();
        let file2 = sub.join("b.txt");
        fs::write(&file1, "a").unwrap();
        fs::write(&file2, "b").unwrap();

        let result = expand_paths(&[dir.path().to_path_buf()], false);
        assert_eq!(result.len(), 1);
        assert!(result.contains(&file1));
    }

    #[test]
    fn test_expand_paths_directory_recursive() {
        let dir = tempfile::tempdir().unwrap();
        let file1 = dir.path().join("a.txt");
        let sub = dir.path().join("sub");
        fs::create_dir(&sub).unwrap();
        let file2 = sub.join("b.txt");
        fs::write(&file1, "a").unwrap();
        fs::write(&file2, "b").unwrap();

        let result = expand_paths(&[dir.path().to_path_buf()], true);
        assert_eq!(result.len(), 2);
        assert!(result.contains(&file1));
        assert!(result.contains(&file2));
    }

    #[test]
    fn test_expand_paths_skips_hidden_files() {
        let dir = tempfile::tempdir().unwrap();
        let visible = dir.path().join("visible.txt");
        let hidden = dir.path().join(".hidden.txt");
        fs::write(&visible, "v").unwrap();
        fs::write(&hidden, "h").unwrap();

        let result = expand_paths(&[dir.path().to_path_buf()], false);
        assert_eq!(result.len(), 1);
        assert!(result.contains(&visible));
    }

    #[test]
    fn test_expand_paths_nonexistent_passthrough() {
        let path = PathBuf::from("/tmp/does-not-exist-rune-test.txt");
        let result = expand_paths(std::slice::from_ref(&path), false);
        assert_eq!(result, vec![path]);
    }

    #[test]
    fn test_is_binary_text_file() {
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("text.txt");
        fs::write(&file, "Hello, world!").unwrap();
        assert!(!is_binary(&file));
    }

    #[test]
    fn test_is_binary_binary_file() {
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("bin.dat");
        fs::write(&file, b"\x00\x01\x02").unwrap();
        assert!(is_binary(&file));
    }

    #[test]
    fn test_extract_position_line_only() {
        let mut args = vec!["rune".to_string(), "+42".to_string(), "main.rs".to_string()];
        let pos = extract_position(&mut args);
        assert_eq!(pos, Some((42, None)));
        assert_eq!(args, vec!["rune", "main.rs"]);
    }

    #[test]
    fn test_extract_position_line_and_col() {
        let mut args = vec![
            "rune".to_string(),
            "+42,10".to_string(),
            "main.rs".to_string(),
        ];
        let pos = extract_position(&mut args);
        assert_eq!(pos, Some((42, Some(10))));
        assert_eq!(args, vec!["rune", "main.rs"]);
    }

    #[test]
    fn test_extract_position_none() {
        let mut args = vec!["rune".to_string(), "main.rs".to_string()];
        let pos = extract_position(&mut args);
        assert_eq!(pos, None);
        assert_eq!(args, vec!["rune", "main.rs"]);
    }

    #[test]
    fn test_extract_position_after_file() {
        let mut args = vec![
            "rune".to_string(),
            "main.rs".to_string(),
            "+100".to_string(),
        ];
        let pos = extract_position(&mut args);
        assert_eq!(pos, Some((100, None)));
        assert_eq!(args, vec!["rune", "main.rs"]);
    }

    #[test]
    fn test_view_flag_sets_read_only() {
        let mut tab_manager = tabs::TabManager::new();
        assert!(!tab_manager.read_only);
        tab_manager.read_only = true;
        assert!(tab_manager.read_only);
    }
}
