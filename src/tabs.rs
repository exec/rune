use std::path::{Path, PathBuf};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use crate::config::{self, Config};
use crate::constants;
use crate::editor::{Editor, InputMode};

/// What to do after the filename prompt successfully saves an untitled buffer.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AfterSave {
    /// Ctrl+Q flow: close the saved tab and keep quitting — move to the next
    /// tab (prompting if it is modified) and only exit when the last tab closes.
    ContinueQuit,
    /// Alt+W flow: close just the saved tab; exit only if it was the last one.
    CloseTab,
}

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
    pub input_buffer: String,
    pub pending_after_save: Option<AfterSave>,
    pub needs_redraw: bool,
    // Fuzzy finder state
    pub fuzzy_query: String,
    pub fuzzy_selected: usize,
    pub fuzzy_candidates: Vec<crate::fuzzy::FuzzyCandidate>,
    // Tab bar scroll offset (index of first visible tab)
    pub tab_scroll_offset: usize,
    // Pending command for execute confirmation
    pub pending_command: Option<String>,
    // Global read-only mode (--view flag)
    pub read_only: bool,
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
            input_buffer: String::new(),
            pending_after_save: None,

            needs_redraw: true,
            fuzzy_query: String::new(),
            fuzzy_selected: 0,
            fuzzy_candidates: Vec::new(),
            tab_scroll_offset: 0,
            pending_command: None,
            read_only: false,
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
            input_buffer: String::new(),
            pending_after_save: None,

            needs_redraw: true,
            fuzzy_query: String::new(),
            fuzzy_selected: 0,
            fuzzy_candidates: Vec::new(),
            tab_scroll_offset: 0,
            pending_command: None,
            read_only: false,
        }
    }

    /// Rebuild the cached prepared fuzzy candidate list from the current tab names.
    /// Should be called when entering FuzzyFinder mode or when the tab list changes.
    pub fn rebuild_fuzzy_candidates(&mut self) {
        self.fuzzy_candidates = self
            .tabs
            .iter()
            .enumerate()
            .map(|(i, t)| crate::fuzzy::FuzzyCandidate::new(i, t.display_name.clone()))
            .collect();
    }

    pub fn active_editor(&self) -> &Editor {
        debug_assert!(
            self.active_tab < self.tabs.len(),
            "active_tab {} out of bounds (tabs.len() = {})",
            self.active_tab,
            self.tabs.len()
        );
        self.tabs
            .get(self.active_tab)
            .expect("active_tab index out of bounds")
    }

    pub fn active_editor_mut(&mut self) -> &mut Editor {
        debug_assert!(
            self.active_tab < self.tabs.len(),
            "active_tab {} out of bounds (tabs.len() = {})",
            self.active_tab,
            self.tabs.len()
        );
        let len = self.tabs.len();
        self.tabs.get_mut(self.active_tab).unwrap_or_else(|| {
            panic!(
                "active_tab {} out of bounds (tabs.len() = {})",
                self.active_tab, len
            )
        })
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
        // Pick the smallest free untitled suffix; plain "[untitled]" counts
        // as 1, so with "[untitled]" open the first duplicate becomes
        // "[untitled-2]", and closed tabs free their suffix for reuse.
        let suffix_in_use = |n: usize| {
            let name = if n == 1 {
                "[untitled]".to_string()
            } else {
                format!("[untitled-{n}]")
            };
            self.tabs.iter().any(|t| t.display_name == name)
        };
        let mut suffix = 1;
        while suffix_in_use(suffix) {
            suffix += 1;
        }
        if suffix > 1 {
            tab.display_name = format!("[untitled-{suffix}]");
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
        self.reset_editor_mode_on_tab_switch();
        self.tabs.remove(self.active_tab);
        if self.active_tab >= self.tabs.len() {
            self.active_tab = self.tabs.len() - 1;
        }
        self.needs_redraw = true;
        false
    }

    /// Reset input_mode to Normal when switching tabs if we're in an
    /// editor-specific mode whose state lives on the per-tab Editor.
    fn reset_editor_mode_on_tab_switch(&mut self) {
        match self.input_mode {
            InputMode::Find
            | InputMode::FindOptionsMenu
            | InputMode::Replace
            | InputMode::ReplaceConfirm
            | InputMode::GoToLine
            | InputMode::HexView => {
                self.input_mode = InputMode::Normal;
                self.status_message.clear();
            }
            _ => {}
        }
    }

    pub fn next_tab(&mut self) {
        if self.tabs.len() > 1 {
            self.reset_editor_mode_on_tab_switch();
            self.active_tab = (self.active_tab + 1) % self.tabs.len();
            self.needs_redraw = true;
        }
    }

    pub fn prev_tab(&mut self) {
        if self.tabs.len() > 1 {
            self.reset_editor_mode_on_tab_switch();
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
        self.input_buffer.clear();
        self.status_message = "File Name to Write: ".to_string();
        self.needs_redraw = true;
    }

    /// Prompt for a filename for an untitled buffer that is being closed via
    /// Alt+W ("save and close"). After a successful save the tab is closed
    /// (see `finish_filename_input`); cancelling leaves the tab open.
    pub fn start_close_tab_filename_input(&mut self) {
        self.pending_after_save = Some(AfterSave::CloseTab);
        self.start_filename_input();
    }

    fn start_save_as_input(&mut self) {
        self.input_mode = InputMode::EnteringSaveAs;
        self.input_buffer = self
            .active_editor()
            .file_path
            .as_ref()
            .map(|p| p.display().to_string())
            .unwrap_or_default();
        self.status_message = format!("File Name to Write: {}", self.input_buffer);
        self.needs_redraw = true;
    }

    pub fn perform_save(&mut self, path: PathBuf) -> anyhow::Result<()> {
        let created_parent: Option<PathBuf> = match path.parent() {
            Some(parent) if !parent.as_os_str().is_empty() => {
                let missing = !parent.is_dir();
                std::fs::create_dir_all(parent)?;
                if missing {
                    Some(parent.to_path_buf())
                } else {
                    None
                }
            }
            _ => None,
        };

        // Create backup if enabled. This runs BEFORE the atomic swap so it
        // still reads the old file (which exists until the rename below).
        if self.config.backup_on_save && path.exists() {
            let backup_path = PathBuf::from(format!("{}~", path.display()));
            // If the backup path already exists as a symlink, an attacker in a
            // shared directory could have planted it to redirect the copy
            // (and thus the file's contents) to a destination of their choice.
            // Replace a symlink with a regular file before copying.
            // `symlink_metadata` does not follow the link.
            if let Ok(meta) = std::fs::symlink_metadata(&backup_path) {
                if meta.file_type().is_symlink() {
                    let _ = std::fs::remove_file(&backup_path);
                }
            }
            if let Err(e) = std::fs::copy(&path, &backup_path) {
                self.set_temporary_status_message(format!("Warning: backup failed: {e}"));
            }
        }

        let editor = self.active_editor_mut();
        let bytes: Vec<u8> = editor.rope.bytes().collect();
        atomic_write(&path, &bytes)?;

        editor.file_path = Some(path.clone());
        editor.modified = false;

        let first_line = editor
            .rope
            .line(0)
            .as_str()
            .map(|s| s.trim_end_matches('\n'));
        editor.syntax_name = editor.highlighter.detect_syntax(Some(&path), first_line);
        editor.highlighter.set_syntax(editor.syntax_name.as_deref());

        // Update display name
        editor.display_name = path
            .file_name()
            .map(|n| n.to_string_lossy().to_string())
            .unwrap_or_else(|| "[untitled]".to_string());

        match created_parent {
            Some(parent) => self.set_temporary_status_message(format!(
                "Saved: {} (created new directory: {})",
                path.display(),
                parent.display()
            )),
            None => self.set_temporary_status_message(format!("Saved: {}", path.display())),
        }
        Ok(())
    }

    pub fn finish_filename_input(&mut self) -> anyhow::Result<bool> {
        if self.input_buffer.is_empty() {
            self.set_temporary_status_message("Cancelled".to_string());
            self.input_mode = InputMode::Normal;
            self.pending_after_save = None;
            return Ok(false);
        }

        let path = PathBuf::from(&self.input_buffer);
        if let Err(e) = self.perform_save(path) {
            self.set_temporary_status_message(format!("Error saving file: {e}"));
            self.input_mode = InputMode::Normal;
            self.input_buffer.clear();
            self.pending_after_save = None;
            return Ok(false);
        }
        self.input_mode = InputMode::Normal;
        self.input_buffer.clear();

        // The save succeeded; carry out whatever close action prompted the
        // filename input. Only return true (exit the app) when the saved tab
        // was the last one.
        match self.pending_after_save.take() {
            Some(AfterSave::ContinueQuit) => Ok(self.close_current_and_continue()),
            Some(AfterSave::CloseTab) => Ok(self.close_tab()),
            None => Ok(false),
        }
    }

    pub fn cancel_filename_input(&mut self) {
        self.input_mode = InputMode::Normal;
        self.input_buffer.clear();
        self.pending_after_save = None;
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
                self.pending_after_save = Some(AfterSave::ContinueQuit);
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

        self.reset_editor_mode_on_tab_switch();
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
            let _ = crossterm::execute!(std::io::stdout(), crossterm::event::EnableMouseCapture);
            self.set_temporary_status_message("Mouse mode enabled".to_string());
        } else {
            let _ = crossterm::execute!(std::io::stdout(), crossterm::event::DisableMouseCapture);
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
            self.needs_redraw = true;
            self.set_temporary_status_message("Undo".to_string());
        } else {
            self.set_temporary_status_message("Nothing to undo".to_string());
        }
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
            self.needs_redraw = true;
            self.set_temporary_status_message("Redo".to_string());
        } else {
            self.set_temporary_status_message("Nothing to redo".to_string());
        }
    }

    pub fn start_find(&mut self) {
        self.input_mode = InputMode::Find;
        let editor = self.active_editor_mut();
        // Snapshot the cursor so Esc / cancel_search can restore it. The
        // snapshot must happen here, NOT on every perform_find call —
        // otherwise it drifts to the most recent match as the user types.
        editor.search.search_start_pos = editor.viewport.cursor_pos;
        editor.search.search_buffer.clear();
        editor.search.search_matches.clear();
        editor.search.current_match_index = None;
        editor.search.find_navigation_mode = crate::search::FindNavigationMode::HistoryBrowsing;
        self.status_message = "Find: ".to_string();
        self.needs_redraw = true;
    }

    pub fn start_replace(&mut self) {
        self.input_mode = InputMode::Replace;
        let editor = self.active_editor_mut();
        // Snapshot for Esc/cancel restore — see start_find for rationale.
        editor.search.search_start_pos = editor.viewport.cursor_pos;
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
        let char_idx =
            editor.line_col_to_char_idx(editor.viewport.cursor_pos.0, editor.viewport.cursor_pos.1);
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
            self.needs_redraw = true;
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
        self.needs_redraw = true;
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

        let paste_line = self.tabs[idx].viewport.cursor_pos.0;
        let insert_pos = self.tabs[idx].rope.line_to_char(paste_line);

        self.tabs[idx].rope.insert(insert_pos, &paste_text);
        self.tabs[idx].modified = true;

        // Nano places the cursor AFTER the pasted text: the line below the
        // last pasted line (clamped to the document), column 0. An inline
        // fragment without newlines leaves the cursor on the paste line.
        let newline_count = paste_text.matches('\n').count();
        let max_line = self.tabs[idx].rope.len_lines().saturating_sub(1);
        self.tabs[idx].viewport.cursor_pos = ((paste_line + newline_count).min(max_line), 0);
        self.tabs[idx].mark_document_changed(paste_line);

        if newline_count > 0 {
            self.set_temporary_status_message(format!("Pasted {newline_count} line(s)"));
        } else {
            self.set_temporary_status_message("Pasted text".to_string());
        }
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
        let insert_line = self.tabs[idx].viewport.cursor_pos.0;
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
        self.tabs[idx].mark_document_changed(insert_line);
        self.needs_redraw = true;
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
        editor.mark_anchor = None;

        let (line, current_col) = editor.viewport.cursor_pos;
        let spaces_to_next_tab = tab_width - (current_col % tab_width.max(1));

        // Insert all the spaces as one rope edit so a single Tab press is
        // one undo step (insert_char would save an undo state per space).
        let spaces = " ".repeat(spaces_to_next_tab);
        let pos = editor.line_col_to_char_idx(line, current_col);
        editor.rope.insert(pos, &spaces);
        editor.viewport.cursor_pos.1 += spaces_to_next_tab;
        editor.modified = true;
        editor.mark_document_changed(line);
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
        use crossterm::event::MouseEventKind;

        // Row 0 is the tab bar -- only respond to clicks, not hover/drag
        let mut adjusted = event;
        if adjusted.row == 0 {
            if matches!(event.kind, MouseEventKind::Down(_)) {
                self.handle_tab_bar_click(adjusted.column as usize);
                self.needs_redraw = true;
            }
            return;
        }
        adjusted.row = adjusted.row.saturating_sub(1);

        // Layout (see draw_ui): tab bar (1 row) + editor + status bar (1) +
        // help bar (1), so the editor pane is terminal_height - 3 rows tall.
        let editor_height = terminal_height.saturating_sub(3);

        // Ignore clicks/drags on the status and help bars -- they must not
        // move the cursor. Scroll events are allowed regardless of row:
        // scrolling with the pointer over the status bar is harmless and
        // matches common terminal-app behavior.
        if matches!(
            adjusted.kind,
            MouseEventKind::Down(_) | MouseEventKind::Drag(_)
        ) && (adjusted.row as usize) >= editor_height
        {
            return;
        }

        let line_num_width = if self.config.show_line_numbers {
            self.active_editor().rope.len_lines().to_string().len() + 1
        } else {
            0
        };
        let editor = self.active_editor_mut();
        editor.handle_mouse_event(adjusted, editor_height, line_num_width);
        self.needs_redraw = true;
    }

    fn handle_tab_bar_click(&mut self, click_col: usize) {
        use unicode_width::UnicodeWidthStr;

        let mut col = 0;

        // Account for left overflow indicator width when tabs are scrolled
        if self.tab_scroll_offset > 0 {
            let left_label = format!(" <{} ", self.tab_scroll_offset);
            col += UnicodeWidthStr::width(left_label.as_str());
        }

        // Start iterating from tab_scroll_offset, matching the rendering order
        for i in self.tab_scroll_offset..self.tabs.len() {
            let tab = &self.tabs[i];
            let modified = if tab.modified { "*" } else { "" };
            let title = format!(" {}{} ", tab.display_name, modified);
            // Display width, not byte length -- must match draw_tab_bar's
            // layout math for multibyte tab names.
            let title_len = UnicodeWidthStr::width(title.as_str());
            if click_col >= col && click_col < col + title_len {
                if i != self.active_tab {
                    // Drop per-tab modes (Find/HexView/...) just like
                    // keyboard tab switching does.
                    self.reset_editor_mode_on_tab_switch();
                    self.active_tab = i;
                }
                return;
            }
            col += title_len;
        }
    }
}

/// Atomically write `bytes` to `path` using the standard temp-file + rename
/// pattern. This protects the user's file from partial writes caused by
/// crashes, power loss, or `kill -9` mid-write.
///
/// Steps:
/// 1. Create a temp file in the same directory as `path` (so rename is on
///    the same filesystem — cross-FS rename is not atomic). The temp is
///    created with `O_EXCL` (`create_new`) so it can never follow or
///    truncate a pre-existing path (e.g. an attacker-planted symlink in a
///    shared directory).
/// 2. On Unix, when overwriting an existing file, tighten the (still empty)
///    temp file's permissions to match the target *before* writing any
///    bytes — see the in-body comment for why this ordering matters.
/// 3. Write bytes, then `sync_all()` to flush to disk before rename.
/// 4. `fs::rename(temp, path)` — POSIX rename is atomic within a filesystem.
/// 5. On any error between steps 1 and 4, best-effort remove the temp file.
fn atomic_write(path: &Path, bytes: &[u8]) -> std::io::Result<()> {
    use std::io::Write;

    let dir = match path.parent() {
        Some(p) if !p.as_os_str().is_empty() => p,
        _ => Path::new("."),
    };

    let stem = path
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_else(|| "rune-save".to_string());

    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    let pid = std::process::id();
    let temp_name = format!(".{stem}.rune-tmp.{pid}.{nanos}");
    let temp_path = dir.join(temp_name);

    // From here on, any early return must best-effort clean up temp_path.
    let result: std::io::Result<()> = (|| {
        // `create_new(true)` => O_EXCL: fail rather than open an existing
        // path. Combined with the pid+nanos name this prevents a symlink or
        // collision at `temp_path` from redirecting/clobbering the write.
        let mut file = std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&temp_path)?;

        // On Unix, preserve the target file's permissions across the rename.
        // `fs::rename` replaces the target inode with the temp file's inode,
        // so without this a restrictive mode (e.g. 0o600) on the original
        // file could be weakened to the umask-default applied at temp creation.
        //
        // Crucially, apply the mode to the *empty* temp file BEFORE writing
        // the contents. The file is created at the umask default (often
        // world-readable 0o644); if we wrote the data first and tightened
        // afterwards, a copy of a secret file's contents would be briefly
        // readable by other local users. Tightening while the file is still
        // empty closes that window. The already-open write handle keeps
        // working even if we set a read-only mode like 0o400, because Unix
        // checks permissions at open() time, not on each write().
        #[cfg(unix)]
        {
            if let Ok(meta) = std::fs::metadata(path) {
                // Best-effort: don't fail the save if we can't apply perms.
                let _ = file.set_permissions(meta.permissions());
            }
        }

        file.write_all(bytes)?;
        file.sync_all()?;
        drop(file);

        std::fs::rename(&temp_path, path)?;
        Ok(())
    })();

    if result.is_err() {
        // Best-effort cleanup; ignore removal errors.
        let _ = std::fs::remove_file(&temp_path);
    }

    result
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
    fn perform_save_writes_content_atomically() {
        let dir = tempfile::tempdir().unwrap();
        let file_path = dir.path().join("atomic.txt");

        let mut t = make_tabs("hello atomic world\n");
        t.active_editor_mut().file_path = Some(file_path.clone());
        t.perform_save(file_path.clone()).unwrap();

        // Content matches rope
        assert_eq!(
            std::fs::read_to_string(&file_path).unwrap(),
            "hello atomic world\n"
        );

        // No lingering .<stem>.rune-tmp.* files
        let lingering: Vec<_> = std::fs::read_dir(dir.path())
            .unwrap()
            .flatten()
            .filter(|e| {
                let name = e.file_name().to_string_lossy().into_owned();
                name.starts_with(".atomic.txt.rune-tmp.")
            })
            .collect();
        assert!(
            lingering.is_empty(),
            "found lingering temp files: {:?}",
            lingering.iter().map(|e| e.file_name()).collect::<Vec<_>>()
        );
    }

    #[test]
    fn perform_save_no_lingering_temp_files_after_success() {
        // Proxy for "cleans up temp on failure" — if normal saves leave
        // stray .rune-tmp files behind, something is wrong with the swap.
        let dir = tempfile::tempdir().unwrap();
        let file_path = dir.path().join("no-leak.txt");

        let mut t = make_tabs("first\n");
        t.active_editor_mut().file_path = Some(file_path.clone());
        t.perform_save(file_path.clone()).unwrap();

        // Save a few more times to exercise the overwrite path.
        t.active_editor_mut().rope = Rope::from_str("second\n");
        t.perform_save(file_path.clone()).unwrap();
        t.active_editor_mut().rope = Rope::from_str("third\n");
        t.perform_save(file_path.clone()).unwrap();

        let entries: Vec<_> = std::fs::read_dir(dir.path())
            .unwrap()
            .flatten()
            .map(|e| e.file_name().to_string_lossy().into_owned())
            .collect();

        let temps: Vec<_> = entries
            .iter()
            .filter(|n| n.starts_with(".no-leak.txt.rune-tmp."))
            .collect();
        assert!(temps.is_empty(), "temp files leaked: {:?}", temps);
        assert_eq!(std::fs::read_to_string(&file_path).unwrap(), "third\n");
    }

    #[test]
    #[cfg(unix)]
    fn perform_save_preserves_mode_on_unix() {
        use std::os::unix::fs::PermissionsExt;

        let dir = tempfile::tempdir().unwrap();
        let file_path = dir.path().join("perms.txt");

        // Seed a target file with a restrictive mode.
        std::fs::write(&file_path, "seed").unwrap();
        let mut perms = std::fs::metadata(&file_path).unwrap().permissions();
        perms.set_mode(0o600);
        std::fs::set_permissions(&file_path, perms).unwrap();

        let mut t = make_tabs("new content\n");
        t.active_editor_mut().file_path = Some(file_path.clone());
        t.perform_save(file_path.clone()).unwrap();

        assert_eq!(
            std::fs::read_to_string(&file_path).unwrap(),
            "new content\n"
        );
        let mode = std::fs::metadata(&file_path).unwrap().permissions().mode();
        // Mask to perm bits; file-type bits vary.
        assert_eq!(mode & 0o777, 0o600, "mode was {:o}", mode & 0o777);
    }

    #[test]
    #[cfg(unix)]
    fn perform_save_into_readonly_mode_file() {
        // Saving over a 0o400 (owner read-only) file must succeed and keep the
        // mode. This is a regression guard for the atomic_write ordering: the
        // restrictive mode is applied to the temp file *after* it's opened for
        // writing, so the open handle can still write even when the resulting
        // mode forbids it. If perms were applied via a fresh open (or before
        // opening the write handle) this would fail with EACCES.
        use std::os::unix::fs::PermissionsExt;

        let dir = tempfile::tempdir().unwrap();
        let file_path = dir.path().join("ro.txt");

        std::fs::write(&file_path, "seed").unwrap();
        let mut perms = std::fs::metadata(&file_path).unwrap().permissions();
        perms.set_mode(0o400);
        std::fs::set_permissions(&file_path, perms).unwrap();

        let mut t = make_tabs("updated\n");
        t.active_editor_mut().file_path = Some(file_path.clone());
        t.perform_save(file_path.clone()).unwrap();

        assert_eq!(std::fs::read_to_string(&file_path).unwrap(), "updated\n");
        let mode = std::fs::metadata(&file_path).unwrap().permissions().mode();
        assert_eq!(mode & 0o777, 0o400, "mode was {:o}", mode & 0o777);
    }

    #[test]
    #[cfg(unix)]
    fn backup_replaces_symlink_instead_of_following_it() {
        // A pre-existing symlink at the backup path ("<file>~") must not
        // redirect the backup copy to the symlink's target. It should be
        // replaced by a regular file containing the saved content.
        use std::os::unix::fs::symlink;

        let dir = tempfile::tempdir().unwrap();
        let file_path = dir.path().join("doc.txt");
        let backup_path = dir.path().join("doc.txt~");
        let outside = dir.path().join("outside.txt");

        std::fs::write(&file_path, "original\n").unwrap();
        std::fs::write(&outside, "DO NOT CLOBBER\n").unwrap();
        // Plant a symlink: doc.txt~ -> outside.txt
        symlink(&outside, &backup_path).unwrap();

        let mut t = make_tabs("edited\n");
        t.config.backup_on_save = true;
        t.active_editor_mut().file_path = Some(file_path.clone());
        t.perform_save(file_path.clone()).unwrap();

        // The redirect target must be untouched.
        assert_eq!(
            std::fs::read_to_string(&outside).unwrap(),
            "DO NOT CLOBBER\n"
        );
        // The backup path is now a regular file (not a symlink) holding the
        // pre-save contents.
        let meta = std::fs::symlink_metadata(&backup_path).unwrap();
        assert!(!meta.file_type().is_symlink(), "backup is still a symlink");
        assert_eq!(std::fs::read_to_string(&backup_path).unwrap(), "original\n");
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
        t.input_buffer = "echo test".to_string();
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
        t.input_buffer = "echo test".to_string();
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
        t.input_buffer = "tr a-z A-Z".to_string();
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

    #[test]
    fn confirm_close_tab_untitled_prompts_for_filename_then_saves_and_closes() {
        use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

        let mut t = make_tabs("precious data\n");
        t.new_tab(); // second tab so closing the first doesn't quit the app
        t.active_tab = 0;
        t.active_editor_mut().modified = true;
        assert!(t.active_editor().file_path.is_none());

        // Alt+W answered with 'y' on an untitled buffer: must NOT close the
        // tab yet — it should prompt for a filename instead.
        t.input_mode = InputMode::ConfirmCloseTab;
        let y = KeyEvent::new(KeyCode::Char('y'), KeyModifiers::NONE);
        let quit = crate::input::handle_key_event(&mut t, y).unwrap();
        assert!(!quit);
        assert_eq!(t.tabs.len(), 2, "tab must not close before the save");
        assert_eq!(t.input_mode, InputMode::EnteringFilename);
        assert_eq!(t.pending_after_save, Some(AfterSave::CloseTab));

        // Enter a filename and confirm: the content is written and only the
        // saved tab closes (the app keeps running with the remaining tab).
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("untitled_close.txt");
        t.input_buffer = path.display().to_string();
        let enter = KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE);
        let quit = crate::input::handle_key_event(&mut t, enter).unwrap();
        assert!(!quit, "other tabs remain, so the app must not exit");
        assert_eq!(t.tabs.len(), 1, "saved tab should now be closed");
        assert_eq!(
            std::fs::read_to_string(&path).unwrap(),
            "precious data\n",
            "buffer content must be written before the tab closes"
        );
        assert_eq!(t.pending_after_save, None);
    }

    #[test]
    fn confirm_close_tab_untitled_save_closes_last_tab_and_quits() {
        use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

        let mut t = make_tabs("only tab\n");
        t.active_editor_mut().modified = true;
        t.input_mode = InputMode::ConfirmCloseTab;
        let y = KeyEvent::new(KeyCode::Char('y'), KeyModifiers::NONE);
        let _ = crate::input::handle_key_event(&mut t, y).unwrap();
        assert_eq!(t.input_mode, InputMode::EnteringFilename);

        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("last_tab.txt");
        t.input_buffer = path.display().to_string();
        let enter = KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE);
        let quit = crate::input::handle_key_event(&mut t, enter).unwrap();
        assert!(quit, "closing the last tab should exit the app");
        assert_eq!(std::fs::read_to_string(&path).unwrap(), "only tab\n");
    }

    #[test]
    fn confirm_close_tab_filename_cancel_keeps_tab_open() {
        use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

        let mut t = make_tabs("keep me\n");
        t.new_tab();
        t.active_tab = 0;
        t.active_editor_mut().modified = true;
        t.input_mode = InputMode::ConfirmCloseTab;
        let y = KeyEvent::new(KeyCode::Char('y'), KeyModifiers::NONE);
        let _ = crate::input::handle_key_event(&mut t, y).unwrap();
        assert_eq!(t.input_mode, InputMode::EnteringFilename);

        // Esc cancels the filename prompt: nothing is closed or saved.
        let esc = KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE);
        let _ = crate::input::handle_key_event(&mut t, esc).unwrap();
        assert_eq!(t.tabs.len(), 2);
        assert_eq!(t.pending_after_save, None);
        assert_eq!(content(&t), "keep me\n");
    }

    #[test]
    fn quit_confirmation_untitled_save_continues_to_next_modified_tab() {
        use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

        // Tab 0: modified untitled; tab 1: also modified. Ctrl+Q -> Y ->
        // filename -> Enter must close tab 0 and prompt for tab 1 instead of
        // exiting the whole app.
        let mut t = make_tabs("first\n");
        t.active_editor_mut().modified = true;
        t.new_tab();
        t.active_editor_mut().rope = Rope::from_str("second\n");
        t.active_editor_mut().modified = true;
        t.active_tab = 0;

        assert!(!t.try_quit());
        assert_eq!(t.input_mode, InputMode::ConfirmQuit);
        let y = KeyEvent::new(KeyCode::Char('y'), KeyModifiers::NONE);
        let quit = crate::input::handle_key_event(&mut t, y).unwrap();
        assert!(!quit);
        assert_eq!(t.input_mode, InputMode::EnteringFilename);
        assert_eq!(t.pending_after_save, Some(AfterSave::ContinueQuit));

        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("first.txt");
        t.input_buffer = path.display().to_string();
        let enter = KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE);
        let quit = crate::input::handle_key_event(&mut t, enter).unwrap();
        assert!(!quit, "a modified tab remains; the app must not exit yet");
        assert_eq!(t.tabs.len(), 1);
        assert_eq!(std::fs::read_to_string(&path).unwrap(), "first\n");
        // The remaining modified tab is now being prompted for.
        assert_eq!(t.input_mode, InputMode::ConfirmQuit);
    }

    // M2: one Tab press must be exactly one undo step.
    #[test]
    fn test_tab_insertion_single_undo() {
        let mut t = make_tabs("");
        t.handle_tab_insertion();
        assert_eq!(content(&t), "    ");
        assert_eq!(t.active_editor().viewport.cursor_pos, (0, 4));
        t.undo();
        assert_eq!(content(&t), "", "one undo should revert one Tab press");
    }

    #[test]
    fn test_tab_insertion_aligns_to_next_tab_stop() {
        let mut t = make_tabs("ab\n");
        t.active_editor_mut().viewport.cursor_pos = (0, 2);
        t.handle_tab_insertion();
        assert_eq!(content(&t), "ab  \n");
        assert_eq!(t.active_editor().viewport.cursor_pos, (0, 4));
    }

    // M3: clicking a different tab in the tab bar must reset per-tab modes
    // (HexView state lives on the Editor being switched away from).
    #[test]
    fn test_tab_bar_click_resets_hex_view_mode() {
        use crossterm::event::{KeyModifiers, MouseButton, MouseEvent, MouseEventKind};

        let mut t = make_tabs("hello\n");
        t.new_tab();
        t.active_tab = 0;
        t.input_mode = InputMode::HexView;

        // Tab 0 title " [untitled] " spans columns 0..12; column 13 is
        // inside tab 1 (" [untitled-2] ").
        let click = MouseEvent {
            kind: MouseEventKind::Down(MouseButton::Left),
            column: 13,
            row: 0,
            modifiers: KeyModifiers::NONE,
        };
        t.handle_mouse_event(click, 24);
        assert_eq!(t.active_tab, 1);
        assert_eq!(t.input_mode, InputMode::Normal);
    }

    #[test]
    fn test_tab_bar_click_same_tab_keeps_mode() {
        use crossterm::event::{KeyModifiers, MouseButton, MouseEvent, MouseEventKind};

        let mut t = make_tabs("hello\n");
        t.new_tab();
        t.active_tab = 0;
        t.input_mode = InputMode::HexView;

        // Column 2 is inside the already-active tab 0 -- no switch, no reset.
        let click = MouseEvent {
            kind: MouseEventKind::Down(MouseButton::Left),
            column: 2,
            row: 0,
            modifiers: KeyModifiers::NONE,
        };
        t.handle_mouse_event(click, 24);
        assert_eq!(t.active_tab, 0);
        assert_eq!(t.input_mode, InputMode::HexView);
    }

    // L5: tab-bar hit-testing must use display width, not byte length.
    #[test]
    fn test_tab_bar_click_multibyte_name_hit_test() {
        use crossterm::event::{KeyModifiers, MouseButton, MouseEvent, MouseEventKind};

        let mut t = make_tabs("a\n");
        t.active_editor_mut().display_name = "日本語".to_string();
        t.new_tab();
        t.active_tab = 0;

        // " 日本語 " renders 8 columns wide (3 CJK chars x 2 + 2 spaces),
        // so tab 1 starts at column 8. Byte-length math (9 bytes + 2 = 11)
        // would wrongly keep a click at column 8 on tab 0.
        let click = MouseEvent {
            kind: MouseEventKind::Down(MouseButton::Left),
            column: 8,
            row: 0,
            modifiers: KeyModifiers::NONE,
        };
        t.handle_mouse_event(click, 24);
        assert_eq!(t.active_tab, 1);
    }

    // M4: clicks on the status/help rows must not move the cursor.
    #[test]
    fn test_mouse_click_on_status_row_ignored() {
        use crossterm::event::{KeyModifiers, MouseButton, MouseEvent, MouseEventKind};

        let mut t = make_tabs("l1\nl2\nl3\nl4\nl5\nl6\nl7\nl8\nl9\n");
        t.active_editor_mut().viewport.cursor_pos = (0, 0);

        // Terminal height 10: tab bar row 0, editor rows 1..=7, status row 8,
        // help row 9.
        for row in [8u16, 9u16] {
            let click = MouseEvent {
                kind: MouseEventKind::Down(MouseButton::Left),
                column: 0,
                row,
                modifiers: KeyModifiers::NONE,
            };
            t.handle_mouse_event(click, 10);
            assert_eq!(
                t.active_editor().viewport.cursor_pos,
                (0, 0),
                "click on row {row} must not move the cursor"
            );
        }
    }

    #[test]
    fn test_mouse_click_in_editor_area_moves_cursor() {
        use crossterm::event::{KeyModifiers, MouseButton, MouseEvent, MouseEventKind};

        let mut t = make_tabs("l1\nl2\nl3\n");
        t.active_editor_mut().viewport.cursor_pos = (0, 0);
        // Row 3 is editor row 2 (tab bar occupies row 0).
        let click = MouseEvent {
            kind: MouseEventKind::Down(MouseButton::Left),
            column: 0,
            row: 3,
            modifiers: KeyModifiers::NONE,
        };
        t.handle_mouse_event(click, 10);
        assert_eq!(t.active_editor().viewport.cursor_pos, (2, 0));
    }

    // L2: untitled naming picks the smallest free suffix.
    #[test]
    fn test_new_tab_untitled_naming_picks_free_suffix() {
        let mut t = TabManager::new_for_test();
        assert_eq!(t.active_editor().display_name, "[untitled]");

        // With "[untitled]" open the first duplicate is "[untitled-2]".
        t.new_tab();
        assert_eq!(t.active_editor().display_name, "[untitled-2]");

        // With only "[untitled-2]" open, plain "[untitled]" is free again.
        t.tabs.remove(0);
        t.active_tab = 0;
        t.new_tab();
        assert_eq!(t.active_editor().display_name, "[untitled]");

        // Both 1 and 2 taken -> next is 3.
        t.new_tab();
        assert_eq!(t.active_editor().display_name, "[untitled-3]");
    }

    // L3: undo/redo on an empty stack must say so instead of flashing
    // "Undo"/"Redo".
    #[test]
    fn test_undo_redo_empty_stack_report_nothing() {
        let mut t = make_tabs("hello\n");
        t.undo();
        assert_eq!(t.status_message, "Nothing to undo");
        t.redo();
        assert_eq!(t.status_message, "Nothing to redo");
        assert!(!t.active_editor().modified);
    }

    // L10: paste places the cursor after the pasted lines (nano behavior).
    #[test]
    fn test_paste_places_cursor_after_pasted_lines() {
        let mut t = make_tabs("line1\nline2\n");
        t.clipboard = vec!["a\nb\n".to_string()];
        t.active_editor_mut().viewport.cursor_pos = (1, 3);
        t.paste();
        assert_eq!(content(&t), "line1\na\nb\nline2\n");
        assert_eq!(t.active_editor().viewport.cursor_pos, (3, 0));
        assert_eq!(t.status_message, "Pasted 2 line(s)");
    }

    #[test]
    fn test_paste_at_last_line_clamps_cursor() {
        let mut t = make_tabs("x\n");
        t.clipboard = vec!["y\n".to_string()];
        t.active_editor_mut().viewport.cursor_pos = (1, 0);
        t.paste();
        assert_eq!(content(&t), "x\ny\n");
        assert_eq!(t.active_editor().viewport.cursor_pos, (2, 0));
    }

    #[test]
    fn test_paste_inline_fragment_reports_text_not_lines() {
        let mut t = make_tabs("hello\n");
        t.clipboard = vec!["abc".to_string()];
        t.active_editor_mut().viewport.cursor_pos = (0, 2);
        t.paste();
        assert_eq!(content(&t), "abchello\n");
        // No newline pasted: cursor stays on the paste line, column 0.
        assert_eq!(t.active_editor().viewport.cursor_pos, (0, 0));
        assert_eq!(t.status_message, "Pasted text");
    }
}
