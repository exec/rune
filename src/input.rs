use anyhow::Result;
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use unicode_width::UnicodeWidthStr;

use crate::editor::{Editor, InputMode};
use crate::search::{FindNavigationMode, ReplacePhase};

pub fn handle_key_event(editor: &mut Editor, key: KeyEvent) -> Result<bool> {
    match editor.input_mode {
        InputMode::ConfirmQuit => handle_confirm_quit(editor, key),
        InputMode::OptionsMenu => handle_options_menu(editor, key),
        InputMode::FindOptionsMenu => handle_find_options_menu(editor, key),
        InputMode::EnteringFilename | InputMode::EnteringSaveAs => {
            handle_filename_input(editor, key)
        }
        InputMode::Find => handle_find(editor, key),
        InputMode::Replace => handle_replace(editor, key),
        InputMode::ReplaceConfirm => handle_replace_confirm(editor, key),
        InputMode::GoToLine => handle_goto_line(editor, key),
        InputMode::Help => handle_help(editor, key),
        InputMode::Normal => handle_normal(editor, key),
    }
}

fn handle_confirm_quit(editor: &mut Editor, key: KeyEvent) -> Result<bool> {
    match key.code {
        KeyCode::Char('y') | KeyCode::Char('Y') => {
            return editor.handle_quit_confirmation(true);
        }
        KeyCode::Char('n') | KeyCode::Char('N') => {
            return editor.handle_quit_confirmation(false);
        }
        KeyCode::Esc => {
            editor.cancel_quit_confirmation();
        }
        _ if key.modifiers == KeyModifiers::CONTROL && key.code == KeyCode::Char('c') => {
            editor.cancel_quit_confirmation();
        }
        _ => {}
    }
    Ok(false)
}

fn handle_options_menu(editor: &mut Editor, key: KeyEvent) -> Result<bool> {
    match key.code {
        KeyCode::Char('m') | KeyCode::Char('M') => {
            editor.toggle_mouse_mode();
            editor.input_mode = InputMode::Normal;
            editor.needs_redraw = true;
        }
        KeyCode::Char('l') | KeyCode::Char('L') => {
            editor.config.show_line_numbers = !editor.config.show_line_numbers;
            editor.set_temporary_status_message(format!(
                "Line numbers: {}",
                if editor.config.show_line_numbers {
                    "ON"
                } else {
                    "OFF"
                }
            ));
            editor.input_mode = InputMode::Normal;
            editor.needs_redraw = true;
        }
        KeyCode::Char('w') | KeyCode::Char('W') => {
            editor.config.word_wrap = !editor.config.word_wrap;
            editor.set_temporary_status_message(format!(
                "Word wrap: {}",
                if editor.config.word_wrap { "ON" } else { "OFF" }
            ));
            editor.input_mode = InputMode::Normal;
            editor.needs_redraw = true;
        }
        KeyCode::Char('t') | KeyCode::Char('T') => {
            editor.config.tab_width = match editor.config.tab_width {
                2 => 4,
                4 => 8,
                _ => 2,
            };
            editor
                .set_temporary_status_message(format!("Tab width: {}", editor.config.tab_width));
            editor.input_mode = InputMode::Normal;
            editor.needs_redraw = true;
        }
        KeyCode::Esc => {
            editor.save_config();
            editor.input_mode = InputMode::Normal;
            editor.status_message.clear();
            editor.needs_redraw = true;
        }
        _ if key.modifiers == KeyModifiers::CONTROL && key.code == KeyCode::Char('c') => {
            editor.save_config();
            editor.input_mode = InputMode::Normal;
            editor.status_message.clear();
            editor.needs_redraw = true;
        }
        _ => {}
    }
    Ok(false)
}

fn handle_find_options_menu(editor: &mut Editor, key: KeyEvent) -> Result<bool> {
    match key.code {
        KeyCode::Char('c') | KeyCode::Char('C') => {
            editor.toggle_case_sensitive();
            editor.set_temporary_status_message(format!(
                "Case sensitivity: {}",
                if editor.search.case_sensitive {
                    "ON"
                } else {
                    "OFF"
                }
            ));
            editor.input_mode = InputMode::Find;
        }
        KeyCode::Char('r') | KeyCode::Char('R') => {
            editor.toggle_regex_mode();
            editor.input_mode = InputMode::Find;
        }
        KeyCode::Esc => {
            editor.input_mode = InputMode::Find;
        }
        _ if key.modifiers == KeyModifiers::CONTROL && key.code == KeyCode::Char('c') => {
            editor.input_mode = InputMode::Find;
        }
        _ => {}
    }
    Ok(false)
}

fn handle_filename_input(editor: &mut Editor, key: KeyEvent) -> Result<bool> {
    match key.code {
        KeyCode::Enter => {
            return editor.finish_filename_input();
        }
        KeyCode::Esc => {
            editor.cancel_filename_input();
        }
        KeyCode::Backspace => {
            editor.filename_buffer.pop();
            editor.status_message = format!("File Name to Write: {}", editor.filename_buffer);
            editor.needs_redraw = true;
        }
        KeyCode::Char(c) => {
            editor.filename_buffer.push(c);
            editor.status_message = format!("File Name to Write: {}", editor.filename_buffer);
            editor.needs_redraw = true;
        }
        _ => {}
    }
    Ok(false)
}

fn handle_find(editor: &mut Editor, key: KeyEvent) -> Result<bool> {
    match key.code {
        KeyCode::Char('r') if key.modifiers == KeyModifiers::CONTROL => {
            if !editor.search.search_buffer.is_empty() {
                editor.input_mode = InputMode::Replace;
                editor.search.replace_buffer.clear();
                editor.search.replace_phase = ReplacePhase::ReplaceWith;
                editor.status_message =
                    format!("Replace '{}' with: ", editor.search.search_buffer);
                editor.needs_redraw = true;
            } else {
                editor.toggle_regex_mode();
            }
        }
        KeyCode::Char('o') if key.modifiers == KeyModifiers::CONTROL => {
            editor.input_mode = InputMode::FindOptionsMenu;
            editor.needs_redraw = true;
        }
        KeyCode::Enter => {
            let search_term = editor.search.search_buffer.clone();
            editor.search.add_to_search_history(&search_term);

            if editor.search.find_navigation_mode == FindNavigationMode::ResultNavigation
                && !editor.search.search_matches.is_empty()
            {
                editor.input_mode = InputMode::Normal;
                editor.search.search_matches.clear();
                editor.search.current_match_index = None;
                editor.search.search_buffer.clear();
                editor.set_temporary_status_message("Search completed".to_string());
            } else {
                let search_term = editor.search.search_buffer.clone();
                if editor.perform_find(&search_term) {
                    editor.search.find_navigation_mode = FindNavigationMode::ResultNavigation;
                    let matches_count = editor.search.search_matches.len();
                    let current = editor.search.current_match_index.map(|i| i + 1).unwrap_or(1);
                    editor.status_message = format!(
                        "Find: {search_term} ({current}/{matches_count} matches) - Use \u{2191}\u{2193} to navigate, Enter/Esc to exit"
                    );
                } else {
                    editor.set_temporary_status_message("Not found".to_string());
                    editor.input_mode = InputMode::Normal;
                }
            }
        }
        KeyCode::Esc => {
            editor.cancel_search();
            editor.input_mode = InputMode::Normal;
            editor.set_temporary_status_message("Search cancelled".to_string());
        }
        KeyCode::Char('c') if key.modifiers == KeyModifiers::CONTROL => {
            editor.cancel_search();
            editor.input_mode = InputMode::Normal;
            editor.set_temporary_status_message("Search cancelled".to_string());
        }
        KeyCode::Up | KeyCode::Left => {
            if key.code == KeyCode::Up
                && editor.search.find_navigation_mode == FindNavigationMode::HistoryBrowsing
            {
                if editor.search.navigate_search_history_up() {
                    editor.status_message =
                        format!("Find: {}", editor.search.search_buffer);
                    editor.needs_redraw = true;
                    if !editor.search.search_buffer.is_empty() {
                        let search_term = editor.search.search_buffer.clone();
                        editor.perform_find(&search_term);
                    }
                } else {
                    editor.move_cursor_up();
                }
            } else if editor.search.find_navigation_mode == FindNavigationMode::ResultNavigation
                && !editor.search.search_matches.is_empty()
            {
                editor.find_previous_match();
                let matches_count = editor.search.search_matches.len();
                let current = editor.search.current_match_index.map(|i| i + 1).unwrap_or(1);
                editor.status_message = format!(
                    "Find: {} ({current}/{matches_count} matches) - Use arrows to navigate, Enter/Esc to exit",
                    editor.search.search_buffer
                );
                editor.needs_redraw = true;
            } else if key.code == KeyCode::Up {
                editor.move_cursor_up();
            } else {
                editor.move_cursor_left();
            }
        }
        KeyCode::Down | KeyCode::Right => {
            if key.code == KeyCode::Down
                && editor.search.find_navigation_mode == FindNavigationMode::HistoryBrowsing
            {
                if editor.search.navigate_search_history_down() {
                    editor.status_message =
                        format!("Find: {}", editor.search.search_buffer);
                    editor.needs_redraw = true;
                    if !editor.search.search_buffer.is_empty() {
                        let search_term = editor.search.search_buffer.clone();
                        editor.perform_find(&search_term);
                    } else {
                        editor.search.search_matches.clear();
                        editor.search.current_match_index = None;
                    }
                } else {
                    editor.move_cursor_down();
                }
            } else if editor.search.find_navigation_mode == FindNavigationMode::ResultNavigation
                && !editor.search.search_matches.is_empty()
            {
                editor.find_next_match();
                let matches_count = editor.search.search_matches.len();
                let current = editor.search.current_match_index.map(|i| i + 1).unwrap_or(1);
                editor.status_message = format!(
                    "Find: {} ({current}/{matches_count} matches) - Use arrows to navigate, Enter/Esc to exit",
                    editor.search.search_buffer
                );
                editor.needs_redraw = true;
            } else if key.code == KeyCode::Down {
                editor.move_cursor_down();
            } else {
                editor.move_cursor_right();
            }
        }
        KeyCode::Backspace => {
            editor.search.search_buffer.pop();
            if !editor.search.search_buffer.is_empty() {
                let search_term = editor.search.search_buffer.clone();
                if editor.perform_find(&search_term) {
                    let matches_count = editor.search.search_matches.len();
                    let current = editor.search.current_match_index.map(|i| i + 1).unwrap_or(1);
                    editor.status_message = format!(
                        "Find: {search_term} ({current}/{matches_count} matches) - Use \u{2191}\u{2193} to navigate, Enter/Esc to exit"
                    );
                } else {
                    editor.status_message =
                        format!("Find: {} (no matches)", editor.search.search_buffer);
                }
            } else {
                editor.search.find_navigation_mode = FindNavigationMode::HistoryBrowsing;
                editor.status_message = "Find: ".to_string();
                editor.search.search_matches.clear();
                editor.search.current_match_index = None;
                editor.needs_redraw = true;
            }
        }
        KeyCode::Char(c) => {
            editor.search.search_buffer.push(c);
            editor.search.find_navigation_mode = FindNavigationMode::ResultNavigation;
            let search_term = editor.search.search_buffer.clone();
            if editor.perform_find(&search_term) {
                let matches_count = editor.search.search_matches.len();
                let current = editor.search.current_match_index.map(|i| i + 1).unwrap_or(1);
                editor.status_message = format!(
                    "Find: {search_term} ({current}/{matches_count} matches) - Use \u{2191}\u{2193} to navigate, Enter/Esc to exit"
                );
                editor.needs_redraw = true;
            } else {
                editor.status_message =
                    format!("Find: {} (no matches)", editor.search.search_buffer);
                editor.needs_redraw = true;
            }
        }
        _ => {}
    }
    Ok(false)
}

fn handle_replace(editor: &mut Editor, key: KeyEvent) -> Result<bool> {
    match key.code {
        KeyCode::Char('o') if key.modifiers == KeyModifiers::CONTROL => {
            editor.input_mode = InputMode::FindOptionsMenu;
            editor.needs_redraw = true;
        }
        KeyCode::Enter => match editor.search.replace_phase {
            ReplacePhase::FindPattern => {
                editor.search.replace_phase = ReplacePhase::ReplaceWith;
                editor.status_message =
                    format!("Replace '{}' with: ", editor.search.search_buffer);
                editor.needs_redraw = true;
            }
            ReplacePhase::ReplaceWith => {
                editor.input_mode = InputMode::ReplaceConfirm;
                editor.status_message = format!(
                    "Replace '{}' with '{}'? Y: Replace This | N: Skip | A: Replace All | ^C: Cancel",
                    editor.search.search_buffer, editor.search.replace_buffer
                );
                editor.needs_redraw = true;
            }
        },
        KeyCode::Esc => {
            editor.input_mode = InputMode::Normal;
            editor.status_message = "Replace cancelled".to_string();
            editor.needs_redraw = true;
        }
        KeyCode::Char('c') if key.modifiers == KeyModifiers::CONTROL => {
            editor.input_mode = InputMode::Normal;
            editor.status_message = "Replace cancelled".to_string();
            editor.needs_redraw = true;
        }
        KeyCode::Backspace => match editor.search.replace_phase {
            ReplacePhase::FindPattern => {
                editor.search.search_buffer.pop();
                editor.status_message = format!("Find: {}", editor.search.search_buffer);
                editor.needs_redraw = true;
            }
            ReplacePhase::ReplaceWith => {
                editor.search.replace_buffer.pop();
                editor.status_message = format!(
                    "Replace '{}' with: {}",
                    editor.search.search_buffer, editor.search.replace_buffer
                );
                editor.needs_redraw = true;
            }
        },
        KeyCode::Char(c) => match editor.search.replace_phase {
            ReplacePhase::FindPattern => {
                editor.search.search_buffer.push(c);
                editor.status_message = format!("Find: {}", editor.search.search_buffer);
                editor.needs_redraw = true;
            }
            ReplacePhase::ReplaceWith => {
                editor.search.replace_buffer.push(c);
                editor.status_message = format!(
                    "Replace '{}' with: {}",
                    editor.search.search_buffer, editor.search.replace_buffer
                );
                editor.needs_redraw = true;
            }
        },
        _ => {}
    }
    Ok(false)
}

fn handle_replace_confirm(editor: &mut Editor, key: KeyEvent) -> Result<bool> {
    match key.code {
        KeyCode::Char('y') | KeyCode::Char('Y') => {
            let replacements = editor.perform_replace_interactive(
                &editor.search.search_buffer.clone(),
                &editor.search.replace_buffer.clone(),
            );
            if replacements > 0 {
                editor.status_message =
                    "Replaced 1. Continue? Y: Replace This | N: Skip | A: Replace All | ^C: Cancel"
                        .to_string();
            } else {
                editor.set_temporary_status_message("No more matches found".to_string());
                editor.input_mode = InputMode::Normal;
            }
        }
        KeyCode::Char('n') | KeyCode::Char('N') => {
            editor.input_mode = InputMode::Normal;
            editor.set_temporary_status_message("Replace skipped".to_string());
        }
        KeyCode::Char('a') | KeyCode::Char('A') => {
            let replacements = editor.perform_replace(
                &editor.search.search_buffer.clone(),
                &editor.search.replace_buffer.clone(),
            );
            if replacements > 0 {
                editor.set_temporary_status_message(format!(
                    "Replaced all {replacements} occurrence(s)"
                ));
            } else {
                editor.set_temporary_status_message("No matches found".to_string());
            }
            editor.input_mode = InputMode::Normal;
        }
        KeyCode::Esc => {
            editor.input_mode = InputMode::Normal;
            editor.set_temporary_status_message("Replace cancelled".to_string());
        }
        _ if key.modifiers == KeyModifiers::CONTROL && key.code == KeyCode::Char('c') => {
            editor.input_mode = InputMode::Normal;
            editor.set_temporary_status_message("Replace cancelled".to_string());
        }
        _ => {}
    }
    Ok(false)
}

fn handle_goto_line(editor: &mut Editor, key: KeyEvent) -> Result<bool> {
    match key.code {
        KeyCode::Enter => {
            if let Ok(line_num) = editor.search.goto_line_buffer.parse::<usize>() {
                editor.goto_line(line_num);
            } else {
                editor.set_temporary_status_message("Invalid line number".to_string());
            }
            editor.input_mode = InputMode::Normal;
        }
        KeyCode::Esc => {
            editor.input_mode = InputMode::Normal;
            editor.status_message = "Go to line cancelled".to_string();
            editor.needs_redraw = true;
        }
        KeyCode::Char('c') if key.modifiers == KeyModifiers::CONTROL => {
            editor.input_mode = InputMode::Normal;
            editor.status_message = "Go to line cancelled".to_string();
            editor.needs_redraw = true;
        }
        KeyCode::Backspace => {
            editor.search.goto_line_buffer.pop();
            editor.status_message = format!("Go to line: {}", editor.search.goto_line_buffer);
            editor.needs_redraw = true;
        }
        KeyCode::Char(c) if c.is_ascii_digit() => {
            editor.search.goto_line_buffer.push(c);
            editor.status_message = format!("Go to line: {}", editor.search.goto_line_buffer);
            editor.needs_redraw = true;
        }
        _ => {}
    }
    Ok(false)
}

fn handle_help(editor: &mut Editor, key: KeyEvent) -> Result<bool> {
    match key.code {
        KeyCode::Esc => {
            editor.input_mode = InputMode::Normal;
            editor.needs_redraw = true;
        }
        KeyCode::Char('c') if key.modifiers == KeyModifiers::CONTROL => {
            editor.input_mode = InputMode::Normal;
            editor.needs_redraw = true;
        }
        KeyCode::Char('h') if key.modifiers == KeyModifiers::CONTROL => {
            editor.input_mode = InputMode::Normal;
            editor.needs_redraw = true;
        }
        _ => {}
    }
    Ok(false)
}

fn handle_normal(editor: &mut Editor, key: KeyEvent) -> Result<bool> {
    match (key.modifiers, key.code) {
        (KeyModifiers::CONTROL, KeyCode::Char('q')) => return Ok(editor.try_quit()),
        (KeyModifiers::CONTROL, KeyCode::Char('x')) => return Ok(editor.try_quit()),
        (KeyModifiers::CONTROL, KeyCode::Char('s')) => {
            editor.save_file()?;
        }
        (KeyModifiers::CONTROL, KeyCode::Char('w')) => {
            editor.save_as();
        }
        (KeyModifiers::CONTROL, KeyCode::Char('v')) => {
            editor.page_down();
        }
        (KeyModifiers::CONTROL, KeyCode::Char('o')) => {
            editor.open_options_menu();
        }
        (KeyModifiers::CONTROL, KeyCode::Char('h')) => {
            editor.input_mode = InputMode::Help;
            editor.needs_redraw = true;
        }
        (KeyModifiers::CONTROL, KeyCode::Char('z')) => {
            editor.undo();
        }
        (KeyModifiers::CONTROL, KeyCode::Char('y')) => {
            editor.page_up();
        }
        (KeyModifiers::CONTROL, KeyCode::Char('f')) => {
            editor.start_find();
        }
        (KeyModifiers::CONTROL, KeyCode::Char('\\')) => {
            editor.start_replace();
        }
        (KeyModifiers::CONTROL, KeyCode::Char('g')) => {
            editor.start_goto_line();
        }
        (KeyModifiers::CONTROL, KeyCode::Char('r')) => {
            editor.redo();
        }

        // Navigation
        (_, KeyCode::Up) => editor.move_cursor_up(),
        (_, KeyCode::Down) => editor.move_cursor_down(),
        (_, KeyCode::Left) => editor.move_cursor_left(),
        (_, KeyCode::Right) => editor.move_cursor_right(),
        (_, KeyCode::PageUp) => editor.page_up(),
        (_, KeyCode::PageDown) => editor.page_down(),
        (_, KeyCode::Home) => editor.viewport.cursor_pos.1 = 0,
        (_, KeyCode::End) => {
            if let Some(line) = editor.rope.line(editor.viewport.cursor_pos.0).as_str() {
                editor.viewport.cursor_pos.1 = line.trim_end_matches('\n').width();
            }
        }

        // Editing
        (_, KeyCode::Char(c)) => editor.insert_char(c),
        (_, KeyCode::Tab) => {
            editor.handle_tab_insertion();
        }
        (_, KeyCode::Enter) => editor.insert_newline(),
        (_, KeyCode::Backspace) => editor.delete_char(),
        (_, KeyCode::Esc) => {}

        _ => {}
    }

    Ok(false)
}
