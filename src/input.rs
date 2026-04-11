use anyhow::Result;
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

use crate::editor::InputMode;
use crate::search::{FindNavigationMode, ReplacePhase};
use crate::tabs::TabManager;

pub fn handle_key_event(tabs: &mut TabManager, key: KeyEvent) -> Result<bool> {
    match tabs.input_mode {
        InputMode::ConfirmQuit => handle_confirm_quit(tabs, key),
        InputMode::ConfirmCloseTab => handle_confirm_close_tab(tabs, key),
        InputMode::OptionsMenu => handle_options_menu(tabs, key),
        InputMode::FindOptionsMenu => handle_find_options_menu(tabs, key),
        InputMode::EnteringFilename | InputMode::EnteringSaveAs => handle_filename_input(tabs, key),
        InputMode::OpenFileCurrentTab | InputMode::OpenFileNewTab => {
            handle_open_file_input(tabs, key)
        }
        InputMode::Find => handle_find(tabs, key),
        InputMode::Replace => handle_replace(tabs, key),
        InputMode::ReplaceConfirm => handle_replace_confirm(tabs, key),
        InputMode::GoToLine => handle_goto_line(tabs, key),
        InputMode::HexView => handle_hex_view(tabs, key),
        InputMode::FuzzyFinder => handle_fuzzy_finder(tabs, key),
        InputMode::VerbatimInput => handle_verbatim_input(tabs, key),
        InputMode::ExecuteCommand => handle_execute_command(tabs, key),
        InputMode::ConfirmExecute => handle_confirm_execute(tabs, key),
        InputMode::Normal => handle_normal(tabs, key),
    }
}

fn handle_hex_view(tabs: &mut TabManager, key: KeyEvent) -> Result<bool> {
    use crate::hex::BYTES_PER_ROW;

    let byte_count = tabs
        .active_editor()
        .hex_state
        .as_ref()
        .map(|s| s.raw_bytes.len())
        .unwrap_or(0);

    if byte_count == 0 {
        match key.code {
            KeyCode::Esc => tabs.toggle_hex_view(),
            _ if key.modifiers == KeyModifiers::CONTROL && key.code == KeyCode::Char('b') => {
                tabs.toggle_hex_view();
            }
            _ => {}
        }
        return Ok(false);
    }

    let max_cursor = byte_count.saturating_sub(1);

    match key.code {
        KeyCode::Left => {
            if let Some(state) = &mut tabs.active_editor_mut().hex_state {
                state.cursor = state.cursor.saturating_sub(1);
                tabs.needs_redraw = true;
            }
        }
        KeyCode::Right => {
            if let Some(state) = &mut tabs.active_editor_mut().hex_state {
                state.cursor = (state.cursor + 1).min(max_cursor);
                tabs.needs_redraw = true;
            }
        }
        KeyCode::Up => {
            if let Some(state) = &mut tabs.active_editor_mut().hex_state {
                state.cursor = state.cursor.saturating_sub(BYTES_PER_ROW);
                tabs.needs_redraw = true;
            }
        }
        KeyCode::Down => {
            if let Some(state) = &mut tabs.active_editor_mut().hex_state {
                state.cursor = (state.cursor + BYTES_PER_ROW).min(max_cursor);
                tabs.needs_redraw = true;
            }
        }
        KeyCode::PageUp => {
            if let Some(state) = &mut tabs.active_editor_mut().hex_state {
                let page = 20 * BYTES_PER_ROW;
                state.cursor = state.cursor.saturating_sub(page);
                tabs.needs_redraw = true;
            }
        }
        KeyCode::PageDown => {
            if let Some(state) = &mut tabs.active_editor_mut().hex_state {
                let page = 20 * BYTES_PER_ROW;
                state.cursor = (state.cursor + page).min(max_cursor);
                tabs.needs_redraw = true;
            }
        }
        KeyCode::Esc => {
            tabs.toggle_hex_view();
        }
        _ if key.modifiers == KeyModifiers::CONTROL && key.code == KeyCode::Char('b') => {
            tabs.toggle_hex_view();
        }
        _ => {}
    }

    // Keep cursor row visible
    if let Some(state) = &mut tabs.active_editor_mut().hex_state {
        let cursor_row = state.cursor / BYTES_PER_ROW;
        if cursor_row < state.scroll_offset {
            state.scroll_offset = cursor_row;
        }
        let visible_rows = 20;
        if cursor_row >= state.scroll_offset + visible_rows {
            state.scroll_offset = cursor_row.saturating_sub(visible_rows - 1);
        }
    }

    Ok(false)
}

fn handle_confirm_quit(tabs: &mut TabManager, key: KeyEvent) -> Result<bool> {
    match key.code {
        KeyCode::Char('y') | KeyCode::Char('Y') => {
            return tabs.handle_quit_confirmation(true);
        }
        KeyCode::Char('n') | KeyCode::Char('N') => {
            return tabs.handle_quit_confirmation(false);
        }
        KeyCode::Esc => {
            tabs.cancel_quit_confirmation();
        }
        _ if key.modifiers == KeyModifiers::CONTROL && key.code == KeyCode::Char('c') => {
            tabs.cancel_quit_confirmation();
        }
        _ => {}
    }
    Ok(false)
}

fn handle_confirm_close_tab(tabs: &mut TabManager, key: KeyEvent) -> Result<bool> {
    match key.code {
        KeyCode::Char('y') | KeyCode::Char('Y') => {
            let path = tabs.active_editor().file_path.clone();
            if let Some(path) = path {
                if let Err(e) = tabs.perform_save(path) {
                    tabs.set_temporary_status_message(format!("Error saving file: {e}"));
                    return Ok(false);
                }
            }
            tabs.input_mode = InputMode::Normal;
            if tabs.close_tab() {
                return Ok(true);
            }
        }
        KeyCode::Char('n') | KeyCode::Char('N') => {
            tabs.input_mode = InputMode::Normal;
            if tabs.close_tab() {
                return Ok(true);
            }
        }
        KeyCode::Esc => {
            tabs.input_mode = InputMode::Normal;
            tabs.set_temporary_status_message("Cancelled".to_string());
        }
        _ if key.modifiers == KeyModifiers::CONTROL && key.code == KeyCode::Char('c') => {
            tabs.input_mode = InputMode::Normal;
            tabs.set_temporary_status_message("Cancelled".to_string());
        }
        _ => {}
    }
    Ok(false)
}

fn handle_options_menu(tabs: &mut TabManager, key: KeyEvent) -> Result<bool> {
    match key.code {
        KeyCode::Char('m') | KeyCode::Char('M') => {
            tabs.toggle_mouse_mode();
            tabs.input_mode = InputMode::Normal;
            tabs.needs_redraw = true;
        }
        KeyCode::Char('l') | KeyCode::Char('L') => {
            tabs.config.show_line_numbers = !tabs.config.show_line_numbers;
            tabs.set_temporary_status_message(format!(
                "Line numbers: {}",
                if tabs.config.show_line_numbers {
                    "ON"
                } else {
                    "OFF"
                }
            ));
            tabs.input_mode = InputMode::Normal;
            tabs.needs_redraw = true;
        }
        KeyCode::Char('w') | KeyCode::Char('W') => {
            tabs.config.word_wrap = !tabs.config.word_wrap;
            tabs.set_temporary_status_message(format!(
                "Word wrap: {}",
                if tabs.config.word_wrap { "ON" } else { "OFF" }
            ));
            tabs.input_mode = InputMode::Normal;
            tabs.needs_redraw = true;
        }
        KeyCode::Char('t') | KeyCode::Char('T') => {
            tabs.config.tab_width = match tabs.config.tab_width {
                2 => 4,
                4 => 8,
                _ => 2,
            };
            tabs.set_temporary_status_message(format!("Tab width: {}", tabs.config.tab_width));
            tabs.input_mode = InputMode::Normal;
            tabs.needs_redraw = true;
        }
        KeyCode::Char('i') | KeyCode::Char('I') => {
            tabs.config.auto_indent = !tabs.config.auto_indent;
            tabs.set_temporary_status_message(format!(
                "Auto-indent: {}",
                if tabs.config.auto_indent { "ON" } else { "OFF" }
            ));
            tabs.input_mode = InputMode::Normal;
            tabs.needs_redraw = true;
        }
        KeyCode::Char('p') | KeyCode::Char('P') => {
            tabs.config.show_whitespace = !tabs.config.show_whitespace;
            tabs.set_temporary_status_message(format!(
                "Whitespace display: {}",
                if tabs.config.show_whitespace {
                    "ON"
                } else {
                    "OFF"
                }
            ));
            tabs.input_mode = InputMode::Normal;
            tabs.needs_redraw = true;
        }
        KeyCode::Char('o') | KeyCode::Char('O') => {
            tabs.input_mode = InputMode::OpenFileCurrentTab;
            tabs.filename_buffer.clear();
            tabs.status_message = "Open file: ".to_string();
            tabs.needs_redraw = true;
        }
        KeyCode::Char('n') | KeyCode::Char('N') => {
            tabs.input_mode = InputMode::OpenFileNewTab;
            tabs.filename_buffer.clear();
            tabs.status_message = "Open in new tab: ".to_string();
            tabs.needs_redraw = true;
        }
        KeyCode::Char('b') | KeyCode::Char('B') => {
            tabs.config.backup_on_save = !tabs.config.backup_on_save;
            tabs.set_temporary_status_message(format!(
                "Backup on save: {}",
                if tabs.config.backup_on_save {
                    "ON"
                } else {
                    "OFF"
                }
            ));
            tabs.input_mode = InputMode::Normal;
            tabs.needs_redraw = true;
        }
        KeyCode::Esc => {
            tabs.save_config();
            tabs.input_mode = InputMode::Normal;
            tabs.status_message.clear();
            tabs.needs_redraw = true;
        }
        _ if key.modifiers == KeyModifiers::CONTROL && key.code == KeyCode::Char('c') => {
            tabs.save_config();
            tabs.input_mode = InputMode::Normal;
            tabs.status_message.clear();
            tabs.needs_redraw = true;
        }
        _ => {}
    }
    Ok(false)
}

fn handle_find_options_menu(tabs: &mut TabManager, key: KeyEvent) -> Result<bool> {
    match key.code {
        KeyCode::Char('c') | KeyCode::Char('C') => {
            tabs.toggle_case_sensitive();
            tabs.set_temporary_status_message(format!(
                "Case sensitivity: {}",
                if tabs.active_editor().search.case_sensitive {
                    "ON"
                } else {
                    "OFF"
                }
            ));
            tabs.input_mode = InputMode::Find;
        }
        KeyCode::Char('r') | KeyCode::Char('R') => {
            tabs.toggle_regex_mode();
            tabs.input_mode = InputMode::Find;
        }
        KeyCode::Esc => {
            tabs.input_mode = InputMode::Find;
        }
        _ if key.modifiers == KeyModifiers::CONTROL && key.code == KeyCode::Char('c') => {
            tabs.input_mode = InputMode::Find;
        }
        _ => {}
    }
    Ok(false)
}

fn handle_filename_input(tabs: &mut TabManager, key: KeyEvent) -> Result<bool> {
    match key.code {
        KeyCode::Enter => {
            return tabs.finish_filename_input();
        }
        KeyCode::Esc => {
            tabs.cancel_filename_input();
        }
        KeyCode::Backspace => {
            tabs.filename_buffer.pop();
            tabs.status_message = format!("File Name to Write: {}", tabs.filename_buffer);
            tabs.needs_redraw = true;
        }
        KeyCode::Char(c) => {
            tabs.filename_buffer.push(c);
            tabs.status_message = format!("File Name to Write: {}", tabs.filename_buffer);
            tabs.needs_redraw = true;
        }
        _ => {}
    }
    Ok(false)
}

fn handle_open_file_input(tabs: &mut TabManager, key: KeyEvent) -> Result<bool> {
    let is_new_tab = tabs.input_mode == InputMode::OpenFileNewTab;
    let prompt = if is_new_tab {
        "Open in new tab: "
    } else {
        "Open file: "
    };
    match key.code {
        KeyCode::Enter => {
            if tabs.filename_buffer.is_empty() {
                tabs.input_mode = InputMode::Normal;
                tabs.set_temporary_status_message("Cancelled".to_string());
                return Ok(false);
            }
            let path = std::path::PathBuf::from(&tabs.filename_buffer);
            if is_new_tab {
                match tabs.open_in_new_tab(path) {
                    Ok(()) => {
                        tabs.resolve_display_names();
                        tabs.set_temporary_status_message("Opened in new tab".to_string());
                    }
                    Err(e) => {
                        tabs.set_temporary_status_message(format!("Error: {e}"));
                    }
                }
            } else {
                match tabs.open_in_current_tab(path) {
                    Ok(()) => {
                        tabs.resolve_display_names();
                        tabs.set_temporary_status_message("File opened".to_string());
                    }
                    Err(e) => {
                        tabs.set_temporary_status_message(format!("Error: {e}"));
                    }
                }
            }
            tabs.input_mode = InputMode::Normal;
            tabs.filename_buffer.clear();
        }
        KeyCode::Esc => {
            tabs.input_mode = InputMode::Normal;
            tabs.filename_buffer.clear();
            tabs.set_temporary_status_message("Cancelled".to_string());
        }
        KeyCode::Backspace => {
            tabs.filename_buffer.pop();
            tabs.status_message = format!("{}{}", prompt, tabs.filename_buffer);
            tabs.needs_redraw = true;
        }
        KeyCode::Char(c) => {
            tabs.filename_buffer.push(c);
            tabs.status_message = format!("{}{}", prompt, tabs.filename_buffer);
            tabs.needs_redraw = true;
        }
        _ => {}
    }
    Ok(false)
}

fn handle_find(tabs: &mut TabManager, key: KeyEvent) -> Result<bool> {
    match key.code {
        KeyCode::Char('r') if key.modifiers == KeyModifiers::CONTROL => {
            if !tabs.active_editor().search.search_buffer.is_empty() {
                tabs.input_mode = InputMode::Replace;
                tabs.active_editor_mut().search.replace_buffer.clear();
                tabs.active_editor_mut().search.replace_phase = ReplacePhase::ReplaceWith;
                let search_buf = tabs.active_editor().search.search_buffer.clone();
                tabs.status_message = format!("Replace '{}' with: ", search_buf);
                tabs.needs_redraw = true;
            } else {
                tabs.toggle_regex_mode();
            }
        }
        KeyCode::Char('o') if key.modifiers == KeyModifiers::CONTROL => {
            tabs.input_mode = InputMode::FindOptionsMenu;
            tabs.needs_redraw = true;
        }
        KeyCode::Enter => {
            let search_term = tabs.active_editor().search.search_buffer.clone();
            tabs.active_editor_mut()
                .search
                .add_to_search_history(&search_term);

            if tabs.active_editor().search.find_navigation_mode
                == FindNavigationMode::ResultNavigation
                && !tabs.active_editor().search.search_matches.is_empty()
            {
                tabs.input_mode = InputMode::Normal;
                let editor = tabs.active_editor_mut();
                editor.search.search_matches.clear();
                editor.search.current_match_index = None;
                editor.search.search_buffer.clear();
                tabs.set_temporary_status_message("Search completed".to_string());
            } else {
                let search_term = tabs.active_editor().search.search_buffer.clone();
                if tabs.active_editor_mut().perform_find(&search_term) {
                    tabs.active_editor_mut().search.find_navigation_mode =
                        FindNavigationMode::ResultNavigation;
                    let matches_count = tabs.active_editor().search.search_matches.len();
                    let current = tabs
                        .active_editor()
                        .search
                        .current_match_index
                        .map(|i| i + 1)
                        .unwrap_or(1);
                    tabs.status_message = format!(
                        "Find: {search_term} ({current}/{matches_count} matches) - Use \u{2191}\u{2193} to navigate, Enter/Esc to exit"
                    );
                    tabs.needs_redraw = true;
                } else {
                    tabs.set_temporary_status_message("Not found".to_string());
                    tabs.input_mode = InputMode::Normal;
                }
            }
        }
        KeyCode::Esc => {
            tabs.active_editor_mut().cancel_search();
            tabs.input_mode = InputMode::Normal;
            tabs.set_temporary_status_message("Search cancelled".to_string());
        }
        KeyCode::Char('c') if key.modifiers == KeyModifiers::CONTROL => {
            tabs.active_editor_mut().cancel_search();
            tabs.input_mode = InputMode::Normal;
            tabs.set_temporary_status_message("Search cancelled".to_string());
        }
        KeyCode::Up | KeyCode::Left => {
            if key.code == KeyCode::Up
                && tabs.active_editor().search.find_navigation_mode
                    == FindNavigationMode::HistoryBrowsing
            {
                if tabs.active_editor_mut().search.navigate_search_history_up() {
                    let search_buf = tabs.active_editor().search.search_buffer.clone();
                    tabs.status_message = format!("Find: {}", search_buf);
                    tabs.needs_redraw = true;
                    if !tabs.active_editor().search.search_buffer.is_empty() {
                        let search_term = tabs.active_editor().search.search_buffer.clone();
                        tabs.active_editor_mut().perform_find(&search_term);
                    }
                } else {
                    tabs.active_editor_mut().move_cursor_up();
                    tabs.needs_redraw = true;
                }
            } else if tabs.active_editor().search.find_navigation_mode
                == FindNavigationMode::ResultNavigation
                && !tabs.active_editor().search.search_matches.is_empty()
            {
                tabs.active_editor_mut().find_previous_match();
                let matches_count = tabs.active_editor().search.search_matches.len();
                let current = tabs
                    .active_editor()
                    .search
                    .current_match_index
                    .map(|i| i + 1)
                    .unwrap_or(1);
                let search_buf = tabs.active_editor().search.search_buffer.clone();
                tabs.status_message = format!(
                    "Find: {} ({current}/{matches_count} matches) - Use arrows to navigate, Enter/Esc to exit",
                    search_buf
                );
                tabs.needs_redraw = true;
            } else if key.code == KeyCode::Up {
                tabs.active_editor_mut().move_cursor_up();
                tabs.needs_redraw = true;
            } else {
                tabs.active_editor_mut().move_cursor_left();
                tabs.needs_redraw = true;
            }
        }
        KeyCode::Down | KeyCode::Right => {
            if key.code == KeyCode::Down
                && tabs.active_editor().search.find_navigation_mode
                    == FindNavigationMode::HistoryBrowsing
            {
                if tabs
                    .active_editor_mut()
                    .search
                    .navigate_search_history_down()
                {
                    let search_buf = tabs.active_editor().search.search_buffer.clone();
                    tabs.status_message = format!("Find: {}", search_buf);
                    tabs.needs_redraw = true;
                    if !tabs.active_editor().search.search_buffer.is_empty() {
                        let search_term = tabs.active_editor().search.search_buffer.clone();
                        tabs.active_editor_mut().perform_find(&search_term);
                    } else {
                        let editor = tabs.active_editor_mut();
                        editor.search.search_matches.clear();
                        editor.search.current_match_index = None;
                    }
                } else {
                    tabs.active_editor_mut().move_cursor_down();
                    tabs.needs_redraw = true;
                }
            } else if tabs.active_editor().search.find_navigation_mode
                == FindNavigationMode::ResultNavigation
                && !tabs.active_editor().search.search_matches.is_empty()
            {
                tabs.active_editor_mut().find_next_match();
                let matches_count = tabs.active_editor().search.search_matches.len();
                let current = tabs
                    .active_editor()
                    .search
                    .current_match_index
                    .map(|i| i + 1)
                    .unwrap_or(1);
                let search_buf = tabs.active_editor().search.search_buffer.clone();
                tabs.status_message = format!(
                    "Find: {} ({current}/{matches_count} matches) - Use arrows to navigate, Enter/Esc to exit",
                    search_buf
                );
                tabs.needs_redraw = true;
            } else if key.code == KeyCode::Down {
                tabs.active_editor_mut().move_cursor_down();
                tabs.needs_redraw = true;
            } else {
                tabs.active_editor_mut().move_cursor_right();
                tabs.needs_redraw = true;
            }
        }
        KeyCode::Backspace => {
            tabs.active_editor_mut().search.search_buffer.pop();
            if !tabs.active_editor().search.search_buffer.is_empty() {
                let search_term = tabs.active_editor().search.search_buffer.clone();
                if tabs.active_editor_mut().perform_find(&search_term) {
                    let matches_count = tabs.active_editor().search.search_matches.len();
                    let current = tabs
                        .active_editor()
                        .search
                        .current_match_index
                        .map(|i| i + 1)
                        .unwrap_or(1);
                    tabs.status_message = format!(
                        "Find: {search_term} ({current}/{matches_count} matches) - Use \u{2191}\u{2193} to navigate, Enter/Esc to exit"
                    );
                } else {
                    let search_buf = tabs.active_editor().search.search_buffer.clone();
                    tabs.status_message = format!("Find: {} (no matches)", search_buf);
                }
            } else {
                tabs.active_editor_mut().search.find_navigation_mode =
                    FindNavigationMode::HistoryBrowsing;
                tabs.status_message = "Find: ".to_string();
                let editor = tabs.active_editor_mut();
                editor.search.search_matches.clear();
                editor.search.current_match_index = None;
                tabs.needs_redraw = true;
            }
        }
        KeyCode::Char(c) => {
            tabs.active_editor_mut().search.search_buffer.push(c);
            tabs.active_editor_mut().search.find_navigation_mode =
                FindNavigationMode::ResultNavigation;
            let search_term = tabs.active_editor().search.search_buffer.clone();
            if tabs.active_editor_mut().perform_find(&search_term) {
                let matches_count = tabs.active_editor().search.search_matches.len();
                let current = tabs
                    .active_editor()
                    .search
                    .current_match_index
                    .map(|i| i + 1)
                    .unwrap_or(1);
                tabs.status_message = format!(
                    "Find: {search_term} ({current}/{matches_count} matches) - Use \u{2191}\u{2193} to navigate, Enter/Esc to exit"
                );
                tabs.needs_redraw = true;
            } else {
                let search_buf = tabs.active_editor().search.search_buffer.clone();
                tabs.status_message = format!("Find: {} (no matches)", search_buf);
                tabs.needs_redraw = true;
            }
        }
        _ => {}
    }
    Ok(false)
}

fn handle_replace(tabs: &mut TabManager, key: KeyEvent) -> Result<bool> {
    match key.code {
        KeyCode::Char('o') if key.modifiers == KeyModifiers::CONTROL => {
            tabs.input_mode = InputMode::FindOptionsMenu;
            tabs.needs_redraw = true;
        }
        KeyCode::Enter => match tabs.active_editor().search.replace_phase {
            ReplacePhase::FindPattern => {
                tabs.active_editor_mut().search.replace_phase = ReplacePhase::ReplaceWith;
                let search_buf = tabs.active_editor().search.search_buffer.clone();
                tabs.status_message = format!("Replace '{}' with: ", search_buf);
                tabs.needs_redraw = true;
            }
            ReplacePhase::ReplaceWith => {
                tabs.input_mode = InputMode::ReplaceConfirm;
                let search_buf = tabs.active_editor().search.search_buffer.clone();
                let replace_buf = tabs.active_editor().search.replace_buffer.clone();
                tabs.status_message = format!(
                    "Replace '{}' with '{}'? Y: Replace This | N: Skip | A: Replace All | ^C: Cancel",
                    search_buf, replace_buf
                );
                tabs.needs_redraw = true;
            }
        },
        KeyCode::Esc => {
            tabs.input_mode = InputMode::Normal;
            tabs.set_temporary_status_message("Replace cancelled".to_string());
        }
        KeyCode::Char('c') if key.modifiers == KeyModifiers::CONTROL => {
            tabs.input_mode = InputMode::Normal;
            tabs.set_temporary_status_message("Replace cancelled".to_string());
        }
        KeyCode::Backspace => match tabs.active_editor().search.replace_phase {
            ReplacePhase::FindPattern => {
                tabs.active_editor_mut().search.search_buffer.pop();
                let search_buf = tabs.active_editor().search.search_buffer.clone();
                tabs.status_message = format!("Find: {}", search_buf);
                tabs.needs_redraw = true;
            }
            ReplacePhase::ReplaceWith => {
                tabs.active_editor_mut().search.replace_buffer.pop();
                let search_buf = tabs.active_editor().search.search_buffer.clone();
                let replace_buf = tabs.active_editor().search.replace_buffer.clone();
                tabs.status_message = format!("Replace '{}' with: {}", search_buf, replace_buf);
                tabs.needs_redraw = true;
            }
        },
        KeyCode::Char(c) => match tabs.active_editor().search.replace_phase {
            ReplacePhase::FindPattern => {
                tabs.active_editor_mut().search.search_buffer.push(c);
                let search_buf = tabs.active_editor().search.search_buffer.clone();
                tabs.status_message = format!("Find: {}", search_buf);
                tabs.needs_redraw = true;
            }
            ReplacePhase::ReplaceWith => {
                tabs.active_editor_mut().search.replace_buffer.push(c);
                let search_buf = tabs.active_editor().search.search_buffer.clone();
                let replace_buf = tabs.active_editor().search.replace_buffer.clone();
                tabs.status_message = format!("Replace '{}' with: {}", search_buf, replace_buf);
                tabs.needs_redraw = true;
            }
        },
        _ => {}
    }
    Ok(false)
}

fn handle_replace_confirm(tabs: &mut TabManager, key: KeyEvent) -> Result<bool> {
    match key.code {
        KeyCode::Char('y') | KeyCode::Char('Y') => {
            let search_buf = tabs.active_editor().search.search_buffer.clone();
            let replace_buf = tabs.active_editor().search.replace_buffer.clone();
            let replacements = tabs
                .active_editor_mut()
                .perform_replace_interactive(&search_buf, &replace_buf);
            if replacements > 0 {
                tabs.status_message =
                    "Replaced 1. Continue? Y: Replace This | N: Skip | A: Replace All | ^C: Cancel"
                        .to_string();
            } else {
                tabs.set_temporary_status_message("No more matches found".to_string());
                tabs.input_mode = InputMode::Normal;
            }
            tabs.needs_redraw = true;
        }
        KeyCode::Char('n') | KeyCode::Char('N') => {
            tabs.input_mode = InputMode::Normal;
            tabs.set_temporary_status_message("Replace skipped".to_string());
        }
        KeyCode::Char('a') | KeyCode::Char('A') => {
            let search_buf = tabs.active_editor().search.search_buffer.clone();
            let replace_buf = tabs.active_editor().search.replace_buffer.clone();
            let replacements = tabs
                .active_editor_mut()
                .perform_replace(&search_buf, &replace_buf);
            if replacements > 0 {
                tabs.set_temporary_status_message(format!(
                    "Replaced all {replacements} occurrence(s)"
                ));
            } else {
                tabs.set_temporary_status_message("No matches found".to_string());
            }
            tabs.input_mode = InputMode::Normal;
        }
        KeyCode::Esc => {
            tabs.input_mode = InputMode::Normal;
            tabs.set_temporary_status_message("Replace cancelled".to_string());
        }
        _ if key.modifiers == KeyModifiers::CONTROL && key.code == KeyCode::Char('c') => {
            tabs.input_mode = InputMode::Normal;
            tabs.set_temporary_status_message("Replace cancelled".to_string());
        }
        _ => {}
    }
    Ok(false)
}

fn handle_goto_line(tabs: &mut TabManager, key: KeyEvent) -> Result<bool> {
    match key.code {
        KeyCode::Enter => {
            let line_buf = tabs.active_editor().search.goto_line_buffer.clone();
            if let Ok(line_num) = line_buf.parse::<usize>() {
                tabs.goto_line(line_num);
            } else {
                tabs.set_temporary_status_message("Invalid line number".to_string());
            }
            tabs.input_mode = InputMode::Normal;
        }
        KeyCode::Esc => {
            tabs.input_mode = InputMode::Normal;
            tabs.set_temporary_status_message("Go to line cancelled".to_string());
        }
        KeyCode::Char('c') if key.modifiers == KeyModifiers::CONTROL => {
            tabs.input_mode = InputMode::Normal;
            tabs.set_temporary_status_message("Go to line cancelled".to_string());
        }
        KeyCode::Backspace => {
            tabs.active_editor_mut().search.goto_line_buffer.pop();
            let line_buf = tabs.active_editor().search.goto_line_buffer.clone();
            tabs.status_message = format!("Go to line: {}", line_buf);
            tabs.needs_redraw = true;
        }
        KeyCode::Char(c) if c.is_ascii_digit() => {
            tabs.active_editor_mut().search.goto_line_buffer.push(c);
            let line_buf = tabs.active_editor().search.goto_line_buffer.clone();
            tabs.status_message = format!("Go to line: {}", line_buf);
            tabs.needs_redraw = true;
        }
        _ => {}
    }
    Ok(false)
}

fn handle_normal(tabs: &mut TabManager, key: KeyEvent) -> Result<bool> {
    let is_read_only = tabs.active_editor().display_name == "[Help]";

    // Reset cut accumulation for any key that isn't Ctrl+K
    if !(key.modifiers == KeyModifiers::CONTROL && key.code == KeyCode::Char('k')) {
        tabs.reset_cut_tracking();
    }

    match (key.modifiers, key.code) {
        (KeyModifiers::CONTROL, KeyCode::Char('q')) => return Ok(tabs.try_quit()),
        (KeyModifiers::CONTROL, KeyCode::Char('x')) => return Ok(tabs.try_quit()),
        (KeyModifiers::CONTROL, KeyCode::Char('s')) => {
            tabs.save_file()?;
        }
        (KeyModifiers::CONTROL, KeyCode::Char('w')) => {
            tabs.save_as();
        }
        (KeyModifiers::CONTROL, KeyCode::Char('v')) => {
            tabs.active_editor_mut().page_down();
            tabs.needs_redraw = true;
        }
        (KeyModifiers::CONTROL, KeyCode::Char('o')) => {
            tabs.open_options_menu();
        }
        (KeyModifiers::CONTROL, KeyCode::Char('h')) => {
            tabs.open_help_tab();
        }
        (KeyModifiers::CONTROL, KeyCode::Char('z')) if !is_read_only => {
            tabs.undo();
        }
        (KeyModifiers::CONTROL, KeyCode::Char('y')) => {
            tabs.active_editor_mut().page_up();
            tabs.needs_redraw = true;
        }
        (KeyModifiers::CONTROL, KeyCode::Char('f')) => {
            tabs.start_find();
        }
        (KeyModifiers::CONTROL, KeyCode::Char('\\')) => {
            tabs.start_replace();
        }
        (KeyModifiers::CONTROL, KeyCode::Char('g')) => {
            tabs.start_goto_line();
        }
        (KeyModifiers::CONTROL, KeyCode::Char('r')) if !is_read_only => {
            tabs.redo();
        }
        (KeyModifiers::CONTROL, KeyCode::Char('b')) => {
            tabs.toggle_hex_view();
        }
        (KeyModifiers::CONTROL, KeyCode::Char('t')) => {
            tabs.new_tab();
            tabs.set_temporary_status_message("New tab".to_string());
        }
        (KeyModifiers::ALT, KeyCode::Left) => {
            tabs.prev_tab();
        }
        (KeyModifiers::ALT, KeyCode::Right) => {
            tabs.next_tab();
        }
        (KeyModifiers::ALT, KeyCode::Char(',')) => {
            tabs.prev_tab();
        }
        (KeyModifiers::ALT, KeyCode::Char('.')) => {
            tabs.next_tab();
        }
        (KeyModifiers::ALT, KeyCode::Char('w')) => {
            if tabs.active_editor().modified {
                tabs.input_mode = InputMode::ConfirmCloseTab;
                tabs.status_message =
                    "Save modified buffer before closing? (Y/N/Ctrl+C)".to_string();
                tabs.needs_redraw = true;
            } else if tabs.close_tab() {
                return Ok(true);
            }
        }
        (KeyModifiers::CONTROL, KeyCode::Char('p')) => {
            tabs.input_mode = InputMode::FuzzyFinder;
            tabs.fuzzy_query.clear();
            tabs.fuzzy_selected = 0;
            tabs.needs_redraw = true;
        }
        (KeyModifiers::CONTROL, KeyCode::Char('k')) if !is_read_only => {
            tabs.cut();
        }
        (KeyModifiers::CONTROL, KeyCode::Char('u')) if !is_read_only => {
            tabs.paste_inline();
        }
        (KeyModifiers::ALT, KeyCode::Char('6')) => {
            tabs.copy();
        }
        (KeyModifiers::ALT, KeyCode::Char('a')) => {
            tabs.toggle_mark();
        }
        (KeyModifiers::ALT, KeyCode::Char('p')) => {
            tabs.config.show_whitespace = !tabs.config.show_whitespace;
            tabs.set_temporary_status_message(format!(
                "Whitespace display: {}",
                if tabs.config.show_whitespace {
                    "ON"
                } else {
                    "OFF"
                }
            ));
            tabs.needs_redraw = true;
        }

        (KeyModifiers::ALT, KeyCode::Char('}')) if !is_read_only => {
            tabs.indent_lines();
        }
        (KeyModifiers::ALT, KeyCode::Char('{')) if !is_read_only => {
            tabs.unindent_lines();
        }
        (KeyModifiers::ALT, KeyCode::Char(';')) if !is_read_only => {
            tabs.toggle_comment();
            tabs.needs_redraw = true;
        }
        (KeyModifiers::ALT, KeyCode::Char('\\')) => {
            tabs.active_editor_mut().word_complete();
            tabs.needs_redraw = true;
        }
        (KeyModifiers::ALT, KeyCode::Char('v')) if !is_read_only => {
            tabs.input_mode = InputMode::VerbatimInput;
            tabs.status_message = "Verbatim input: press any key to insert literally".to_string();
            tabs.needs_redraw = true;
        }
        (KeyModifiers::CONTROL, KeyCode::Char('e')) if !is_read_only => {
            tabs.input_mode = InputMode::ExecuteCommand;
            tabs.filename_buffer.clear();
            tabs.status_message = "Command to execute: ".to_string();
            tabs.needs_redraw = true;
        }

        // Navigation
        (KeyModifiers::CONTROL, KeyCode::Home) => {
            tabs.active_editor_mut().goto_start();
            tabs.needs_redraw = true;
        }
        (KeyModifiers::CONTROL, KeyCode::End) => {
            tabs.active_editor_mut().goto_end();
            tabs.needs_redraw = true;
        }
        (KeyModifiers::CONTROL, KeyCode::Left) => {
            tabs.active_editor_mut().move_word_left();
            tabs.needs_redraw = true;
        }
        (KeyModifiers::CONTROL, KeyCode::Right) => {
            tabs.active_editor_mut().move_word_right();
            tabs.needs_redraw = true;
        }
        (KeyModifiers::ALT, KeyCode::Char(']')) => {
            tabs.active_editor_mut().match_bracket();
            tabs.needs_redraw = true;
        }
        (KeyModifiers::CONTROL, KeyCode::Char('c')) => tabs.show_cursor_info(),
        (_, KeyCode::Up) => {
            tabs.active_editor_mut().move_cursor_up();
            tabs.needs_redraw = true;
        }
        (_, KeyCode::Down) => {
            tabs.active_editor_mut().move_cursor_down();
            tabs.needs_redraw = true;
        }
        (_, KeyCode::Left) => {
            tabs.active_editor_mut().move_cursor_left();
            tabs.needs_redraw = true;
        }
        (_, KeyCode::Right) => {
            tabs.active_editor_mut().move_cursor_right();
            tabs.needs_redraw = true;
        }
        (KeyModifiers::CONTROL, KeyCode::PageUp) => {
            tabs.prev_tab();
        }
        (KeyModifiers::CONTROL, KeyCode::PageDown) => {
            tabs.next_tab();
        }
        (_, KeyCode::PageUp) => {
            tabs.active_editor_mut().page_up();
            tabs.needs_redraw = true;
        }
        (_, KeyCode::PageDown) => {
            tabs.active_editor_mut().page_down();
            tabs.needs_redraw = true;
        }
        (_, KeyCode::Home) => {
            tabs.active_editor_mut().viewport.cursor_pos.1 = 0;
            tabs.needs_redraw = true;
        }
        (_, KeyCode::End) => {
            let editor = tabs.active_editor_mut();
            editor.viewport.cursor_pos.1 =
                crate::editor::line_display_width(&editor.rope, editor.viewport.cursor_pos.0);
            tabs.needs_redraw = true;
        }

        // Editing (blocked for read-only tabs like [Help])
        (_, KeyCode::Char(_))
        | (_, KeyCode::Tab)
        | (_, KeyCode::Enter)
        | (_, KeyCode::Backspace)
        | (_, KeyCode::Delete)
            if is_read_only => {}
        (_, KeyCode::Char(c)) => {
            tabs.active_editor_mut().insert_char(c);
            tabs.needs_redraw = true;
        }
        (_, KeyCode::Tab) => {
            tabs.handle_tab_insertion();
            tabs.needs_redraw = true;
        }
        (_, KeyCode::Enter) => {
            tabs.insert_newline();
            tabs.needs_redraw = true;
        }
        (_, KeyCode::Backspace) => {
            tabs.active_editor_mut().delete_char();
            tabs.needs_redraw = true;
        }
        (_, KeyCode::Delete) => {
            tabs.active_editor_mut().delete_char_forward();
            tabs.needs_redraw = true;
        }
        (_, KeyCode::Esc) => {}

        _ => {}
    }

    Ok(false)
}

fn handle_fuzzy_finder(tabs: &mut TabManager, key: KeyEvent) -> Result<bool> {
    match key.code {
        KeyCode::Esc => {
            tabs.input_mode = InputMode::Normal;
            tabs.needs_redraw = true;
        }
        KeyCode::Enter => {
            let candidates: Vec<(usize, String)> = tabs
                .tabs
                .iter()
                .enumerate()
                .map(|(i, t)| (i, t.display_name.clone()))
                .collect();
            let filtered = crate::fuzzy::fuzzy_filter(&tabs.fuzzy_query, &candidates);
            if let Some((tab_idx, _, _)) = filtered.get(tabs.fuzzy_selected) {
                tabs.active_tab = *tab_idx;
            }
            tabs.input_mode = InputMode::Normal;
            tabs.needs_redraw = true;
        }
        KeyCode::Up => {
            tabs.fuzzy_selected = tabs.fuzzy_selected.saturating_sub(1);
            tabs.needs_redraw = true;
        }
        KeyCode::Down => {
            tabs.fuzzy_selected += 1;
            tabs.needs_redraw = true;
        }
        KeyCode::Backspace => {
            tabs.fuzzy_query.pop();
            tabs.fuzzy_selected = 0;
            tabs.needs_redraw = true;
        }
        KeyCode::Char(c) => {
            tabs.fuzzy_query.push(c);
            tabs.fuzzy_selected = 0;
            tabs.needs_redraw = true;
        }
        _ => {}
    }
    Ok(false)
}

fn handle_verbatim_input(tabs: &mut TabManager, key: KeyEvent) -> Result<bool> {
    match key.code {
        KeyCode::Char(c) => {
            tabs.active_editor_mut().insert_char(c);
        }
        KeyCode::Enter => {
            tabs.insert_newline();
        }
        KeyCode::Tab => {
            let editor = tabs.active_editor_mut();
            editor.save_undo_state();
            let pos = editor
                .line_col_to_char_idx(editor.viewport.cursor_pos.0, editor.viewport.cursor_pos.1);
            editor.rope.insert_char(pos, '\t');
            editor.mark_document_changed(editor.viewport.cursor_pos.0);
            editor.viewport.cursor_pos.1 += 1;
            editor.modified = true;
        }
        _ => {} // ignore non-character keys
    }
    tabs.input_mode = InputMode::Normal;
    tabs.status_message.clear();
    tabs.needs_redraw = true;
    Ok(false)
}

fn handle_execute_command(tabs: &mut TabManager, key: KeyEvent) -> Result<bool> {
    match key.code {
        KeyCode::Enter => {
            let command = tabs.filename_buffer.clone();
            if command.is_empty() {
                tabs.input_mode = InputMode::Normal;
                tabs.set_temporary_status_message("Cancelled".to_string());
                return Ok(false);
            }

            // Transition to confirmation mode
            tabs.pending_command = Some(command.clone());
            tabs.input_mode = InputMode::ConfirmExecute;
            tabs.status_message = format!("Execute: {}? (Y/N)", command);
            tabs.needs_redraw = true;
        }
        KeyCode::Esc => {
            tabs.input_mode = InputMode::Normal;
            tabs.filename_buffer.clear();
            tabs.set_temporary_status_message("Cancelled".to_string());
        }
        KeyCode::Char('c') if key.modifiers == KeyModifiers::CONTROL => {
            tabs.input_mode = InputMode::Normal;
            tabs.filename_buffer.clear();
            tabs.set_temporary_status_message("Cancelled".to_string());
        }
        KeyCode::Backspace => {
            tabs.filename_buffer.pop();
            tabs.status_message = format!("Command to execute: {}", tabs.filename_buffer);
            tabs.needs_redraw = true;
        }
        KeyCode::Char(c) => {
            tabs.filename_buffer.push(c);
            tabs.status_message = format!("Command to execute: {}", tabs.filename_buffer);
            tabs.needs_redraw = true;
        }
        _ => {}
    }
    Ok(false)
}

fn handle_confirm_execute(tabs: &mut TabManager, key: KeyEvent) -> Result<bool> {
    match key.code {
        KeyCode::Char('y') | KeyCode::Char('Y') => {
            let command = match tabs.pending_command.take() {
                Some(cmd) => cmd,
                None => {
                    tabs.input_mode = InputMode::Normal;
                    tabs.filename_buffer.clear();
                    tabs.status_message = "No command to execute".to_string();
                    tabs.needs_redraw = true;
                    return Ok(false);
                }
            };

            // Check if there's a selection
            let selection_text = tabs
                .active_editor()
                .get_selection_range()
                .map(|(start, end)| {
                    tabs.active_editor()
                        .rope
                        .slice(start..end)
                        .chars()
                        .collect::<String>()
                });

            // Execute the command with timeout
            use std::process::{Command, Stdio};
            let mut child = match Command::new("sh")
                .arg("-c")
                .arg(&command)
                .stdin(if selection_text.is_some() {
                    Stdio::piped()
                } else {
                    Stdio::null()
                })
                .stdout(Stdio::piped())
                .stderr(Stdio::piped())
                .spawn()
            {
                Ok(child) => child,
                Err(e) => {
                    tabs.input_mode = InputMode::Normal;
                    tabs.filename_buffer.clear();
                    tabs.set_temporary_status_message(format!("Error: {e}"));
                    return Ok(false);
                }
            };

            if let Some(ref input) = selection_text {
                use std::io::Write;
                if let Some(mut stdin) = child.stdin.take() {
                    let _ = stdin.write_all(input.as_bytes());
                }
            }

            // Wait with a 10-second timeout using try_wait polling
            let timeout = std::time::Duration::from_secs(10);
            let poll_interval = std::time::Duration::from_millis(50);
            let start_time = std::time::Instant::now();
            let timed_out = loop {
                match child.try_wait() {
                    Ok(Some(_status)) => break false,
                    Ok(None) => {
                        if start_time.elapsed() >= timeout {
                            let _ = child.kill();
                            let _ = child.wait();
                            break true;
                        }
                        std::thread::sleep(poll_interval);
                    }
                    Err(_) => break false,
                }
            };

            if timed_out {
                tabs.input_mode = InputMode::Normal;
                tabs.filename_buffer.clear();
                tabs.set_temporary_status_message("Command timed out (10s limit)".to_string());
                tabs.needs_redraw = true;
                return Ok(false);
            }

            // Read output from the completed child
            const MAX_OUTPUT: usize = 1024 * 1024; // 1MB
            let output = child.wait_with_output();

            match output {
                Ok(output) => {
                    let raw_stdout = String::from_utf8_lossy(&output.stdout);
                    let mut stdout = raw_stdout.to_string();
                    let mut truncated = false;
                    if stdout.len() > MAX_OUTPUT {
                        stdout.truncate(MAX_OUTPUT);
                        stdout.push_str("\n[Output truncated at 1MB]");
                        truncated = true;
                    }

                    if let Some((start, end)) = tabs.active_editor().get_selection_range() {
                        // Replace selection with output
                        let editor = tabs.active_editor_mut();
                        editor.save_undo_state();
                        editor.rope.remove(start..end);
                        editor.rope.insert(start, &stdout);
                        editor.mark_anchor = None;
                        editor.modified = true;
                        let line = editor.rope.char_to_line(start);
                        editor.mark_document_changed(line);
                        // Position cursor at end of insertion
                        let end_pos = start + stdout.chars().count();
                        let char_count = editor.rope.len_chars();
                        let clamped = end_pos.min(char_count.saturating_sub(1));
                        let line = editor.rope.char_to_line(clamped);
                        let line_start = editor.rope.line_to_char(line);
                        editor.viewport.cursor_pos = (line, end_pos.saturating_sub(line_start));
                    } else {
                        // Insert output at cursor
                        let editor = tabs.active_editor_mut();
                        editor.save_undo_state();
                        let pos = editor.line_col_to_char_idx(
                            editor.viewport.cursor_pos.0,
                            editor.viewport.cursor_pos.1,
                        );
                        editor.rope.insert(pos, &stdout);
                        editor.modified = true;
                        let cursor_line = editor.viewport.cursor_pos.0;
                        editor.mark_document_changed(cursor_line);
                    }
                    let msg = if truncated {
                        format!("Executed: {command} (output truncated at 1MB)")
                    } else {
                        format!("Executed: {command}")
                    };
                    tabs.set_temporary_status_message(msg);
                }
                Err(e) => {
                    tabs.set_temporary_status_message(format!("Error: {e}"));
                }
            }

            tabs.input_mode = InputMode::Normal;
            tabs.filename_buffer.clear();
            tabs.needs_redraw = true;
        }
        KeyCode::Char('n') | KeyCode::Char('N') | KeyCode::Esc => {
            tabs.pending_command = None;
            tabs.input_mode = InputMode::Normal;
            tabs.filename_buffer.clear();
            tabs.set_temporary_status_message("Cancelled".to_string());
        }
        _ => {
            // Show the prompt again for any other key
            if let Some(ref cmd) = tabs.pending_command {
                tabs.status_message = format!("Execute: {}? (Y/N)", cmd);
            }
            tabs.needs_redraw = true;
        }
    }
    Ok(false)
}
