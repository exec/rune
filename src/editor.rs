use anyhow::Result;
use ropey::Rope;
use std::collections::VecDeque;
use std::fs;
use std::path::PathBuf;
use unicode_width::{UnicodeWidthChar, UnicodeWidthStr};

use crate::constants;
use crate::hex::HexViewState;
use crate::search::SearchState;
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
    pub undo_stack: VecDeque<UndoState>,
    pub redo_stack: VecDeque<UndoState>,
}

impl UndoManager {
    pub fn save_state(&mut self, rope: &Rope, cursor_pos: (usize, usize)) {
        let state = UndoState {
            rope: rope.clone(),
            cursor_pos,
        };
        self.undo_stack.push_back(state);
        self.redo_stack.clear();

        if self.undo_stack.len() > constants::UNDO_STACK_LIMIT {
            self.undo_stack.pop_front();
        }
    }

    /// Apply undo or redo. `is_undo=true` pops from undo_stack, pushes to redo_stack.
    fn apply(&mut self, is_undo: bool, rope: &mut Rope, cursor_pos: &mut (usize, usize)) -> bool {
        let (from, to) = if is_undo {
            (&mut self.undo_stack, &mut self.redo_stack)
        } else {
            (&mut self.redo_stack, &mut self.undo_stack)
        };

        if let Some(state) = from.pop_back() {
            let current = UndoState {
                rope: rope.clone(),
                cursor_pos: *cursor_pos,
            };
            to.push_back(current);
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
    ConfirmCloseTab,
    OptionsMenu,
    Find,
    FindOptionsMenu,
    Replace,
    ReplaceConfirm,
    GoToLine,
    HexView,
    OpenFileCurrentTab,
    OpenFileNewTab,
    FuzzyFinder,
    VerbatimInput,
    ExecuteCommand,
    ConfirmExecute,
}

/// Main editor state — represents a single buffer/tab.
/// Shared state (config, clipboard, input_mode, status_message, etc.)
/// lives on TabManager.
pub struct Editor {
    pub rope: Rope,
    pub viewport: ViewportState,
    pub file_path: Option<PathBuf>,
    pub display_name: String,
    pub modified: bool,
    pub highlighter: SyntaxHighlighter,
    pub syntax_name: Option<String>,
    pub search: SearchState,
    pub undo_manager: UndoManager,
    pub hex_state: Option<HexViewState>,
    pub mark_anchor: Option<(usize, usize)>,
}

/// Get the display width of a line, handling the case where the line spans chunk boundaries.
pub fn line_display_width(rope: &Rope, line: usize) -> usize {
    let rope_line = rope.line(line);
    if let Some(s) = rope_line.as_str() {
        s.trim_end_matches('\n').width()
    } else {
        rope_line
            .chars()
            .filter(|&c| c != '\n')
            .map(|c| UnicodeWidthChar::width(c).unwrap_or(0))
            .sum()
    }
}

impl Default for Editor {
    fn default() -> Self {
        Self::new_buffer()
    }
}

impl Editor {
    /// Create a new buffer-only Editor (no shared state).
    pub fn new_buffer() -> Self {
        Self {
            rope: Rope::new(),
            viewport: ViewportState::default(),
            file_path: None,
            display_name: "[untitled]".to_string(),
            modified: false,
            highlighter: SyntaxHighlighter::new(),
            syntax_name: None,
            search: SearchState::default(),
            undo_manager: UndoManager::default(),
            hex_state: None,
            mark_anchor: None,
        }
    }

    /// Create an Editor for unit tests (same as new_buffer).
    pub fn new_for_test() -> Self {
        Self::new_buffer()
    }

    pub fn load_file(&mut self, path: PathBuf) -> Result<()> {
        let content = fs::read_to_string(&path)?;
        self.rope = Rope::from_str(&content);

        let first_line = self.rope.line(0).as_str().map(|s| s.trim_end_matches('\n'));
        self.syntax_name = self.highlighter.detect_syntax(Some(&path), first_line);
        self.highlighter.set_syntax(self.syntax_name.as_deref());

        self.display_name = path
            .file_name()
            .map(|n| n.to_string_lossy().to_string())
            .unwrap_or_else(|| "[untitled]".to_string());
        self.file_path = Some(path);
        self.modified = false;
        Ok(())
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
            let pos =
                self.line_col_to_char_idx(self.viewport.cursor_pos.0, self.viewport.cursor_pos.1);
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

    pub fn insert_newline(&mut self, auto_indent: bool) {
        self.mark_anchor = None;
        self.save_undo_state();
        let pos = self.line_col_to_char_idx(self.viewport.cursor_pos.0, self.viewport.cursor_pos.1);

        // Collect leading whitespace from current line if auto_indent is enabled
        let indent = if auto_indent {
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
        self.viewport.cursor_pos.1 = UnicodeWidthStr::width(indent.as_str());
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
        }
    }

    pub fn move_cursor_down(&mut self) {
        if self.viewport.cursor_pos.0 < self.rope.len_lines().saturating_sub(1) {
            self.viewport.cursor_pos.0 += 1;
            self.clamp_cursor_to_line();
        }
    }

    pub fn move_cursor_left(&mut self) {
        if self.viewport.cursor_pos.1 > 0 {
            self.viewport.cursor_pos.1 -= 1;
        } else if self.viewport.cursor_pos.0 > 0 {
            self.viewport.cursor_pos.0 -= 1;
            self.viewport.cursor_pos.1 = line_display_width(&self.rope, self.viewport.cursor_pos.0);
        }
    }

    pub fn move_cursor_right(&mut self) {
        let line_len = line_display_width(&self.rope, self.viewport.cursor_pos.0);
        if self.viewport.cursor_pos.1 < line_len {
            self.viewport.cursor_pos.1 += 1;
        } else if self.viewport.cursor_pos.0 < self.rope.len_lines().saturating_sub(1) {
            self.viewport.cursor_pos.0 += 1;
            self.viewport.cursor_pos.1 = 0;
        }
    }

    pub fn page_up(&mut self) {
        let page_size = constants::FALLBACK_TERMINAL_HEIGHT.saturating_sub(4);
        self.viewport.cursor_pos.0 = self.viewport.cursor_pos.0.saturating_sub(page_size);
        self.clamp_cursor_to_line();
    }

    pub fn page_down(&mut self) {
        let page_size = constants::FALLBACK_TERMINAL_HEIGHT.saturating_sub(4);
        let max_line = self.rope.len_lines().saturating_sub(1);
        self.viewport.cursor_pos.0 = (self.viewport.cursor_pos.0 + page_size).min(max_line);
        self.clamp_cursor_to_line();
    }

    pub fn clamp_cursor_to_line(&mut self) {
        let line_len = line_display_width(&self.rope, self.viewport.cursor_pos.0);
        self.viewport.cursor_pos.1 = self.viewport.cursor_pos.1.min(line_len);
    }

    /// Convert a char offset (number of chars from line start) to a display column,
    /// accounting for character widths (e.g. wide CJK characters).
    pub fn char_idx_to_display_col(&self, line: usize, char_offset: usize) -> usize {
        let rope_line = self.rope.line(line);
        let mut display_col = 0;
        for (i, ch) in rope_line.chars().enumerate() {
            if i >= char_offset || ch == '\n' {
                break;
            }
            display_col += UnicodeWidthChar::width(ch).unwrap_or(0);
        }
        display_col
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
    pub fn update_viewport_for_height(&mut self, editor_height: usize) {
        self.update_viewport_for_size(editor_height, 0, 0, false);
    }

    /// Update viewport scroll offsets (both vertical and horizontal) to keep cursor visible.
    pub fn update_viewport_for_size(
        &mut self,
        editor_height: usize,
        editor_width: usize,
        line_num_width: usize,
        word_wrap: bool,
    ) {
        if editor_height == 0 {
            return;
        }

        // Clamp cursor line to valid document range
        let max_line = self.rope.len_lines().saturating_sub(1);
        if self.viewport.cursor_pos.0 > max_line {
            self.viewport.cursor_pos.0 = max_line;
            self.clamp_cursor_to_line();
        }

        let content_width = editor_width.saturating_sub(line_num_width);

        if word_wrap {
            // Word wrap mode: no horizontal scrolling
            self.viewport.viewport_offset.1 = 0;
            self.update_viewport_vertical_word_wrap(editor_height, content_width);
        } else {
            // No word wrap: use horizontal scrolling

            // Vertical scrolling
            if self.viewport.cursor_pos.0 < self.viewport.viewport_offset.0 {
                self.viewport.viewport_offset.0 = self.viewport.cursor_pos.0;
            }
            if self.viewport.cursor_pos.0 >= self.viewport.viewport_offset.0 + editor_height {
                self.viewport.viewport_offset.0 = self
                    .viewport
                    .cursor_pos
                    .0
                    .saturating_sub(editor_height.saturating_sub(1));
            }
            let max_offset = max_line.saturating_sub(editor_height.saturating_sub(1));
            if self.viewport.viewport_offset.0 > max_offset {
                self.viewport.viewport_offset.0 = max_offset;
            }

            // Horizontal scrolling
            if content_width > 0 {
                let cursor_col = self.viewport.cursor_pos.1;
                if cursor_col < self.viewport.viewport_offset.1 {
                    self.viewport.viewport_offset.1 = cursor_col;
                }
                if cursor_col >= self.viewport.viewport_offset.1 + content_width {
                    self.viewport.viewport_offset.1 =
                        cursor_col.saturating_sub(content_width.saturating_sub(1));
                }
            }
        }
    }

    /// Vertical viewport adjustment for word-wrap mode.
    fn update_viewport_vertical_word_wrap(&mut self, editor_height: usize, content_width: usize) {
        if content_width == 0 {
            return;
        }

        let cursor_line = self.viewport.cursor_pos.0;

        if cursor_line < self.viewport.viewport_offset.0 {
            self.viewport.viewport_offset.0 = cursor_line;
        }

        loop {
            let mut screen_rows = 0;
            let mut found_cursor = false;
            for line_idx in self.viewport.viewport_offset.0..self.rope.len_lines() {
                let rows = self.wrapped_line_height(line_idx, content_width);
                if line_idx == cursor_line {
                    let cursor_sub_row = self.viewport.cursor_pos.1 / content_width;
                    let cursor_screen_y = screen_rows + cursor_sub_row;
                    if cursor_screen_y < editor_height {
                        found_cursor = true;
                    }
                    break;
                }
                screen_rows += rows;
                if screen_rows >= editor_height {
                    break;
                }
            }

            if found_cursor {
                break;
            }

            self.viewport.viewport_offset.0 += 1;
            if self.viewport.viewport_offset.0 > cursor_line {
                self.viewport.viewport_offset.0 = cursor_line;
                break;
            }
        }
    }

    /// Calculate how many screen rows a line occupies when wrapped.
    pub fn wrapped_line_height(&self, line_idx: usize, content_width: usize) -> usize {
        if content_width == 0 {
            return 1;
        }
        let width = line_display_width(&self.rope, line_idx);
        if width == 0 {
            1
        } else {
            width.div_ceil(content_width)
        }
    }

    pub fn handle_mouse_event(
        &mut self,
        event: crossterm::event::MouseEvent,
        terminal_height: usize,
        line_num_width: usize,
    ) {
        use crossterm::event::MouseEventKind;
        match event.kind {
            MouseEventKind::Down(_) => {
                let clicked_line = self.viewport.viewport_offset.0 + event.row as usize;
                // Subtract gutter width and add horizontal scroll offset
                let clicked_col = (event.column as usize).saturating_sub(line_num_width)
                    + self.viewport.viewport_offset.1;

                if clicked_line < self.rope.len_lines() {
                    self.viewport.cursor_pos.0 = clicked_line;
                    self.viewport.cursor_pos.1 = clicked_col;
                    self.clamp_cursor_to_line();
                }
            }
            MouseEventKind::Drag(_) => {}
            MouseEventKind::ScrollDown => {
                if self.viewport.viewport_offset.0
                    < self.rope.len_lines().saturating_sub(terminal_height)
                {
                    self.viewport.viewport_offset.0 += constants::SCROLL_SPEED;
                }
            }
            MouseEventKind::ScrollUp => {
                self.viewport.viewport_offset.0 = self
                    .viewport
                    .viewport_offset
                    .0
                    .saturating_sub(constants::SCROLL_SPEED);
            }
            _ => {}
        }
    }

    pub fn save_undo_state(&mut self) {
        self.undo_manager
            .save_state(&self.rope, self.viewport.cursor_pos);
    }

    pub fn toggle_hex_view(&mut self) {
        if self.hex_state.is_some() {
            // Restore text cursor from hex cursor byte offset
            if let Some(state) = &self.hex_state {
                let byte_offset = state.cursor;
                let text = self.rope.to_string();
                let char_idx = text[..byte_offset.min(text.len())].chars().count();
                let line = self
                    .rope
                    .char_to_line(char_idx.min(self.rope.len_chars().saturating_sub(1)));
                let line_start = self.rope.line_to_char(line);
                let col_chars = char_idx.saturating_sub(line_start);
                let display_col = self.char_idx_to_display_col(line, col_chars);
                self.viewport.cursor_pos = (line, display_col);
            }
            self.hex_state = None;
            return;
        }

        // Materialize rope content once and reuse
        let text = self.rope.to_string();

        // Get bytes from the live rope content
        let bytes = text.as_bytes().to_vec();

        // Convert text cursor position to byte offset
        let char_idx =
            self.line_col_to_char_idx(self.viewport.cursor_pos.0, self.viewport.cursor_pos.1);
        let byte_offset = text
            .char_indices()
            .nth(char_idx)
            .map(|(i, _)| i)
            .unwrap_or(text.len());

        let mut state = HexViewState::new(bytes);
        state.cursor = byte_offset.min(state.raw_bytes.len().saturating_sub(1));
        self.hex_state = Some(state);
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
            let cursor_char_idx =
                self.line_col_to_char_idx(self.viewport.cursor_pos.0, self.viewport.cursor_pos.1);

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

        if let Some(byte_pos) = text.find(search_term) {
            let mut new_text = text.clone();
            new_text.replace_range(byte_pos..byte_pos + search_term.len(), replace_term);

            self.rope = Rope::from_str(&new_text);
            self.modified = true;

            // Convert byte offset to char index before using with rope methods
            let char_pos = text[..byte_pos].chars().count();
            let line = self.rope.char_to_line(char_pos);
            let line_start = self.rope.line_to_char(line);
            let col = char_pos - line_start;
            self.viewport.cursor_pos = (line, col);
            self.clamp_cursor_to_line();

            self.invalidate_cache();
            self.highlighter.invalidate_cache_from_line(0);

            return 1;
        }

        0
    }

    /// Toggle mark (start/stop selection).
    pub fn toggle_mark(&mut self) {
        if self.mark_anchor.is_some() {
            self.mark_anchor = None;
        } else {
            self.mark_anchor = Some(self.viewport.cursor_pos);
        }
    }

    /// Get the selection range as (start_char_idx, end_char_idx) where start < end.
    pub fn get_selection_range(&self) -> Option<(usize, usize)> {
        let anchor = self.mark_anchor?;
        let cursor = self.viewport.cursor_pos;
        let anchor_idx = self.line_col_to_char_idx(anchor.0, anchor.1);
        let cursor_idx = self.line_col_to_char_idx(cursor.0, cursor.1);
        if anchor_idx <= cursor_idx {
            Some((anchor_idx, cursor_idx))
        } else {
            Some((cursor_idx, anchor_idx))
        }
    }

    /// Get the range of lines affected by the current selection, or just the cursor line.
    pub fn get_affected_lines(&self) -> (usize, usize) {
        if let Some(anchor) = self.mark_anchor {
            let start = anchor.0.min(self.viewport.cursor_pos.0);
            let end = anchor.0.max(self.viewport.cursor_pos.0);
            (start, end)
        } else {
            (self.viewport.cursor_pos.0, self.viewport.cursor_pos.0)
        }
    }

    /// Toggle line comment on selected lines (or current line).
    pub fn toggle_comment(&mut self) {
        let comment_str = match self.syntax_name.as_deref() {
            Some("Rust") | Some("C") | Some("C++") | Some("Go") | Some("JavaScript")
            | Some("TypeScript") | Some("Java") | Some("Swift") | Some("Kotlin") | Some("Zig") => {
                "// "
            }
            Some("Python")
            | Some("Ruby")
            | Some("Shell Script (Bash)")
            | Some("Perl")
            | Some("R")
            | Some("YAML")
            | Some("TOML") => "# ",
            Some("Lua") | Some("SQL") => "-- ",
            Some("HTML") | Some("XML") | Some("CSS") => return,
            _ => "// ",
        };

        let (start_line, end_line) = self.get_affected_lines();
        self.save_undo_state();

        // Check if all lines are already commented
        let all_commented = (start_line..=end_line).all(|line_idx| {
            if line_idx < self.rope.len_lines() {
                let rope_line = self.rope.line(line_idx);
                let line_text: String = rope_line.chars().collect();
                let trimmed = line_text.trim_start();
                trimmed.starts_with(comment_str.trim_end())
            } else {
                true
            }
        });

        if all_commented {
            for line_idx in (start_line..=end_line).rev() {
                if line_idx < self.rope.len_lines() {
                    let line_start = self.rope.line_to_char(line_idx);
                    let rope_line = self.rope.line(line_idx);
                    let line_text: String = rope_line.chars().collect();
                    if let Some(pos) = line_text.find(comment_str.trim_end()) {
                        let remove_len = if line_text[pos..].starts_with(comment_str) {
                            comment_str.len()
                        } else {
                            comment_str.trim_end().len()
                        };
                        self.rope
                            .remove(line_start + pos..line_start + pos + remove_len);
                    }
                }
            }
        } else {
            for line_idx in (start_line..=end_line).rev() {
                if line_idx < self.rope.len_lines() {
                    let line_start = self.rope.line_to_char(line_idx);
                    self.rope.insert(line_start, comment_str);
                }
            }
        }

        self.mark_anchor = None;
        self.modified = true;
        self.clamp_cursor_to_line();
        self.mark_document_changed(start_line);
    }

    /// Move cursor to the start of the next word.
    pub fn move_word_right(&mut self) {
        let line_idx = self.viewport.cursor_pos.0;
        let rope_line = self.rope.line(line_idx);
        let line_chars: Vec<char> = rope_line.chars().filter(|&c| c != '\n').collect();
        let display_col = self.viewport.cursor_pos.1;

        // Convert display column to char index
        let mut col = 0;
        let mut dcol = 0;
        for &ch in &line_chars {
            if dcol >= display_col {
                break;
            }
            col += 1;
            dcol += UnicodeWidthChar::width(ch).unwrap_or(0);
        }

        while col < line_chars.len() && !line_chars[col].is_whitespace() {
            col += 1;
        }
        while col < line_chars.len() && line_chars[col].is_whitespace() {
            col += 1;
        }

        if col >= line_chars.len() && line_idx < self.rope.len_lines().saturating_sub(1) {
            self.viewport.cursor_pos.0 += 1;
            self.viewport.cursor_pos.1 = 0;
        } else {
            // Convert char index back to display column
            let new_display_col: usize = line_chars[..col]
                .iter()
                .map(|c| UnicodeWidthChar::width(*c).unwrap_or(0))
                .sum();
            self.viewport.cursor_pos.1 = new_display_col;
        }
    }

    /// Move cursor to the start of the previous word.
    pub fn move_word_left(&mut self) {
        let line_idx = self.viewport.cursor_pos.0;
        let rope_line = self.rope.line(line_idx);
        let line_chars: Vec<char> = rope_line.chars().filter(|&c| c != '\n').collect();
        let display_col = self.viewport.cursor_pos.1;

        if display_col == 0 {
            if line_idx > 0 {
                self.viewport.cursor_pos.0 -= 1;
                self.viewport.cursor_pos.1 =
                    line_display_width(&self.rope, self.viewport.cursor_pos.0);
            }
            return;
        }

        // Convert display column to char index
        let mut col: usize = 0;
        let mut dcol: usize = 0;
        for &ch in &line_chars {
            if dcol >= display_col {
                break;
            }
            col += 1;
            dcol += UnicodeWidthChar::width(ch).unwrap_or(0);
        }

        while col > 0
            && line_chars
                .get(col.saturating_sub(1))
                .is_some_and(|c| c.is_whitespace())
        {
            col -= 1;
        }
        while col > 0
            && line_chars
                .get(col.saturating_sub(1))
                .is_some_and(|c| !c.is_whitespace())
        {
            col -= 1;
        }

        // Convert char index back to display column
        let new_display_col: usize = line_chars[..col]
            .iter()
            .map(|c| UnicodeWidthChar::width(*c).unwrap_or(0))
            .sum();
        self.viewport.cursor_pos.1 = new_display_col;
    }

    /// Jump to start of file.
    pub fn goto_start(&mut self) {
        self.viewport.cursor_pos = (0, 0);
    }

    /// Jump to end of file.
    pub fn goto_end(&mut self) {
        let last_line = self.rope.len_lines().saturating_sub(1);
        self.viewport.cursor_pos.0 = last_line;
        self.viewport.cursor_pos.1 = line_display_width(&self.rope, last_line);
    }

    /// Jump to matching bracket.
    pub fn match_bracket(&mut self) {
        let pos = self.line_col_to_char_idx(self.viewport.cursor_pos.0, self.viewport.cursor_pos.1);
        if pos >= self.rope.len_chars() {
            return;
        }

        let ch = self.rope.char(pos);
        let (target, forward) = match ch {
            '(' => (')', true),
            '[' => (']', true),
            '{' => ('}', true),
            ')' => ('(', false),
            ']' => ('[', false),
            '}' => ('{', false),
            _ => return,
        };

        let mut depth = 1i32;
        if forward {
            for i in (pos + 1)..self.rope.len_chars() {
                let c = self.rope.char(i);
                if c == ch {
                    depth += 1;
                }
                if c == target {
                    depth -= 1;
                }
                if depth == 0 {
                    let line = self.rope.char_to_line(i);
                    let line_start = self.rope.line_to_char(line);
                    let col_chars = i - line_start;
                    let display_col = self.char_idx_to_display_col(line, col_chars);
                    self.viewport.cursor_pos = (line, display_col);
                    return;
                }
            }
        } else {
            let mut i = pos;
            while i > 0 {
                i -= 1;
                let c = self.rope.char(i);
                if c == ch {
                    depth += 1;
                }
                if c == target {
                    depth -= 1;
                }
                if depth == 0 {
                    let line = self.rope.char_to_line(i);
                    let line_start = self.rope.line_to_char(line);
                    let col_chars = i - line_start;
                    let display_col = self.char_idx_to_display_col(line, col_chars);
                    self.viewport.cursor_pos = (line, display_col);
                    return;
                }
            }
        }
    }

    /// Word completion: find the partial word before cursor and complete it
    /// from the first matching word in the buffer.
    pub fn word_complete(&mut self) {
        let line_idx = self.viewport.cursor_pos.0;
        let display_col = self.viewport.cursor_pos.1;
        let rope_line = self.rope.line(line_idx);
        let line_chars: Vec<char> = rope_line.chars().filter(|&c| c != '\n').collect();

        // Convert display column to char index
        let mut char_idx = 0;
        let mut current_display_col = 0;
        for &ch in &line_chars {
            if current_display_col >= display_col {
                break;
            }
            char_idx += 1;
            current_display_col += UnicodeWidthChar::width(ch).unwrap_or(0);
        }

        let before_cursor: String = line_chars[..char_idx].iter().collect();

        // Find the word prefix (alphanumeric + underscore)
        let prefix: String = before_cursor
            .chars()
            .rev()
            .take_while(|c| c.is_alphanumeric() || *c == '_')
            .collect::<String>()
            .chars()
            .rev()
            .collect();

        if prefix.is_empty() {
            return;
        }

        // Scan all words in the document for matches using rope line iteration
        let mut found: Option<String> = None;
        'outer: for line_i in 0..self.rope.len_lines() {
            let rope_ln = self.rope.line(line_i);
            let line_str: String = rope_ln.chars().collect();
            for word in line_str.split(|c: char| !c.is_alphanumeric() && c != '_') {
                if word.starts_with(prefix.as_str()) && word != prefix && word.len() > prefix.len()
                {
                    found = Some(word.to_string());
                    break 'outer;
                }
            }
        }

        if let Some(ref completion) = found {
            let suffix = &completion[prefix.len()..];
            self.save_undo_state();
            let pos =
                self.line_col_to_char_idx(self.viewport.cursor_pos.0, self.viewport.cursor_pos.1);
            self.rope.insert(pos, suffix);
            self.mark_document_changed(self.viewport.cursor_pos.0);
            // Update cursor by display width of the suffix, not byte length
            self.viewport.cursor_pos.1 += UnicodeWidthStr::width(suffix);
            self.modified = true;
        }
    }

    pub fn invalidate_cache(&mut self) {
        // Reserved for future caching needs; currently a no-op used as
        // an extension point by mark_document_changed.
    }

    /// Invalidate highlighting and text caches from a given line onwards
    pub fn mark_document_changed(&mut self, from_line: usize) {
        self.highlighter.invalidate_cache_from_line(from_line);
        self.invalidate_cache();
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
        e.rope = Rope::from_str("    hello\n");
        e.viewport.cursor_pos = (0, 9);
        e.insert_newline(true);
        assert!(e.rope.to_string().starts_with("    hello\n    "));
    }

    #[test]
    fn test_no_auto_indent_when_disabled() {
        let mut e = Editor::new_for_test();
        e.rope = Rope::from_str("    hello\n");
        e.viewport.cursor_pos = (0, 9);
        e.insert_newline(false);
        assert_eq!(e.rope.to_string(), "    hello\n\n");
    }

    fn content(editor: &Editor) -> String {
        editor.rope.to_string()
    }

    #[test]
    fn test_toggle_mark() {
        let mut e = Editor::new_for_test();
        e.rope = Rope::from_str("hello\n");
        e.viewport.cursor_pos = (0, 2);
        e.toggle_mark();
        assert!(e.mark_anchor.is_some());
        assert_eq!(e.mark_anchor.unwrap(), (0, 2));
        e.toggle_mark();
        assert!(e.mark_anchor.is_none());
    }

    #[test]
    fn test_get_selection_range_none() {
        let e = Editor::new_for_test();
        assert!(e.get_selection_range().is_none());
    }

    #[test]
    fn test_get_selection_range_forward() {
        let mut e = Editor::new_for_test();
        e.rope = Rope::from_str("hello\n");
        e.mark_anchor = Some((0, 1));
        e.viewport.cursor_pos = (0, 4);
        let (start, end) = e.get_selection_range().unwrap();
        assert!(start < end);
    }

    #[test]
    fn test_get_selection_range_backward() {
        let mut e = Editor::new_for_test();
        e.rope = Rope::from_str("hello\n");
        e.mark_anchor = Some((0, 4));
        e.viewport.cursor_pos = (0, 1);
        let (start, end) = e.get_selection_range().unwrap();
        assert!(start < end);
    }

    #[test]
    fn test_mark_cleared_on_insert() {
        let mut e = Editor::new_for_test();
        e.rope = Rope::from_str("hello\n");
        e.mark_anchor = Some((0, 0));
        e.viewport.cursor_pos = (0, 0);
        e.insert_char('x');
        assert!(e.mark_anchor.is_none());
    }

    #[test]
    fn test_mark_cleared_on_delete() {
        let mut e = Editor::new_for_test();
        e.rope = Rope::from_str("hello\n");
        e.mark_anchor = Some((0, 0));
        e.viewport.cursor_pos = (0, 3);
        e.delete_char();
        assert!(e.mark_anchor.is_none());
    }

    #[test]
    fn test_mark_cleared_on_newline() {
        let mut e = Editor::new_for_test();
        e.rope = Rope::from_str("hello\n");
        e.mark_anchor = Some((0, 0));
        e.viewport.cursor_pos = (0, 3);
        e.insert_newline(false);
        assert!(e.mark_anchor.is_none());
    }

    // Comment tests stay here since toggle_comment is on Editor
    #[test]
    fn test_comment_adds_rust() {
        let mut e = Editor::new_for_test();
        e.rope = Rope::from_str("hello\n");
        e.syntax_name = Some("Rust".to_string());
        e.viewport.cursor_pos = (0, 0);
        e.toggle_comment();
        assert_eq!(content(&e), "// hello\n");
    }

    #[test]
    fn test_uncomment_removes_rust() {
        let mut e = Editor::new_for_test();
        e.rope = Rope::from_str("// hello\n");
        e.syntax_name = Some("Rust".to_string());
        e.viewport.cursor_pos = (0, 0);
        e.toggle_comment();
        assert_eq!(content(&e), "hello\n");
    }

    #[test]
    fn test_comment_toggle_python() {
        let mut e = Editor::new_for_test();
        e.rope = Rope::from_str("hello\n");
        e.syntax_name = Some("Python".to_string());
        e.viewport.cursor_pos = (0, 0);
        e.toggle_comment();
        assert_eq!(content(&e), "# hello\n");
    }

    #[test]
    fn test_word_complete_basic() {
        let mut e = Editor::new_for_test();
        e.rope = Rope::from_str("hello world help\n");
        // Place cursor after "hel" (col 3 on second word area won't work, let's type on a new line)
        e.rope = Rope::from_str("hello\nhel\n");
        e.viewport.cursor_pos = (1, 3); // after "hel"
        e.word_complete();
        assert_eq!(e.rope.to_string(), "hello\nhello\n");
    }

    #[test]
    fn test_word_complete_no_prefix() {
        let mut e = Editor::new_for_test();
        e.rope = Rope::from_str("hello\n\n");
        e.viewport.cursor_pos = (1, 0); // empty line, no prefix
        e.word_complete();
        assert_eq!(e.rope.to_string(), "hello\n\n"); // unchanged
    }

    #[test]
    fn test_word_complete_no_match() {
        let mut e = Editor::new_for_test();
        e.rope = Rope::from_str("hello\nxyz\n");
        e.viewport.cursor_pos = (1, 3); // after "xyz"
        e.word_complete();
        assert_eq!(e.rope.to_string(), "hello\nxyz\n"); // unchanged, no match
    }

    #[test]
    fn test_word_complete_with_underscore() {
        let mut e = Editor::new_for_test();
        e.rope = Rope::from_str("my_variable\nmy_\n");
        e.viewport.cursor_pos = (1, 3); // after "my_"
        e.word_complete();
        assert_eq!(e.rope.to_string(), "my_variable\nmy_variable\n");
    }
}

#[cfg(test)]
mod navigation_tests {
    use super::*;

    #[test]
    fn test_move_word_right_basic() {
        let mut e = Editor::new_for_test();
        e.rope = Rope::from_str("hello world foo\n");
        e.viewport.cursor_pos = (0, 0);
        e.move_word_right();
        assert_eq!(e.viewport.cursor_pos.1, 6);
    }

    #[test]
    fn test_move_word_right_from_middle() {
        let mut e = Editor::new_for_test();
        e.rope = Rope::from_str("hello world\n");
        e.viewport.cursor_pos = (0, 6);
        e.move_word_right();
        assert_eq!(e.viewport.cursor_pos, (1, 0));
    }

    #[test]
    fn test_move_word_left_basic() {
        let mut e = Editor::new_for_test();
        e.rope = Rope::from_str("hello world\n");
        e.viewport.cursor_pos = (0, 8);
        e.move_word_left();
        assert_eq!(e.viewport.cursor_pos.1, 6);
    }

    #[test]
    fn test_move_word_left_to_start() {
        let mut e = Editor::new_for_test();
        e.rope = Rope::from_str("hello world\n");
        e.viewport.cursor_pos = (0, 3);
        e.move_word_left();
        assert_eq!(e.viewport.cursor_pos.1, 0);
    }

    #[test]
    fn test_move_word_left_wraps() {
        let mut e = Editor::new_for_test();
        e.rope = Rope::from_str("hello\nworld\n");
        e.viewport.cursor_pos = (1, 0);
        e.move_word_left();
        assert_eq!(e.viewport.cursor_pos.0, 0);
        assert_eq!(e.viewport.cursor_pos.1, 5);
    }

    #[test]
    fn test_goto_start() {
        let mut e = Editor::new_for_test();
        e.rope = Rope::from_str("line1\nline2\nline3\n");
        e.viewport.cursor_pos = (2, 3);
        e.goto_start();
        assert_eq!(e.viewport.cursor_pos, (0, 0));
    }

    #[test]
    fn test_goto_end() {
        let mut e = Editor::new_for_test();
        e.rope = Rope::from_str("line1\nline2\n");
        e.viewport.cursor_pos = (0, 0);
        e.goto_end();
        let last_line = e.rope.len_lines().saturating_sub(1);
        assert_eq!(e.viewport.cursor_pos.0, last_line);
    }

    #[test]
    fn test_match_bracket_forward() {
        let mut e = Editor::new_for_test();
        e.rope = Rope::from_str("(hello)\n");
        e.viewport.cursor_pos = (0, 0);
        e.match_bracket();
        assert_eq!(e.viewport.cursor_pos.1, 6);
    }

    #[test]
    fn test_match_bracket_backward() {
        let mut e = Editor::new_for_test();
        e.rope = Rope::from_str("(hello)\n");
        e.viewport.cursor_pos = (0, 6);
        e.match_bracket();
        assert_eq!(e.viewport.cursor_pos.1, 0);
    }

    #[test]
    fn test_match_bracket_nested() {
        let mut e = Editor::new_for_test();
        e.rope = Rope::from_str("((a))\n");
        e.viewport.cursor_pos = (0, 0);
        e.match_bracket();
        assert_eq!(e.viewport.cursor_pos.1, 4);
    }

    #[test]
    fn test_match_bracket_no_match() {
        let mut e = Editor::new_for_test();
        e.rope = Rope::from_str("hello\n");
        e.viewport.cursor_pos = (0, 2);
        e.match_bracket();
        assert_eq!(e.viewport.cursor_pos, (0, 2));
    }

    // ── Horizontal scrolling tests ──

    #[test]
    fn test_horizontal_scroll_cursor_right_past_edge() {
        let mut e = Editor::new_for_test();
        e.rope = Rope::from_str(&"x".repeat(50));
        e.viewport.cursor_pos = (0, 30);
        e.update_viewport_for_size(10, 20, 0, false);
        assert_eq!(e.viewport.viewport_offset.1, 11);
    }

    #[test]
    fn test_horizontal_scroll_cursor_left_past_edge() {
        let mut e = Editor::new_for_test();
        e.rope = Rope::from_str(&"x".repeat(50));
        e.viewport.viewport_offset.1 = 20;
        e.viewport.cursor_pos = (0, 10);
        e.update_viewport_for_size(10, 20, 0, false);
        assert_eq!(e.viewport.viewport_offset.1, 10);
    }

    #[test]
    fn test_horizontal_scroll_no_scroll_needed() {
        let mut e = Editor::new_for_test();
        e.rope = Rope::from_str("short\n");
        e.viewport.cursor_pos = (0, 3);
        e.update_viewport_for_size(10, 20, 0, false);
        assert_eq!(e.viewport.viewport_offset.1, 0);
    }

    #[test]
    fn test_horizontal_scroll_with_line_numbers() {
        let mut e = Editor::new_for_test();
        e.rope = Rope::from_str(&"x".repeat(50));
        e.viewport.cursor_pos = (0, 18);
        e.update_viewport_for_size(10, 20, 3, false);
        assert_eq!(e.viewport.viewport_offset.1, 2);
    }

    #[test]
    fn test_horizontal_scroll_cursor_at_end_of_long_line() {
        let mut e = Editor::new_for_test();
        e.rope = Rope::from_str(&"a".repeat(200));
        e.viewport.cursor_pos = (0, 200);
        e.update_viewport_for_size(10, 80, 0, false);
        assert_eq!(e.viewport.viewport_offset.1, 121);
    }

    #[test]
    fn test_word_wrap_resets_horizontal_offset() {
        let mut e = Editor::new_for_test();
        e.rope = Rope::from_str(&"x".repeat(50));
        e.viewport.cursor_pos = (0, 30);
        e.update_viewport_for_size(10, 20, 0, false);
        assert!(e.viewport.viewport_offset.1 > 0);

        e.update_viewport_for_size(10, 20, 0, true);
        assert_eq!(e.viewport.viewport_offset.1, 0);
    }

    // ── Word wrap tests ──

    #[test]
    fn test_wrapped_line_height_short_line() {
        let e = Editor::new_for_test();
        assert_eq!(e.wrapped_line_height(0, 80), 1);
    }

    #[test]
    fn test_wrapped_line_height_exact_width() {
        let mut e = Editor::new_for_test();
        e.rope = Rope::from_str(&"a".repeat(20));
        assert_eq!(e.wrapped_line_height(0, 20), 1);
    }

    #[test]
    fn test_wrapped_line_height_needs_two_rows() {
        let mut e = Editor::new_for_test();
        e.rope = Rope::from_str(&"a".repeat(25));
        assert_eq!(e.wrapped_line_height(0, 20), 2);
    }

    #[test]
    fn test_wrapped_line_height_needs_three_rows() {
        let mut e = Editor::new_for_test();
        e.rope = Rope::from_str(&"a".repeat(50));
        assert_eq!(e.wrapped_line_height(0, 20), 3);
    }

    #[test]
    fn test_word_wrap_viewport_cursor_on_second_wrap_row() {
        let mut e = Editor::new_for_test();
        e.rope = Rope::from_str(&"a".repeat(30));
        e.viewport.cursor_pos = (0, 25);
        e.update_viewport_for_size(10, 20, 0, true);
        assert_eq!(e.viewport.viewport_offset.0, 0);
    }

    #[test]
    fn test_word_wrap_viewport_scrolls_down() {
        let mut e = Editor::new_for_test();
        let content: String = (0..5).map(|_| "a".repeat(30) + "\n").collect();
        e.rope = Rope::from_str(&content);
        e.viewport.cursor_pos = (4, 0);
        e.update_viewport_for_size(6, 20, 0, true);
        assert!(e.viewport.viewport_offset.0 >= 2);
    }
}
