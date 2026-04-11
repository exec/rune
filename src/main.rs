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

fn main() -> Result<()> {
    let cli = Cli::parse();

    enable_raw_mode()?;
    execute!(stdout(), EnterAlternateScreen)?;

    let mut terminal = Terminal::new(CrosstermBackend::new(stdout()))?;
    terminal.clear()?;

    let mut tab_manager = tabs::TabManager::new();

    if tab_manager.config.mouse_enabled {
        crossterm::execute!(stdout(), crossterm::event::EnableMouseCapture)?;
    }

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

    let result = run_editor(&mut terminal, &mut tab_manager);

    // Always clean up terminal state, even if run_editor returned an error
    let _ = disable_raw_mode();
    let _ = execute!(stdout(), LeaveAlternateScreen);
    let _ = crossterm::execute!(stdout(), crossterm::event::DisableMouseCapture);

    result
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
                    if key.kind == KeyEventKind::Press
                        && input::handle_key_event(tabs, key)?
                    {
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

        let result = expand_paths(&[file.clone()], false);
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
        let result = expand_paths(&[path.clone()], false);
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
}
