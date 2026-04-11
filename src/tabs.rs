use std::path::PathBuf;
use std::time::{Duration, Instant};

use crate::config::{self, Config};
use crate::constants;
use crate::editor::{Editor, InputMode};

pub struct TabManager {
    pub tabs: Vec<Editor>,
    pub active_tab: usize,
    pub config: Config,
    pub clipboard: Vec<String>,
    pub last_cut_line: Option<usize>,
    pub input_mode: InputMode,
    pub status_message: String,
    pub status_message_time: Option<Instant>,
    pub status_message_timeout: Duration,
    pub filename_buffer: String,
    pub quit_after_save: bool,
    pub needs_redraw: bool,
    // Fuzzy finder state
    pub fuzzy_query: String,
    pub fuzzy_selected: usize,
    // Tab bar scroll offset (index of first visible tab)
    pub tab_scroll_offset: usize,
    // Pending command for execute confirmation
    pub pending_command: Option<String>,
}

impl Default for TabManager {
    fn default() -> Self {
        Self::new()
    }
}

impl TabManager {
    pub fn new() -> Self {
        let config = config::load_config();
        let initial_tab = Editor::new_buffer();
        Self {
            tabs: vec![initial_tab],
            active_tab: 0,
            config,
            clipboard: Vec::new(),
            last_cut_line: None,
            input_mode: InputMode::Normal,
            status_message: String::new(),
            status_message_time: None,
            status_message_timeout: constants::STATUS_MESSAGE_TIMEOUT,
            filename_buffer: String::new(),
            quit_after_save: false,

            needs_redraw: true,
            fuzzy_query: String::new(),
            fuzzy_selected: 0,
            tab_scroll_offset: 0,
            pending_command: None,
        }
    }

    pub fn new_for_test() -> Self {
        let initial_tab = Editor::new_for_test();
        Self {
            tabs: vec![initial_tab],
            active_tab: 0,
            config: Config::default(),
            clipboard: Vec::new(),
            last_cut_line: None,
            input_mode: InputMode::Normal,
            status_message: String::new(),
            status_message_time: None,
            status_message_timeout: constants::STATUS_MESSAGE_TIMEOUT,
            filename_buffer: String::new(),
            quit_after_save: false,

            needs_redraw: true,
            fuzzy_query: String::new(),
            fuzzy_selected: 0,
            tab_scroll_offset: 0,
            pending_command: None,
        }
    }

    pub fn active_editor(&self) -> &Editor {
        &self.tabs[self.active_tab]
    }

    pub fn active_editor_mut(&mut self) -> &mut Editor {
        &mut self.tabs[self.active_tab]
    }

    /// Open a file in a new tab and switch to it.
    pub fn open_in_new_tab(&mut self, path: PathBuf) -> anyhow::Result<()> {
        let mut editor = Editor::new_buffer();
        editor.load_file(path)?;
        self.tabs.push(editor);
        self.active_tab = self.tabs.len() - 1;
        self.needs_redraw = true;
        Ok(())
    }

    /// Open a file in the current tab (with unsaved check done by caller).
    pub fn open_in_current_tab(&mut self, path: PathBuf) -> anyhow::Result<()> {
        self.active_editor_mut().load_file(path)?;
        self.needs_redraw = true;
        Ok(())
    }

    /// Open (or switch to) a [Help] tab showing the key-binding reference.
    pub fn open_help_tab(&mut self) {
        // If a help tab already exists, just switch to it.
        if let Some(idx) = self.tabs.iter().position(|t| t.display_name == "[Help]") {
            self.active_tab = idx;
            self.needs_redraw = true;
            return;
        }

        let help_text = crate::ui::help_lines().join("\n");
        let mut editor = Editor::new_buffer();
        editor.rope = ropey::Rope::from_str(&help_text);
        editor.display_name = "[Help]".to_string();
        self.tabs.push(editor);
        self.active_tab = self.tabs.len() - 1;
        self.needs_redraw = true;
    }

    /// Create a new empty tab.
    pub fn new_tab(&mut self) {
        let mut tab = Editor::new_buffer();
        // Generate unique untitled name
        let untitled_count = self
            .tabs
            .iter()
            .filter(|t| t.display_name.starts_with("[untitled"))
            .count();
        if untitled_count > 0 {
            tab.display_name = format!("[untitled-{}]", untitled_count + 1);
        }
        self.tabs.push(tab);
        self.active_tab = self.tabs.len() - 1;
        self.needs_redraw = true;
    }

    /// Close the active tab. Returns true if the app should quit (last tab closed).
    pub fn close_tab(&mut self) -> bool {
        if self.tabs.len() <= 1 {
            return true; // signal to quit
        }
        self.tabs.remove(self.active_tab);
        if self.active_tab >= self.tabs.len() {
            self.active_tab = self.tabs.len() - 1;
        }
        self.needs_redraw = true;
        false
    }

    pub fn next_tab(&mut self) {
        if self.tabs.len() > 1 {
            self.active_tab = (self.active_tab + 1) % self.tabs.len();
            self.needs_redraw = true;
        }
    }

    pub fn prev_tab(&mut self) {
        if self.tabs.len() > 1 {
            self.active_tab = if self.active_tab == 0 {
                self.tabs.len() - 1
            } else {
                self.active_tab - 1
            };
            self.needs_redraw = true;
        }
    }

    /// Resolve tab display names -- use filename normally, switch to relative path on collisions.
    pub fn resolve_display_names(&mut self) {
        // Collect all filenames
        let names: Vec<String> = self
            .tabs
            .iter()
            .map(|t| {
                t.file_path
                    .as_ref()
                    .and_then(|p| p.file_name())
                    .map(|n| n.to_string_lossy().to_string())
                    .unwrap_or_else(|| t.display_name.clone())
            })
            .collect();

        for (i, tab) in self.tabs.iter_mut().enumerate() {
            if let Some(path) = &tab.file_path {
                let filename = path
                    .file_name()
                    .map(|n| n.to_string_lossy().to_string())
                    .unwrap_or_default();

                // Check if this filename collides with another tab
                let collisions = names
                    .iter()
                    .enumerate()
                    .filter(|(j, n)| *j != i && **n == filename)
                    .count();

                if collisions > 0 {
                    // Use relative path or full display path
                    tab.display_name = path.display().to_string();
                } else {
                    tab.display_name = filename;
                }
            }
            // untitled tabs keep their existing display_name
        }
    }

    pub fn set_temporary_status_message(&mut self, message: String) {
        self.status_message = message;
        self.status_message_time = Some(Instant::now());
        self.needs_redraw = true;
    }

    pub fn check_status_message_timeout(&mut self) -> bool {
        if let Some(time) = self.status_message_time {
            if time.elapsed() >= self.status_message_timeout {
                self.status_message.clear();
                self.status_message_time = None;
                return true;
            }
        }
        false
    }

    pub fn reset_cut_tracking(&mut self) {
        self.last_cut_line = None;
    }

    pub fn save_config(&self) {
        let _ = config::save_config(&self.config);
    }

    // === Delegated operations that need shared state ===

    pub fn save_file(&mut self) -> anyhow::Result<()> {
        if let Some(path) = self.active_editor().file_path.clone() {
            if let Err(e) = self.perform_save(path) {
                self.set_temporary_status_message(format!("Error saving file: {e}"));
            }
        } else {
            self.start_filename_input();
        }
        Ok(())
    }

    pub fn save_as(&mut self) {
        self.start_save_as_input();
    }

    fn start_filename_input(&mut self) {
        self.input_mode = InputMode::EnteringFilename;
        self.filename_buffer.clear();
        self.status_message = "File Name to Write: ".to_string();
        self.needs_redraw = true;
    }

    fn start_save_as_input(&mut self) {
        self.input_mode = InputMode::EnteringSaveAs;
        self.filename_buffer = self
            .active_editor()
            .file_path
            .as_ref()
            .map(|p| p.display().to_string())
            .unwrap_or_default();
        self.status_message = format!("File Name to Write: {}", self.filename_buffer);
        self.needs_redraw = true;
    }

    pub fn perform_save(&mut self, path: PathBuf) -> anyhow::Result<()> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }

        // Create backup if enabled
        if self.config.backup_on_save && path.exists() {
            let backup_path = PathBuf::from(format!("{}~", path.display()));
            let _ = std::fs::copy(&path, &backup_path);
        }

        let editor = self.active_editor_mut();
        std::fs::write(&path, editor.rope.to_string())?;

        editor.file_path = Some(path.clone());
        editor.modified = false;

        let first_line = editor
            .rope
            .line(0)
            .as_str()
            .map(|s| s.trim_end_matches('\n'));
        editor.syntax_name = editor.highlighter.detect_syntax(Some(&path), first_line);
        editor
            .highlighter
            .set_syntax(editor.syntax_name.as_deref());

        // Update display name
        editor.display_name = path
            .file_name()
            .map(|n| n.to_string_lossy().to_string())
            .unwrap_or_else(|| "[untitled]".to_string());

        self.set_temporary_status_message(format!("Saved: {}", path.display()));
        Ok(())
    }

    pub fn finish_filename_input(&mut self) -> anyhow::Result<bool> {
        if self.filename_buffer.is_empty() {
            self.set_temporary_status_message("Cancelled".to_string());
            self.input_mode = InputMode::Normal;
            self.quit_after_save = false;
            return Ok(false);
        }

        let path = PathBuf::from(&self.filename_buffer);
        if let Err(e) = self.perform_save(path) {
            self.set_temporary_status_message(format!("Error saving file: {e}"));
            self.input_mode = InputMode::Normal;
            self.filename_buffer.clear();
            self.quit_after_save = false;
            return Ok(false);
        }
        self.input_mode = InputMode::Normal;
        self.filename_buffer.clear();

        let should_quit = self.quit_after_save && !self.active_editor().modified;
        self.quit_after_save = false;
        Ok(should_quit)
    }

    pub fn cancel_filename_input(&mut self) {
        self.input_mode = InputMode::Normal;
        self.filename_buffer.clear();
        self.quit_after_save = false;
        self.set_temporary_status_message("Cancelled".to_string());
    }

    /// Try to quit: close the current tab, then move to the next modified tab.
    /// Only returns true (quit the app) when the last tab is closed.
    pub fn try_quit(&mut self) -> bool {
        if self.active_editor().modified {
            let name = self.active_editor().display_name.clone();
            self.input_mode = InputMode::ConfirmQuit;
            self.status_message = format!("Save '{name}' before closing? (Y/N/Ctrl+C)");
            self.needs_redraw = true;
            false
        } else {
            // Current tab is clean — close it and continue
            self.close_current_and_continue()
        }
    }

    pub fn handle_quit_confirmation(&mut self, save: bool) -> anyhow::Result<bool> {
        self.input_mode = InputMode::Normal;

        if save {
            if self.active_editor().file_path.is_some() {
                self.save_file()?;
                if self.active_editor().modified {
                    // Save failed — don't close
                    return Ok(false);
                }
            } else {
                self.quit_after_save = true;
                self.start_filename_input();
                return Ok(false);
            }
        }

        // Tab is either saved or user chose not to save — close it
        Ok(self.close_current_and_continue())
    }

    /// Close the current tab. If more tabs remain, move to the next one
    /// (prompting for unsaved changes if needed). Returns true only when
    /// the very last tab has been closed (meaning the app should exit).
    fn close_current_and_continue(&mut self) -> bool {
        if self.tabs.len() <= 1 {
            return true; // last tab — quit the app
        }

        self.tabs.remove(self.active_tab);
        if self.active_tab >= self.tabs.len() {
            self.active_tab = self.tabs.len() - 1;
        }
        self.needs_redraw = true;

        // Check if the next tab also needs saving
        if self.active_editor().modified {
            let name = self.active_editor().display_name.clone();
            self.input_mode = InputMode::ConfirmQuit;
            self.status_message = format!("Save '{name}' before closing? (Y/N/Ctrl+C)");
        }

        false // more tabs remain
    }

    pub fn cancel_quit_confirmation(&mut self) {
        self.input_mode = InputMode::Normal;
        self.set_temporary_status_message("Cancelled".to_string());
    }

    pub fn open_options_menu(&mut self) {
        self.input_mode = InputMode::OptionsMenu;
        self.status_message = "Options Menu".to_string();
        self.needs_redraw = true;
    }

    pub fn toggle_mouse_mode(&mut self) {
        self.config.mouse_enabled = !self.config.mouse_enabled;

        if self.config.mouse_enabled {
            let _ = crossterm::execute!(
                std::io::stdout(),
                crossterm::event::EnableMouseCapture
            );
            self.set_temporary_status_message("Mouse mode enabled".to_string());
        } else {
            let _ = crossterm::execute!(
                std::io::stdout(),
                crossterm::event::DisableMouseCapture
            );
            self.set_temporary_status_message("Mouse mode disabled".to_string());
        }
    }

    pub fn undo(&mut self) {
        let editor = self.active_editor_mut();
        if editor
            .undo_manager
            .undo(&mut editor.rope, &mut editor.viewport.cursor_pos)
        {
            editor.modified = true;
            editor.invalidate_cache();
            editor.highlighter.invalidate_cache_from_line(0);
        }
        self.needs_redraw = true;
        self.set_temporary_status_message("Undo".to_string());
    }

    pub fn redo(&mut self) {
        let editor = self.active_editor_mut();
        if editor
            .undo_manager
            .redo(&mut editor.rope, &mut editor.viewport.cursor_pos)
        {
            editor.modified = true;
            editor.invalidate_cache();
            editor.highlighter.invalidate_cache_from_line(0);
        }
        self.needs_redraw = true;
        self.set_temporary_status_message("Redo".to_string());
    }

    pub fn start_find(&mut self) {
        self.input_mode = InputMode::Find;
        let editor = self.active_editor_mut();
        editor.search.search_buffer.clear();
        editor.search.search_matches.clear();
        editor.search.current_match_index = None;
        editor.search.find_navigation_mode =
            crate::search::FindNavigationMode::HistoryBrowsing;
        self.status_message = "Find: ".to_string();
        self.needs_redraw = true;
    }

    pub fn start_replace(&mut self) {
        self.input_mode = InputMode::Replace;
        let editor = self.active_editor_mut();
        editor.search.search_buffer.clear();
        editor.search.replace_buffer.clear();
        editor.search.replace_phase = crate::search::ReplacePhase::FindPattern;
        self.status_message = "Find: ".to_string();
        self.needs_redraw = true;
    }

    pub fn start_goto_line(&mut self) {
        self.input_mode = InputMode::GoToLine;
        self.active_editor_mut().search.goto_line_buffer.clear();
        self.status_message = "Go to line: ".to_string();
        self.needs_redraw = true;
    }

    pub fn toggle_hex_view(&mut self) {
        let editor = self.active_editor_mut();
        editor.toggle_hex_view();
        if editor.hex_state.is_some() {
            self.input_mode = InputMode::HexView;
        } else {
            self.input_mode = InputMode::Normal;
        }
        self.needs_redraw = true;
    }

    pub fn goto_line(&mut self, line_num: usize) {
        let editor = self.active_editor_mut();
        if line_num > 0 && line_num <= editor.rope.len_lines() {
            editor.viewport.cursor_pos.0 = line_num - 1;
            editor.viewport.cursor_pos.1 = 0;
            editor.clamp_cursor_to_line();
            self.set_temporary_status_message(format!("Jumped to line {line_num}"));
        } else {
            self.set_temporary_status_message(format!("Invalid line number: {line_num}"));
        }
    }

    pub fn toggle_regex_mode(&mut self) {
        let editor = self.active_editor_mut();
        editor.search.use_regex = !editor.search.use_regex;
        let mode = if editor.search.use_regex {
            "Regex"
        } else {
            "Literal"
        };
        let detail = if editor.search.use_regex {
            "Pattern matching"
        } else {
            "Exact text"
        };
        self.set_temporary_status_message(format!("Search mode: {} ({})", mode, detail));
        self.needs_redraw = true;

        if !self.active_editor().search.search_buffer.is_empty()
            && self.input_mode == InputMode::Find
        {
            let search_term = self.active_editor().search.search_buffer.clone();
            self.active_editor_mut().perform_find(&search_term);
        }
    }

    pub fn toggle_case_sensitive(&mut self) {
        let editor = self.active_editor_mut();
        editor.search.case_sensitive = !editor.search.case_sensitive;
        let mode = if editor.search.case_sensitive {
            "Case sensitive"
        } else {
            "Case insensitive"
        };
        self.set_temporary_status_message(format!("Search: {}", mode));
        self.needs_redraw = true;

        if !self.active_editor().search.search_buffer.is_empty()
            && self.input_mode == InputMode::Find
        {
            let search_term = self.active_editor().search.search_buffer.clone();
            self.active_editor_mut().perform_find(&search_term);
        }
    }

    pub fn show_cursor_info(&mut self) {
        let editor = self.active_editor();
        let line = editor.viewport.cursor_pos.0 + 1;
        let col = editor.viewport.cursor_pos.1 + 1;
        let total_lines = editor.rope.len_lines();
        let total_chars = editor.rope.len_chars();
        let char_idx = editor.line_col_to_char_idx(
            editor.viewport.cursor_pos.0,
            editor.viewport.cursor_pos.1,
        );
        self.set_temporary_status_message(format!(
            "Line: {}/{} | Col: {} | Char: {}/{}",
            line,
            total_lines,
            col,
            char_idx + 1,
            total_chars
        ));
    }

    /// Cut line/selection - delegates to editor but uses shared clipboard
    pub fn cut(&mut self) {
        let idx = self.active_tab;
        if let Some((start, end)) = self.tabs[idx].get_selection_range() {
            if start == end {
                self.cut_line();
                return;
            }
            self.tabs[idx].save_undo_state();
            let selected: String = self.tabs[idx].rope.slice(start..end).chars().collect();
            self.clipboard = vec![selected];
            self.last_cut_line = None;
            self.tabs[idx].rope.remove(start..end);
            let char_count = self.tabs[idx].rope.len_chars();
            let clamped = if char_count == 0 {
                0
            } else {
                start.min(char_count - 1)
            };
            let line = self.tabs[idx].rope.char_to_line(clamped);
            let line_start = self.tabs[idx].rope.line_to_char(line);
            let col_chars = start.saturating_sub(line_start);
            let display_col = self.tabs[idx].char_idx_to_display_col(line, col_chars);
            self.tabs[idx].viewport.cursor_pos = (line, display_col);
            self.tabs[idx].mark_anchor = None;
            self.tabs[idx].modified = true;
            self.tabs[idx].mark_document_changed(line);
        } else {
            self.cut_line();
        }
    }

    fn cut_line(&mut self) {
        let idx = self.active_tab;
        let line_idx = self.tabs[idx].viewport.cursor_pos.0;
        if line_idx >= self.tabs[idx].rope.len_lines() {
            return;
        }

        self.tabs[idx].save_undo_state();

        let line_start = self.tabs[idx].rope.line_to_char(line_idx);
        let line_end = if line_idx + 1 < self.tabs[idx].rope.len_lines() {
            self.tabs[idx].rope.line_to_char(line_idx + 1)
        } else {
            self.tabs[idx].rope.len_chars()
        };

        let line_text: String = self.tabs[idx]
            .rope
            .slice(line_start..line_end)
            .chars()
            .collect();

        // Accumulate if consecutive cut on adjacent line
        if self.last_cut_line == Some(line_idx) || self.last_cut_line == Some(line_idx + 1) {
            // Append to existing clipboard for consecutive cuts (nano behavior)
        } else {
            self.clipboard.clear();
        }
        self.clipboard.push(line_text);
        self.last_cut_line = Some(line_idx);

        self.tabs[idx].rope.remove(line_start..line_end);

        let max_line = self.tabs[idx].rope.len_lines().saturating_sub(1);
        if self.tabs[idx].viewport.cursor_pos.0 > max_line {
            self.tabs[idx].viewport.cursor_pos.0 = max_line;
        }
        self.tabs[idx].viewport.cursor_pos.1 = 0;
        self.tabs[idx].clamp_cursor_to_line();

        self.tabs[idx].modified = true;
        self.tabs[idx].mark_document_changed(line_idx);
    }

    /// Copy line/selection
    pub fn copy(&mut self) {
        let idx = self.active_tab;
        if let Some((start, end)) = self.tabs[idx].get_selection_range() {
            if start == end {
                self.copy_line();
                return;
            }
            let selected: String = self.tabs[idx].rope.slice(start..end).chars().collect();
            self.clipboard = vec![selected];
            self.last_cut_line = None;
            self.tabs[idx].mark_anchor = None;
            self.set_temporary_status_message("Copied selection".to_string());
        } else {
            self.copy_line();
        }
    }

    fn copy_line(&mut self) {
        let idx = self.active_tab;
        let line_idx = self.tabs[idx].viewport.cursor_pos.0;
        if line_idx >= self.tabs[idx].rope.len_lines() {
            return;
        }

        let line_start = self.tabs[idx].rope.line_to_char(line_idx);
        let line_end = if line_idx + 1 < self.tabs[idx].rope.len_lines() {
            self.tabs[idx].rope.line_to_char(line_idx + 1)
        } else {
            self.tabs[idx].rope.len_chars()
        };

        let line_text: String = self.tabs[idx]
            .rope
            .slice(line_start..line_end)
            .chars()
            .collect();
        self.clipboard = vec![line_text];
        self.last_cut_line = None;

        self.set_temporary_status_message("Copied 1 line".to_string());
    }

    /// Paste clipboard contents at cursor position (inserts above current line).
    pub fn paste(&mut self) {
        if self.clipboard.is_empty() {
            return;
        }

        let paste_text: String = self.clipboard.join("");
        let idx = self.active_tab;
        self.tabs[idx].save_undo_state();

        let insert_pos = self.tabs[idx]
            .rope
            .line_to_char(self.tabs[idx].viewport.cursor_pos.0);

        self.tabs[idx].rope.insert(insert_pos, &paste_text);
        self.tabs[idx].modified = true;

        self.tabs[idx].viewport.cursor_pos.1 = 0;
        let cursor_line = self.tabs[idx].viewport.cursor_pos.0;
        self.tabs[idx].mark_document_changed(cursor_line);

        let lines_pasted = paste_text.matches('\n').count();
        self.set_temporary_status_message(format!("Pasted {} line(s)", lines_pasted.max(1)));
    }

    /// Paste clipboard at current cursor position (inline, not above line).
    pub fn paste_inline(&mut self) {
        if self.clipboard.is_empty() {
            return;
        }
        let paste_text: String = self.clipboard.join("");
        let idx = self.active_tab;
        self.tabs[idx].save_undo_state();
        self.tabs[idx].mark_anchor = None;
        let pos = self.tabs[idx].line_col_to_char_idx(
            self.tabs[idx].viewport.cursor_pos.0,
            self.tabs[idx].viewport.cursor_pos.1,
        );
        self.tabs[idx].rope.insert(pos, &paste_text);
        self.tabs[idx].modified = true;
        let end_pos = pos + paste_text.chars().count();
        let char_count = self.tabs[idx].rope.len_chars();
        let clamped = if char_count == 0 {
            0
        } else {
            end_pos.min(char_count - 1)
        };
        let line = self.tabs[idx].rope.char_to_line(clamped);
        let line_start = self.tabs[idx].rope.line_to_char(line);
        let col_chars = end_pos.saturating_sub(line_start);
        let display_col = self.tabs[idx].char_idx_to_display_col(line, col_chars);
        self.tabs[idx].viewport.cursor_pos = (line, display_col);
        let cursor_line = self.tabs[idx].viewport.cursor_pos.0;
        self.tabs[idx].mark_document_changed(cursor_line);
    }

    pub fn toggle_mark(&mut self) {
        let editor = self.active_editor_mut();
        if editor.mark_anchor.is_some() {
            editor.mark_anchor = None;
            self.set_temporary_status_message("Mark unset".to_string());
        } else {
            editor.mark_anchor = Some(editor.viewport.cursor_pos);
            self.set_temporary_status_message("Mark set".to_string());
        }
        self.needs_redraw = true;
    }

    pub fn indent_lines(&mut self) {
        let tab_width = self.config.tab_width;
        let editor = self.active_editor_mut();
        let (start_line, end_line) = editor.get_affected_lines();
        editor.save_undo_state();

        let indent: String = " ".repeat(tab_width);

        for line_idx in (start_line..=end_line).rev() {
            if line_idx < editor.rope.len_lines() {
                let line_start = editor.rope.line_to_char(line_idx);
                editor.rope.insert(line_start, &indent);
            }
        }

        editor.mark_anchor = None;
        editor.modified = true;
        editor.mark_document_changed(start_line);
        self.set_temporary_status_message(format!(
            "Indented {} line(s)",
            end_line - start_line + 1
        ));
    }

    pub fn unindent_lines(&mut self) {
        let tab_width = self.config.tab_width;
        let editor = self.active_editor_mut();
        let (start_line, end_line) = editor.get_affected_lines();
        editor.save_undo_state();

        for line_idx in (start_line..=end_line).rev() {
            if line_idx < editor.rope.len_lines() {
                let line_start = editor.rope.line_to_char(line_idx);
                let mut spaces_to_remove = 0;
                for ch in editor.rope.line(line_idx).chars() {
                    if ch == ' ' && spaces_to_remove < tab_width {
                        spaces_to_remove += 1;
                    } else if ch == '\t' && spaces_to_remove == 0 {
                        spaces_to_remove = 1;
                        break;
                    } else {
                        break;
                    }
                }
                if spaces_to_remove > 0 {
                    editor
                        .rope
                        .remove(line_start..line_start + spaces_to_remove);
                }
            }
        }

        editor.mark_anchor = None;
        editor.modified = true;
        editor.clamp_cursor_to_line();
        editor.mark_document_changed(start_line);
    }

    pub fn toggle_comment(&mut self) {
        let editor = self.active_editor_mut();
        editor.toggle_comment();
    }

    pub fn handle_tab_insertion(&mut self) {
        let tab_width = self.config.tab_width;
        let editor = self.active_editor_mut();
        editor.save_undo_state();

        let current_col = editor.viewport.cursor_pos.1;
        let spaces_to_next_tab = tab_width - (current_col % tab_width.max(1));

        for _ in 0..spaces_to_next_tab {
            editor.insert_char(' ');
        }
    }

    pub fn insert_newline(&mut self) {
        let auto_indent = self.config.auto_indent;
        self.active_editor_mut().insert_newline(auto_indent);
    }

    pub fn handle_mouse_event(
        &mut self,
        event: crossterm::event::MouseEvent,
        terminal_height: usize,
    ) {
        // Row 0 is the tab bar -- only respond to clicks, not hover/drag
        let mut adjusted = event;
        if adjusted.row == 0 {
            if matches!(event.kind, crossterm::event::MouseEventKind::Down(_)) {
                self.handle_tab_bar_click(adjusted.column as usize);
                self.needs_redraw = true;
            }
            return;
        }
        adjusted.row = adjusted.row.saturating_sub(1);
        let line_num_width = if self.config.show_line_numbers {
            self.active_editor().rope.len_lines().to_string().len() + 1
        } else {
            0
        };
        let editor = self.active_editor_mut();
        editor.handle_mouse_event(adjusted, terminal_height, line_num_width);
        self.needs_redraw = true;
    }

    fn handle_tab_bar_click(&mut self, click_col: usize) {
        let mut col = 0;

        // Account for left overflow indicator width when tabs are scrolled
        if self.tab_scroll_offset > 0 {
            let left_label = format!(" <{} ", self.tab_scroll_offset);
            col += left_label.len();
        }

        // Start iterating from tab_scroll_offset, matching the rendering order
        for i in self.tab_scroll_offset..self.tabs.len() {
            let tab = &self.tabs[i];
            let modified = if tab.modified { "*" } else { "" };
            let title = format!(" {}{} ", tab.display_name, modified);
            let title_len = title.len();
            if click_col >= col && click_col < col + title_len {
                self.active_tab = i;
                return;
            }
            col += title_len;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ropey::Rope;

    fn content(tabs: &TabManager) -> String {
        tabs.active_editor().rope.to_string()
    }

    fn make_tabs(text: &str) -> TabManager {
        let mut tabs = TabManager::new_for_test();
        tabs.active_editor_mut().rope = Rope::from_str(text);
        tabs
    }

    #[test]
    fn test_cut_line_basic() {
        let mut t = make_tabs("line1\nline2\nline3\n");
        t.active_editor_mut().viewport.cursor_pos = (1, 0);
        t.cut_line();
        assert_eq!(content(&t), "line1\nline3\n");
        assert_eq!(t.active_editor().viewport.cursor_pos, (1, 0));
    }

    #[test]
    fn test_cut_single_line_doc() {
        let mut t = make_tabs("only line\n");
        t.active_editor_mut().viewport.cursor_pos = (0, 0);
        t.cut_line();
        assert_eq!(content(&t), "");
        assert_eq!(t.active_editor().viewport.cursor_pos.0, 0);
    }

    #[test]
    fn test_paste_after_cut() {
        let mut t = make_tabs("line1\nline2\nline3\n");
        t.active_editor_mut().viewport.cursor_pos = (0, 0);
        t.cut_line();
        assert_eq!(content(&t), "line2\nline3\n");
        t.active_editor_mut().viewport.cursor_pos = (1, 0);
        t.paste();
        assert_eq!(content(&t), "line2\nline1\nline3\n");
    }

    #[test]
    fn test_copy_line() {
        let mut t = make_tabs("line1\nline2\n");
        t.active_editor_mut().viewport.cursor_pos = (0, 0);
        t.copy_line();
        assert_eq!(content(&t), "line1\nline2\n");
        t.active_editor_mut().viewport.cursor_pos = (1, 0);
        t.paste();
        assert_eq!(content(&t), "line1\nline1\nline2\n");
    }

    #[test]
    fn test_paste_empty_clipboard() {
        let mut t = make_tabs("hello\n");
        t.active_editor_mut().viewport.cursor_pos = (0, 0);
        t.paste();
        assert_eq!(content(&t), "hello\n");
    }

    #[test]
    fn test_multiple_cuts_accumulate() {
        let mut t = make_tabs("a\nb\nc\n");
        t.active_editor_mut().viewport.cursor_pos = (0, 0);
        t.cut_line();
        t.cut_line();
        assert_eq!(content(&t), "c\n");
        t.paste();
        assert_eq!(content(&t), "a\nb\nc\n");
    }

    #[test]
    fn test_cut_undo() {
        let mut t = make_tabs("line1\nline2\n");
        t.active_editor_mut().viewport.cursor_pos = (0, 0);
        t.cut_line();
        assert_eq!(content(&t), "line2\n");
        t.undo();
        assert_eq!(content(&t), "line1\nline2\n");
    }

    #[test]
    fn test_cut_resets_on_non_consecutive() {
        let mut t = make_tabs("a\nb\nc\n");
        t.active_editor_mut().viewport.cursor_pos = (0, 0);
        t.cut_line();
        t.reset_cut_tracking();
        t.active_editor_mut().viewport.cursor_pos = (0, 0);
        t.cut_line();
        assert_eq!(t.clipboard.len(), 1);
        assert_eq!(t.clipboard[0], "b\n");
    }

    #[test]
    fn test_cut_selection() {
        let mut t = make_tabs("hello world\n");
        t.active_editor_mut().viewport.cursor_pos = (0, 0);
        t.active_editor_mut().mark_anchor = Some((0, 0));
        t.active_editor_mut().viewport.cursor_pos = (0, 5);
        t.cut();
        assert_eq!(content(&t), " world\n");
        assert!(t.active_editor().mark_anchor.is_none());
        assert_eq!(t.clipboard, vec!["hello".to_string()]);
    }

    #[test]
    fn test_cut_no_selection_falls_back_to_cut_line() {
        let mut t = make_tabs("line1\nline2\n");
        t.active_editor_mut().viewport.cursor_pos = (0, 0);
        t.cut();
        assert_eq!(content(&t), "line2\n");
    }

    #[test]
    fn test_copy_selection() {
        let mut t = make_tabs("hello world\n");
        t.active_editor_mut().mark_anchor = Some((0, 0));
        t.active_editor_mut().viewport.cursor_pos = (0, 5);
        t.copy();
        assert_eq!(content(&t), "hello world\n"); // unchanged
        assert_eq!(t.clipboard, vec!["hello".to_string()]);
        assert!(t.active_editor().mark_anchor.is_none());
    }

    #[test]
    fn test_copy_no_selection_falls_back_to_copy_line() {
        let mut t = make_tabs("line1\nline2\n");
        t.active_editor_mut().viewport.cursor_pos = (0, 0);
        t.copy();
        assert_eq!(content(&t), "line1\nline2\n");
        assert_eq!(t.clipboard, vec!["line1\n".to_string()]);
    }

    #[test]
    fn test_selection_across_lines() {
        let mut t = make_tabs("hello\nworld\n");
        t.active_editor_mut().mark_anchor = Some((0, 3));
        t.active_editor_mut().viewport.cursor_pos = (1, 3);
        t.cut();
        assert_eq!(content(&t), "helld\n");
    }

    #[test]
    fn test_paste_inline() {
        let mut t = make_tabs("hello world\n");
        t.clipboard = vec!["XY".to_string()];
        t.active_editor_mut().viewport.cursor_pos = (0, 5);
        t.paste_inline();
        assert_eq!(content(&t), "helloXY world\n");
        assert_eq!(t.active_editor().viewport.cursor_pos, (0, 7));
    }

    #[test]
    fn test_paste_inline_empty_clipboard() {
        let mut t = make_tabs("hello\n");
        t.active_editor_mut().viewport.cursor_pos = (0, 0);
        t.paste_inline();
        assert_eq!(content(&t), "hello\n");
    }

    // Indent tests
    #[test]
    fn test_indent_adds_spaces() {
        let mut t = make_tabs("hello\nworld\n");
        t.active_editor_mut().viewport.cursor_pos = (0, 0);
        t.active_editor_mut().mark_anchor = Some((1, 0));
        t.indent_lines();
        assert_eq!(content(&t), "    hello\n    world\n");
    }

    #[test]
    fn test_indent_single_line() {
        let mut t = make_tabs("hello\nworld\n");
        t.active_editor_mut().viewport.cursor_pos = (1, 0);
        t.indent_lines();
        assert_eq!(content(&t), "hello\n    world\n");
    }

    #[test]
    fn test_unindent_removes_spaces() {
        let mut t = make_tabs("    hello\n    world\n");
        t.active_editor_mut().viewport.cursor_pos = (0, 0);
        t.active_editor_mut().mark_anchor = Some((1, 0));
        t.unindent_lines();
        assert_eq!(content(&t), "hello\nworld\n");
    }

    // Show cursor info test
    #[test]
    fn test_show_cursor_info() {
        let mut t = make_tabs("hello\nworld\n");
        t.active_editor_mut().viewport.cursor_pos = (1, 3);
        t.show_cursor_info();
        assert!(t.status_message.contains("Line: 2"));
        assert!(t.status_message.contains("Col: 4"));
    }

    #[test]
    fn test_backup_on_save() {
        let dir = std::env::temp_dir().join("rune_test_backup");
        let _ = std::fs::create_dir_all(&dir);
        let file_path = dir.join("test_backup.txt");
        let backup_path = dir.join("test_backup.txt~");

        // Write initial content
        std::fs::write(&file_path, "original").unwrap();

        let mut t = make_tabs("modified content");
        t.config.backup_on_save = true;
        t.active_editor_mut().file_path = Some(file_path.clone());
        t.perform_save(file_path.clone()).unwrap();

        // Backup should exist with original content
        assert!(backup_path.exists());
        assert_eq!(std::fs::read_to_string(&backup_path).unwrap(), "original");
        assert_eq!(
            std::fs::read_to_string(&file_path).unwrap(),
            "modified content"
        );

        // Cleanup
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn test_no_backup_when_disabled() {
        let dir = std::env::temp_dir().join("rune_test_no_backup");
        let _ = std::fs::create_dir_all(&dir);
        let file_path = dir.join("test_no_backup.txt");
        let backup_path = dir.join("test_no_backup.txt~");

        std::fs::write(&file_path, "original").unwrap();

        let mut t = make_tabs("modified");
        t.config.backup_on_save = false;
        t.active_editor_mut().file_path = Some(file_path.clone());
        t.perform_save(file_path.clone()).unwrap();

        assert!(!backup_path.exists());

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn test_verbatim_input_mode() {
        let mut t = make_tabs("hello\n");
        // Enter verbatim input mode
        t.input_mode = InputMode::VerbatimInput;
        assert_eq!(t.input_mode, InputMode::VerbatimInput);
        // Simulate a key press - the handler is in input.rs,
        // but we can verify mode transition logic
        use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
        let key = KeyEvent::new(KeyCode::Char('\t'), KeyModifiers::NONE);
        // Verbatim should insert literally and return to Normal
        let _ = crate::input::handle_key_event(&mut t, key);
        assert_eq!(t.input_mode, InputMode::Normal);
    }

    #[test]
    fn test_execute_command_mode() {
        let mut t = make_tabs("hello\n");
        t.input_mode = InputMode::ExecuteCommand;
        t.filename_buffer = "echo test".to_string();
        // Simulate Enter to go to confirmation
        use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
        let enter = KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE);
        let _ = crate::input::handle_key_event(&mut t, enter);
        assert_eq!(t.input_mode, InputMode::ConfirmExecute);
        // Confirm with Y
        let y = KeyEvent::new(KeyCode::Char('y'), KeyModifiers::NONE);
        let _ = crate::input::handle_key_event(&mut t, y);
        assert_eq!(t.input_mode, InputMode::Normal);
        // "echo test" output should be inserted
        assert!(content(&t).contains("test"));
    }

    #[test]
    fn test_execute_command_cancel() {
        let mut t = make_tabs("hello\n");
        t.input_mode = InputMode::ExecuteCommand;
        use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
        let esc = KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE);
        let _ = crate::input::handle_key_event(&mut t, esc);
        assert_eq!(t.input_mode, InputMode::Normal);
        assert_eq!(content(&t), "hello\n"); // unchanged
    }

    #[test]
    fn test_execute_command_confirm_cancel() {
        let mut t = make_tabs("hello\n");
        t.input_mode = InputMode::ExecuteCommand;
        t.filename_buffer = "echo test".to_string();
        use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
        // Enter to go to confirmation
        let enter = KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE);
        let _ = crate::input::handle_key_event(&mut t, enter);
        assert_eq!(t.input_mode, InputMode::ConfirmExecute);
        // Cancel with N
        let n = KeyEvent::new(KeyCode::Char('n'), KeyModifiers::NONE);
        let _ = crate::input::handle_key_event(&mut t, n);
        assert_eq!(t.input_mode, InputMode::Normal);
        assert_eq!(content(&t), "hello\n"); // unchanged
    }

    #[test]
    fn test_execute_command_with_selection() {
        let mut t = make_tabs("hello world\n");
        // Select "hello" (chars 0..5)
        t.active_editor_mut().mark_anchor = Some((0, 0));
        t.active_editor_mut().viewport.cursor_pos = (0, 5);
        t.input_mode = InputMode::ExecuteCommand;
        t.filename_buffer = "tr a-z A-Z".to_string();
        use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
        let enter = KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE);
        let _ = crate::input::handle_key_event(&mut t, enter);
        assert_eq!(t.input_mode, InputMode::ConfirmExecute);
        // Confirm with Y
        let y = KeyEvent::new(KeyCode::Char('Y'), KeyModifiers::NONE);
        let _ = crate::input::handle_key_event(&mut t, y);
        assert_eq!(t.input_mode, InputMode::Normal);
        // "hello" should be replaced with "HELLO"
        assert!(content(&t).contains("HELLO"));
    }
}
