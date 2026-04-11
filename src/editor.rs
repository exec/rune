use anyhow::Result;
use ropey::Rope;
use std::fs;
use std::io::stdout;
use std::path::PathBuf;
use std::time::{Duration, Instant};
use unicode_width::{UnicodeWidthChar, UnicodeWidthStr};

use crate::config::{self, Config};
use crate::constants;
use crate::hex::HexViewState;
use crate::search::{FindNavigationMode, ReplacePhase, SearchState};
use crate::syntax::SyntaxHighlighter;

#[derive(Clone, Debug)]
pub struct UndoState {
    pub rope: Rope,
    pub cursor_pos: (usize, usize),
}

/// Viewport and cursor state
#[derive(Default)]
pub struct ViewportState {
    pub cursor_pos: (usize, usize),
    pub viewport_offset: (usize, usize),
}

/// Undo/redo management
#[derive(Default)]
pub struct UndoManager {
    pub undo_stack: Vec<UndoState>,
    pub redo_stack: Vec<UndoState>,
}

impl UndoManager {
    pub fn save_state(&mut self, rope: &Rope, cursor_pos: (usize, usize)) {
        let state = UndoState {
            rope: rope.clone(),
            cursor_pos,
        };
        self.undo_stack.push(state);
        self.redo_stack.clear();

        if self.undo_stack.len() > constants::UNDO_STACK_LIMIT {
            self.undo_stack.remove(0);
        }
    }

    /// Apply undo or redo. `is_undo=true` pops from undo_stack, pushes to redo_stack.
    fn apply(
        &mut self,
        is_undo: bool,
        rope: &mut Rope,
        cursor_pos: &mut (usize, usize),
    ) -> bool {
        let (from, to) = if is_undo {
            (&mut self.undo_stack, &mut self.redo_stack)
        } else {
            (&mut self.redo_stack, &mut self.undo_stack)
        };

        if let Some(state) = from.pop() {
            let current = UndoState {
                rope: rope.clone(),
                cursor_pos: *cursor_pos,
            };
            to.push(current);
            *rope = state.rope;
            *cursor_pos = state.cursor_pos;
            true
        } else {
            false
        }
    }

    pub fn undo(&mut self, rope: &mut Rope, cursor_pos: &mut (usize, usize)) -> bool {
        self.apply(true, rope, cursor_pos)
    }

    pub fn redo(&mut self, rope: &mut Rope, cursor_pos: &mut (usize, usize)) -> bool {
        self.apply(false, rope, cursor_pos)
    }
}

/// Different input modes the editor can be in
#[derive(Debug, Clone, PartialEq)]
pub enum InputMode {
    Normal,
    EnteringFilename,
    EnteringSaveAs,
    ConfirmQuit,
    OptionsMenu,
    Find,
    FindOptionsMenu,
    Replace,
    ReplaceConfirm,
    GoToLine,
    Help,
    HexView,
}

/// Main editor state
pub struct Editor {
    pub rope: Rope,
    pub viewport: ViewportState,
    pub file_path: Option<PathBuf>,
    pub modified: bool,
    pub status_message: String,
    pub status_message_time: Option<Instant>,
    pub status_message_timeout: Duration,
    pub highlighter: SyntaxHighlighter,
    pub syntax_name: Option<String>,
    pub input_mode: InputMode,
    pub filename_buffer: String,
    pub quit_after_save: bool,
    pub config: Config,
    pub search: SearchState,
    pub undo_manager: UndoManager,
    pub needs_redraw: bool,
    pub cached_text: Option<String>,
    pub cache_valid: bool,
    pub hex_state: Option<HexViewState>,
    pub clipboard: Vec<String>,
    pub last_cut_line: Option<usize>,
    pub mark_anchor: Option<(usize, usize)>,
}

/// Get the display width of a line, handling the case where the line spans chunk boundaries.
pub fn line_display_width(rope: &Rope, line: usize) -> usize {
    let rope_line = rope.line(line);
    if let Some(s) = rope_line.as_str() {
        s.trim_end_matches('\n').width()
    } else {
        rope_line.chars().filter(|&c| c != '\n').map(|c| UnicodeWidthChar::width(c).unwrap_or(0)).sum()
    }
}


impl Editor {
    pub fn new() -> Self {
        let config = config::load_config();
        Self {
            rope: Rope::new(),
            viewport: ViewportState::default(),
            file_path: None,
            modified: false,
            status_message: String::new(),
            status_message_time: None,
            status_message_timeout: constants::STATUS_MESSAGE_TIMEOUT,
            highlighter: SyntaxHighlighter::new(),
            syntax_name: None,
            input_mode: InputMode::Normal,
            filename_buffer: String::new(),
            quit_after_save: false,
            config,
            search: SearchState::default(),
            undo_manager: UndoManager::default(),
            needs_redraw: true,
            cached_text: None,
            cache_valid: false,
            hex_state: None,
            clipboard: Vec::new(),
            last_cut_line: None,
            mark_anchor: None,
        }
    }

    pub fn new_for_test() -> Self {
        Self {
            rope: Rope::new(),
            viewport: ViewportState::default(),
            file_path: None,
            modified: false,
            status_message: String::new(),
            status_message_time: None,
            status_message_timeout: constants::STATUS_MESSAGE_TIMEOUT,
            highlighter: SyntaxHighlighter::new(),
            syntax_name: None,
            input_mode: InputMode::Normal,
            filename_buffer: String::new(),
            quit_after_save: false,
            config: Config::default(),
            search: SearchState::default(),
            undo_manager: UndoManager::default(),
            needs_redraw: true,
            cached_text: None,
            cache_valid: false,
            hex_state: None,
            clipboard: Vec::new(),
            last_cut_line: None,
            mark_anchor: None,
        }
    }

    pub fn load_file(&mut self, path: PathBuf) -> Result<()> {
        let content = fs::read_to_string(&path)?;
        self.rope = Rope::from_str(&content);

        let first_line = self.rope.line(0).as_str().map(|s| s.trim_end_matches('\n'));
        self.syntax_name = self.highlighter.detect_syntax(Some(&path), first_line);
        self.highlighter.set_syntax(self.syntax_name.as_deref());

        self.file_path = Some(path);
        self.modified = false;
        Ok(())
    }

    pub fn save_file(&mut self) -> Result<()> {
        if let Some(path) = &self.file_path {
            self.perform_save(path.clone())?;
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
            .file_path
            .as_ref()
            .map(|p| p.display().to_string())
            .unwrap_or_default();
        self.status_message = format!("File Name to Write: {}", self.filename_buffer);
        self.needs_redraw = true;
    }

    pub fn perform_save(&mut self, path: PathBuf) -> Result<()> {
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
        }

        match fs::write(&path, self.rope.to_string()) {
            Ok(()) => {
                self.file_path = Some(path.clone());
                self.modified = false;

                let first_line = self.rope.line(0).as_str().map(|s| s.trim_end_matches('\n'));
                self.syntax_name = self.highlighter.detect_syntax(Some(&path), first_line);
                self.highlighter.set_syntax(self.syntax_name.as_deref());

                self.set_temporary_status_message(format!("Saved: {}", path.display()));
            }
            Err(e) => {
                self.set_temporary_status_message(format!("Error saving file: {e}"));
            }
        }
        Ok(())
    }

    pub fn finish_filename_input(&mut self) -> Result<bool> {
        if self.filename_buffer.is_empty() {
            self.status_message = "Cancelled".to_string();
            self.input_mode = InputMode::Normal;
            self.quit_after_save = false;
            return Ok(false);
        }

        let path = PathBuf::from(&self.filename_buffer);
        self.perform_save(path)?;
        self.input_mode = InputMode::Normal;
        self.filename_buffer.clear();

        let should_quit = self.quit_after_save && !self.modified;
        self.quit_after_save = false;
        Ok(should_quit)
    }

    pub fn cancel_filename_input(&mut self) {
        self.input_mode = InputMode::Normal;
        self.filename_buffer.clear();
        self.quit_after_save = false;
        self.status_message = "Cancelled".to_string();
    }

    pub fn try_quit(&mut self) -> bool {
        if self.modified {
            self.start_quit_confirmation();
            false
        } else {
            true
        }
    }

    fn start_quit_confirmation(&mut self) {
        self.input_mode = InputMode::ConfirmQuit;
        self.status_message = "Save modified buffer? (Y/N/Ctrl+C)".to_string();
        self.needs_redraw = true;
    }

    pub fn handle_quit_confirmation(&mut self, save: bool) -> Result<bool> {
        self.input_mode = InputMode::Normal;

        if save {
            if self.file_path.is_some() {
                self.save_file()?;
                if !self.modified {
                    Ok(true)
                } else {
                    Ok(false)
                }
            } else {
                self.quit_after_save = true;
                self.start_filename_input();
                Ok(false)
            }
        } else {
            Ok(true)
        }
    }

    pub fn cancel_quit_confirmation(&mut self) {
        self.input_mode = InputMode::Normal;
        self.status_message = "Cancelled".to_string();
        self.needs_redraw = true;
    }

    pub fn insert_char(&mut self, c: char) {
        self.mark_anchor = None;
        self.save_undo_state();
        let pos = self.line_col_to_char_idx(self.viewport.cursor_pos.0, self.viewport.cursor_pos.1);
        self.rope.insert_char(pos, c);
        self.mark_document_changed(self.viewport.cursor_pos.0);
        self.move_cursor_right();
        self.modified = true;
    }

    pub fn delete_char(&mut self) {
        self.mark_anchor = None;
        if self.viewport.cursor_pos.1 > 0 {
            let pos = self.line_col_to_char_idx(self.viewport.cursor_pos.0, self.viewport.cursor_pos.1);
            if pos > 0 {
                self.save_undo_state();
                self.rope.remove(pos - 1..pos);
                self.mark_document_changed(self.viewport.cursor_pos.0);
                self.move_cursor_left();
                self.modified = true;
            }
        } else if self.viewport.cursor_pos.0 > 0 {
            let pos = self.line_col_to_char_idx(self.viewport.cursor_pos.0, 0);
            if pos > 0 {
                self.save_undo_state();

                let junction_col = line_display_width(&self.rope, self.viewport.cursor_pos.0 - 1);

                self.rope.remove(pos - 1..pos);
                self.mark_document_changed(self.viewport.cursor_pos.0 - 1);
                self.viewport.cursor_pos.0 -= 1;
                self.viewport.cursor_pos.1 = junction_col;
                self.modified = true;
            }
        }
    }

    pub fn insert_newline(&mut self) {
        self.mark_anchor = None;
        self.save_undo_state();
        let pos = self.line_col_to_char_idx(self.viewport.cursor_pos.0, self.viewport.cursor_pos.1);

        // Collect leading whitespace from current line if auto_indent is enabled
        let indent = if self.config.auto_indent {
            let line = self.rope.line(self.viewport.cursor_pos.0);
            let mut ws = String::new();
            for ch in line.chars() {
                if ch == ' ' || ch == '\t' {
                    ws.push(ch);
                } else {
                    break;
                }
            }
            ws
        } else {
            String::new()
        };

        let insert_str = format!("\n{}", indent);
        self.rope.insert(pos, &insert_str);
        self.mark_document_changed(self.viewport.cursor_pos.0);
        self.viewport.cursor_pos.0 += 1;
        self.viewport.cursor_pos.1 = indent.len();
        self.modified = true;
    }

    pub fn delete_char_forward(&mut self) {
        let pos = self.line_col_to_char_idx(self.viewport.cursor_pos.0, self.viewport.cursor_pos.1);
        if pos < self.rope.len_chars() {
            self.save_undo_state();
            self.rope.remove(pos..pos + 1);
            self.mark_document_changed(self.viewport.cursor_pos.0);
            self.modified = true;
        }
    }

    pub fn move_cursor_up(&mut self) {
        if self.viewport.cursor_pos.0 > 0 {
            self.viewport.cursor_pos.0 -= 1;
            self.clamp_cursor_to_line();
            self.needs_redraw = true;
        }
    }

    pub fn move_cursor_down(&mut self) {
        if self.viewport.cursor_pos.0 < self.rope.len_lines().saturating_sub(1) {
            self.viewport.cursor_pos.0 += 1;
            self.clamp_cursor_to_line();
            self.needs_redraw = true;
        }
    }

    pub fn move_cursor_left(&mut self) {
        if self.viewport.cursor_pos.1 > 0 {
            self.viewport.cursor_pos.1 -= 1;
            self.needs_redraw = true;
        } else if self.viewport.cursor_pos.0 > 0 {
            self.viewport.cursor_pos.0 -= 1;
            self.viewport.cursor_pos.1 = line_display_width(&self.rope, self.viewport.cursor_pos.0);
            self.needs_redraw = true;
        }
    }

    pub fn move_cursor_right(&mut self) {
        let line_len = line_display_width(&self.rope, self.viewport.cursor_pos.0);
        if self.viewport.cursor_pos.1 < line_len {
            self.viewport.cursor_pos.1 += 1;
            self.needs_redraw = true;
        } else if self.viewport.cursor_pos.0 < self.rope.len_lines().saturating_sub(1) {
            self.viewport.cursor_pos.0 += 1;
            self.viewport.cursor_pos.1 = 0;
            self.needs_redraw = true;
        }
    }

    pub fn page_up(&mut self) {
        let page_size = constants::FALLBACK_TERMINAL_HEIGHT.saturating_sub(4);
        self.viewport.cursor_pos.0 = self.viewport.cursor_pos.0.saturating_sub(page_size);
        self.clamp_cursor_to_line();
        self.needs_redraw = true;
    }

    pub fn page_down(&mut self) {
        let page_size = constants::FALLBACK_TERMINAL_HEIGHT.saturating_sub(4);
        let max_line = self.rope.len_lines().saturating_sub(1);
        self.viewport.cursor_pos.0 = (self.viewport.cursor_pos.0 + page_size).min(max_line);
        self.clamp_cursor_to_line();
        self.needs_redraw = true;
    }

    pub fn clamp_cursor_to_line(&mut self) {
        let line_len = line_display_width(&self.rope, self.viewport.cursor_pos.0);
        self.viewport.cursor_pos.1 = self.viewport.cursor_pos.1.min(line_len);
    }

    pub fn line_col_to_char_idx(&self, line: usize, col: usize) -> usize {
        let line_start = self.rope.line_to_char(line);
        let rope_line = self.rope.line(line);
        let mut char_idx = 0;
        let mut display_col = 0;
        for (i, ch) in rope_line.chars().enumerate() {
            if display_col >= col || ch == '\n' {
                break;
            }
            char_idx = i + 1;
            display_col += UnicodeWidthChar::width(ch).unwrap_or(0);
        }
        line_start + char_idx
    }

    /// Update viewport scroll offset to keep cursor visible within the given editor area height.
    /// This must be called with the actual rendered area height to avoid mismatches.
    pub fn update_viewport_for_height(&mut self, editor_height: usize) {
        if editor_height == 0 {
            return;
        }

        // Clamp cursor line to valid document range
        let max_line = self.rope.len_lines().saturating_sub(1);
        if self.viewport.cursor_pos.0 > max_line {
            self.viewport.cursor_pos.0 = max_line;
            self.clamp_cursor_to_line();
        }

        // Scroll up if cursor is above viewport
        if self.viewport.cursor_pos.0 < self.viewport.viewport_offset.0 {
            self.viewport.viewport_offset.0 = self.viewport.cursor_pos.0;
        }

        // Scroll down if cursor is below viewport
        if self.viewport.cursor_pos.0 >= self.viewport.viewport_offset.0 + editor_height {
            self.viewport.viewport_offset.0 = self
                .viewport
                .cursor_pos
                .0
                .saturating_sub(editor_height.saturating_sub(1));
        }

        // Clamp viewport offset so we don't scroll past the end of the document
        let max_offset = max_line.saturating_sub(editor_height.saturating_sub(1));
        if self.viewport.viewport_offset.0 > max_offset {
            self.viewport.viewport_offset.0 = max_offset;
        }
    }

    pub fn handle_mouse_event(
        &mut self,
        event: crossterm::event::MouseEvent,
        terminal_height: usize,
    ) {
        use crossterm::event::MouseEventKind;
        match event.kind {
            MouseEventKind::Down(_) => {
                let clicked_line = self.viewport.viewport_offset.0 + event.row as usize;
                let clicked_col = event.column as usize;

                if clicked_line < self.rope.len_lines() {
                    self.viewport.cursor_pos.0 = clicked_line;
                    self.viewport.cursor_pos.1 = clicked_col;
                    self.clamp_cursor_to_line();
                    self.needs_redraw = true;
                }
            }
            MouseEventKind::Drag(_) => {
                self.needs_redraw = true;
            }
            MouseEventKind::ScrollDown => {
                if self.viewport.viewport_offset.0
                    < self.rope.len_lines().saturating_sub(terminal_height)
                {
                    self.viewport.viewport_offset.0 += constants::SCROLL_SPEED;
                    self.needs_redraw = true;
                }
            }
            MouseEventKind::ScrollUp => {
                self.viewport.viewport_offset.0 = self
                    .viewport
                    .viewport_offset
                    .0
                    .saturating_sub(constants::SCROLL_SPEED);
                self.needs_redraw = true;
            }
            _ => {}
        }
    }

    pub fn toggle_mouse_mode(&mut self) {
        self.config.mouse_enabled = !self.config.mouse_enabled;

        if self.config.mouse_enabled {
            let _ = crossterm::execute!(stdout(), crossterm::event::EnableMouseCapture);
            self.set_temporary_status_message("Mouse mode enabled".to_string());
        } else {
            let _ = crossterm::execute!(stdout(), crossterm::event::DisableMouseCapture);
            self.set_temporary_status_message("Mouse mode disabled".to_string());
        }
    }

    pub fn open_options_menu(&mut self) {
        self.input_mode = InputMode::OptionsMenu;
        self.status_message = "Options Menu".to_string();
        self.needs_redraw = true;
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

    pub fn save_config(&self) {
        let _ = config::save_config(&self.config);
    }

    fn save_undo_state(&mut self) {
        self.undo_manager
            .save_state(&self.rope, self.viewport.cursor_pos);
    }

    pub fn undo(&mut self) {
        if self.undo_manager.undo(&mut self.rope, &mut self.viewport.cursor_pos) {
            self.modified = true;
            self.invalidate_cache();
            self.needs_redraw = true;
            self.highlighter.invalidate_cache_from_line(0);
            self.set_temporary_status_message("Undo".to_string());
        }
    }

    pub fn redo(&mut self) {
        if self.undo_manager.redo(&mut self.rope, &mut self.viewport.cursor_pos) {
            self.modified = true;
            self.invalidate_cache();
            self.needs_redraw = true;
            self.highlighter.invalidate_cache_from_line(0);
            self.set_temporary_status_message("Redo".to_string());
        }
    }

    pub fn start_find(&mut self) {
        self.input_mode = InputMode::Find;
        self.search.search_buffer.clear();
        self.search.search_matches.clear();
        self.search.current_match_index = None;
        self.status_message = "Find: ".to_string();
        self.search.find_navigation_mode = FindNavigationMode::HistoryBrowsing;
        self.needs_redraw = true;
    }

    pub fn start_replace(&mut self) {
        self.input_mode = InputMode::Replace;
        self.search.search_buffer.clear();
        self.search.replace_buffer.clear();
        self.search.replace_phase = ReplacePhase::FindPattern;
        self.status_message = "Find: ".to_string();
        self.needs_redraw = true;
    }

    pub fn start_goto_line(&mut self) {
        self.input_mode = InputMode::GoToLine;
        self.search.goto_line_buffer.clear();
        self.status_message = "Go to line: ".to_string();
        self.needs_redraw = true;
    }

    pub fn toggle_hex_view(&mut self) {
        if self.input_mode == InputMode::HexView {
            // Restore text cursor from hex cursor byte offset
            if let Some(state) = &self.hex_state {
                let byte_offset = state.cursor;
                // Convert byte offset to char index, then to (line, col)
                let text = self.rope.to_string();
                let char_idx = text[..byte_offset.min(text.len())]
                    .chars()
                    .count();
                let line = self.rope.char_to_line(char_idx.min(self.rope.len_chars().saturating_sub(1)));
                let line_start = self.rope.line_to_char(line);
                let col_chars = char_idx.saturating_sub(line_start);
                // Convert char offset within line to display column
                let mut display_col = 0;
                for (i, ch) in self.rope.line(line).chars().enumerate() {
                    if i >= col_chars || ch == '\n' {
                        break;
                    }
                    display_col += UnicodeWidthChar::width(ch).unwrap_or(0);
                }
                self.viewport.cursor_pos = (line, display_col);
            }
            self.hex_state = None;
            self.input_mode = InputMode::Normal;
            self.needs_redraw = true;
            return;
        }

        // Get bytes from the live rope content
        let bytes = self.rope.to_string().into_bytes();

        // Convert text cursor position to byte offset
        let char_idx = self.line_col_to_char_idx(
            self.viewport.cursor_pos.0,
            self.viewport.cursor_pos.1,
        );
        let text = self.rope.to_string();
        let byte_offset = text
            .char_indices()
            .nth(char_idx)
            .map(|(i, _)| i)
            .unwrap_or(text.len());

        let mut state = HexViewState::new(bytes);
        state.cursor = byte_offset.min(state.raw_bytes.len().saturating_sub(1));
        self.hex_state = Some(state);
        self.input_mode = InputMode::HexView;
        self.needs_redraw = true;
    }

    pub fn perform_find(&mut self, search_term: &str) -> bool {
        if search_term.is_empty() {
            self.search.search_matches.clear();
            self.search.current_match_index = None;
            return false;
        }

        self.search.search_start_pos = self.viewport.cursor_pos;
        self.search.search_buffer = search_term.to_string();
        self.search.search_matches = self.search.find_all_matches(&self.rope);

        if !self.search.search_matches.is_empty() {
            let cursor_char_idx = self.line_col_to_char_idx(
                self.viewport.cursor_pos.0,
                self.viewport.cursor_pos.1,
            );

            self.search.current_match_index = self
                .search
                .search_matches
                .iter()
                .position(|(line, col)| {
                    let match_char_idx = self.line_col_to_char_idx(*line, *col);
                    match_char_idx >= cursor_char_idx
                })
                .or(Some(0));

            if let Some(index) = self.search.current_match_index {
                if let Some(&(line, col)) = self.search.search_matches.get(index) {
                    self.viewport.cursor_pos = (line, col);
                    self.clamp_cursor_to_line();
                    self.viewport.viewport_offset.0 = line;
                    self.needs_redraw = true;
                } else {
                    self.search.current_match_index = None;
                }
            }

            true
        } else {
            self.search.current_match_index = None;
            false
        }
    }

    pub fn find_next_match(&mut self) -> bool {
        if let Some((line, col)) = self.search.navigate_match(true) {
            self.viewport.cursor_pos = (line, col);
            self.clamp_cursor_to_line();
            self.viewport.viewport_offset.0 = line;
            self.needs_redraw = true;
            true
        } else {
            false
        }
    }

    pub fn find_previous_match(&mut self) -> bool {
        if let Some((line, col)) = self.search.navigate_match(false) {
            self.viewport.cursor_pos = (line, col);
            self.clamp_cursor_to_line();
            self.viewport.viewport_offset.0 = line;
            self.needs_redraw = true;
            true
        } else {
            false
        }
    }

    pub fn cancel_search(&mut self) {
        let start_pos = self.search.cancel_search();
        self.viewport.cursor_pos = start_pos;
    }

    pub fn perform_replace(&mut self, search_term: &str, replace_term: &str) -> usize {
        if search_term.is_empty() {
            return 0;
        }

        self.save_undo_state();
        let text = self.rope.to_string();
        let new_text = text.replace(search_term, replace_term);
        let replacements = text.matches(search_term).count();

        if replacements > 0 {
            self.rope = Rope::from_str(&new_text);
            self.modified = true;
            self.clamp_cursor_to_line();
            self.invalidate_cache();
            self.needs_redraw = true;
            self.highlighter.invalidate_cache_from_line(0);
        }

        replacements
    }

    pub fn perform_replace_interactive(&mut self, search_term: &str, replace_term: &str) -> usize {
        if search_term.is_empty() {
            return 0;
        }

        self.save_undo_state();
        let text = self.rope.to_string();

        if let Some(pos) = text.find(search_term) {
            let mut new_text = text.clone();
            new_text.replace_range(pos..pos + search_term.len(), replace_term);

            self.rope = Rope::from_str(&new_text);
            self.modified = true;

            let line = self.rope.char_to_line(pos);
            let line_start = self.rope.line_to_char(line);
            let col = pos - line_start;
            self.viewport.cursor_pos = (line, col);
            self.clamp_cursor_to_line();

            self.invalidate_cache();
            self.needs_redraw = true;
            self.highlighter.invalidate_cache_from_line(0);

            return 1;
        }

        0
    }

    pub fn goto_line(&mut self, line_num: usize) {
        if line_num > 0 && line_num <= self.rope.len_lines() {
            self.viewport.cursor_pos.0 = line_num - 1;
            self.viewport.cursor_pos.1 = 0;
            self.clamp_cursor_to_line();
            self.set_temporary_status_message(format!("Jumped to line {line_num}"));
        } else {
            self.set_temporary_status_message(format!("Invalid line number: {line_num}"));
        }
    }

    pub fn toggle_regex_mode(&mut self) {
        self.search.use_regex = !self.search.use_regex;
        let mode = if self.search.use_regex {
            "Regex"
        } else {
            "Literal"
        };
        self.set_temporary_status_message(format!(
            "Search mode: {} ({})",
            mode,
            if self.search.use_regex {
                "Pattern matching"
            } else {
                "Exact text"
            }
        ));
        self.needs_redraw = true;

        if !self.search.search_buffer.is_empty() && self.input_mode == InputMode::Find {
            let search_term = self.search.search_buffer.clone();
            self.perform_find(&search_term);
        }
    }

    pub fn toggle_case_sensitive(&mut self) {
        self.search.case_sensitive = !self.search.case_sensitive;
        let mode = if self.search.case_sensitive {
            "Case sensitive"
        } else {
            "Case insensitive"
        };
        self.set_temporary_status_message(format!("Search: {}", mode));
        self.needs_redraw = true;

        if !self.search.search_buffer.is_empty() && self.input_mode == InputMode::Find {
            let search_term = self.search.search_buffer.clone();
            self.perform_find(&search_term);
        }
    }

    pub fn handle_tab_insertion(&mut self) {
        self.save_undo_state();

        let current_col = self.viewport.cursor_pos.1;
        let tab_width = self.config.tab_width.max(1);
        let spaces_to_next_tab = tab_width - (current_col % tab_width);

        for _ in 0..spaces_to_next_tab {
            self.insert_char(' ');
        }
    }

    fn invalidate_cache(&mut self) {
        self.cache_valid = false;
        self.cached_text = None;
    }

    /// Invalidate highlighting and text caches from a given line onwards
    fn mark_document_changed(&mut self, from_line: usize) {
        self.highlighter.invalidate_cache_from_line(from_line);
        self.invalidate_cache();
        self.needs_redraw = true;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_delete_forward() {
        let mut e = Editor::new_for_test();
        e.rope = Rope::from_str("hello\n");
        e.viewport.cursor_pos = (0, 0);
        e.delete_char_forward();
        assert_eq!(e.rope.to_string(), "ello\n");
    }

    #[test]
    fn test_delete_forward_joins_lines() {
        let mut e = Editor::new_for_test();
        e.rope = Rope::from_str("ab\ncd\n");
        e.viewport.cursor_pos = (0, 2);
        e.delete_char_forward();
        assert_eq!(e.rope.to_string(), "abcd\n");
    }

    #[test]
    fn test_delete_forward_at_end_of_document() {
        let mut e = Editor::new_for_test();
        e.rope = Rope::from_str("hi");
        e.viewport.cursor_pos = (0, 2);
        e.delete_char_forward();
        assert_eq!(e.rope.to_string(), "hi");
    }

    #[test]
    fn test_auto_indent() {
        let mut e = Editor::new_for_test();
        e.config.auto_indent = true;
        e.rope = Rope::from_str("    hello\n");
        e.viewport.cursor_pos = (0, 9);
        e.insert_newline();
        assert!(e.rope.to_string().starts_with("    hello\n    "));
    }

    #[test]
    fn test_no_auto_indent_when_disabled() {
        let mut e = Editor::new_for_test();
        e.config.auto_indent = false;
        e.rope = Rope::from_str("    hello\n");
        e.viewport.cursor_pos = (0, 9);
        e.insert_newline();
        assert_eq!(e.rope.to_string(), "    hello\n\n");
    }
}
