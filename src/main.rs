use anyhow::Result;
use clap::Parser;
use crossterm::{
    event::{self, Event, KeyEventKind},
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use ratatui::prelude::*;
use std::io::{self, stdout};
use std::path::PathBuf;

pub mod constants {
    use std::time::Duration;

    pub const DEFAULT_TAB_WIDTH: usize = 4;
    pub const STATUS_MESSAGE_TIMEOUT: Duration = Duration::from_secs(3);
    pub const FALLBACK_TERMINAL_HEIGHT: usize = 24;
    pub const EVENT_POLL_INTERVAL: Duration = Duration::from_millis(100);
    pub const SCROLL_SPEED: usize = 3;
    pub const SEARCH_HISTORY_LIMIT: usize = 50;
    pub const HELP_MODAL_WIDTH: u16 = 48;
    pub const UNDO_STACK_LIMIT: usize = 100;
}

mod config;
mod editor;
mod hex;
mod input;
mod search;
mod syntax;
mod ui;

#[derive(Parser)]
#[command(author, version, about, long_about = None)]
struct Cli {
    /// File to edit
    file: Option<PathBuf>,
}

fn main() -> Result<()> {
    let cli = Cli::parse();

    enable_raw_mode()?;
    execute!(stdout(), EnterAlternateScreen)?;

    let mut terminal = Terminal::new(CrosstermBackend::new(stdout()))?;
    terminal.clear()?;

    let mut editor = editor::Editor::new();

    if editor.config.mouse_enabled {
        crossterm::execute!(stdout(), crossterm::event::EnableMouseCapture)?;
    }

    if let Some(file) = cli.file {
        editor.load_file(file)?;
    }

    let result = run_editor(&mut terminal, &mut editor);

    disable_raw_mode()?;
    execute!(stdout(), LeaveAlternateScreen)?;
    crossterm::execute!(stdout(), crossterm::event::DisableMouseCapture)?;

    result
}

fn run_editor(
    terminal: &mut Terminal<CrosstermBackend<io::Stdout>>,
    editor: &mut editor::Editor,
) -> Result<()> {
    loop {
        if editor.needs_redraw {
            #[cfg(target_os = "macos")]
            {
                use crossterm::{execute, terminal::BeginSynchronizedUpdate};
                let _ = execute!(stdout(), BeginSynchronizedUpdate);
            }

            terminal.draw(|f| ui::draw_ui(f, editor))?;

            #[cfg(target_os = "macos")]
            {
                use crossterm::{execute, terminal::EndSynchronizedUpdate};
                use std::io::Write;
                let _ = execute!(stdout(), EndSynchronizedUpdate);
                let _ = stdout().flush();
            }

            editor.needs_redraw = false;
        }

        let status_timeout = editor.check_status_message_timeout();
        if status_timeout {
            editor.needs_redraw = true;
        }

        if event::poll(constants::EVENT_POLL_INTERVAL)? {
            match event::read()? {
                Event::Key(key) => {
                    if key.kind == KeyEventKind::Press
                        && input::handle_key_event(editor, key)?
                    {
                        break;
                    }
                }
                Event::Mouse(mouse) => {
                    if editor.config.mouse_enabled {
                        let size = terminal.size()?;
                        editor.handle_mouse_event(mouse, size.height as usize);
                    }
                }
                Event::Resize(_, _) => {
                    editor.needs_redraw = true;
                }
                _ => {}
            }
        }
    }

    Ok(())
}
