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

use rune::constants;
use rune::input;
use rune::tabs;
use rune::ui;

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

    let mut tab_manager = tabs::TabManager::new();

    if tab_manager.config.mouse_enabled {
        crossterm::execute!(stdout(), crossterm::event::EnableMouseCapture)?;
    }

    if let Some(file) = cli.file {
        tab_manager.active_editor_mut().load_file(file)?;
        tab_manager.resolve_display_names();
    }

    let result = run_editor(&mut terminal, &mut tab_manager);

    disable_raw_mode()?;
    execute!(stdout(), LeaveAlternateScreen)?;
    crossterm::execute!(stdout(), crossterm::event::DisableMouseCapture)?;

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
