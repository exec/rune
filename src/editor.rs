use anyhow::Result;
use ropey::Rope;
use std::cell::Cell;
use std::collections::VecDeque;
use std::fs;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};
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
    /// Word completion cycling state
    pub word_complete_candidates: Vec<String>,
    pub word_complete_index: usize,
    /// Cursor position when the completion was inserted (used to detect movement)
    word_complete_cursor: Option<(usize, usize)>,
    /// The prefix length used for the current completion
    word_complete_prefix_len: usize,
    /// Single-cell cache of the most recently queried line's display width.
    /// Hits the common case of repeated queries against the same line
    /// (e.g. right-arrow on a long line) without HashMap overhead. Cleared
    /// on any mutation routed through `invalidate_cache`.
    line_width_cache: Cell<Option<(usize, usize)>>,
    /// Monotonic counter bumped on every document mutation. Consumers (e.g.
    /// the render layer) can sample this to detect whether the document
    /// changed since the last frame.
    pub dirty_generation: u64,
    /// Globally unique, never-reused identifier for this buffer. Used by the
    /// render layer to key per-buffer caches without relying on memory
    /// addresses (which can be reused after a tab is closed).
    pub buffer_id: u64,
}

/// Source of unique `buffer_id`s for `Editor::new_buffer`.
static NEXT_BUFFER_ID: AtomicU64 = AtomicU64::new(0);

/// Tab stop width used to expand hard '\t' characters at the display layer.
/// Matches the stop Tab-key insertion aligns to by default
/// (`constants::DEFAULT_TAB_WIDTH`, see `TabManager::handle_tab_insertion`).
/// The rope keeps storing '\t'; only the column math and the renderer expand
/// it, and both must use this constant so they agree.
pub const TAB_WIDTH: usize = crate::constants::DEFAULT_TAB_WIDTH;

/// Display width of `c` when it starts at display column `current_col`.
/// A hard tab advances to the next multiple of `TAB_WIDTH` (so its width is
/// position-dependent); every other char uses its Unicode width. All display
/// column math in the editor and the renderer routes through this helper.
pub fn char_display_width(c: char, current_col: usize) -> usize {
    if c == '\t' {
        TAB_WIDTH - (current_col % TAB_WIDTH)
    } else {
        UnicodeWidthChar::width(c).unwrap_or(0)
    }
}

/// Display width of `s` when it starts at display column `start_col`,
/// expanding hard tabs to `TAB_WIDTH` stops.
pub fn str_display_width(s: &str, start_col: usize) -> usize {
    let mut col = start_col;
    for c in s.chars() {
        col += char_display_width(c, col);
    }
    col - start_col
}

/// Get the display width of a line, handling the case where the line spans chunk boundaries.
pub fn line_display_width(rope: &Rope, line: usize) -> usize {
    let rope_line = rope.line(line);
    if let Some(s) = rope_line.as_str() {
        let s = s.trim_end_matches('\n');
        if !s.contains('\t') {
            return s.width();
        }
    }
    let mut col = 0;
    for c in rope_line.chars().filter(|&c| c != '\n') {
        col += char_display_width(c, col);
    }
    col
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
            word_complete_candidates: Vec::new(),
            word_complete_index: 0,
            word_complete_cursor: None,
            word_complete_prefix_len: 0,
            line_width_cache: Cell::new(None),
            dirty_generation: 0,
            buffer_id: NEXT_BUFFER_ID.fetch_add(1, Ordering::Relaxed),
        }
    }

    /// Create an Editor for unit tests (same as new_buffer).
    pub fn new_for_test() -> Self {
        Self::new_buffer()
    }

    pub fn load_file(&mut self, path: PathBuf) -> Result<()> {
        // `fs::read_to_string` + `Rope::from_str` is actually faster than
        // `Rope::from_reader` on warm SSD across the 1MB–50MB range tested
        // (measured via `load_file` benchmarks). A streaming path would be
        // worth reconsidering for cold-disk / network-mount scenarios.
        let content = fs::read_to_string(&path).map_err(|e| {
            if e.kind() == std::io::ErrorKind::InvalidData {
                anyhow::anyhow!("{}: binary or non-UTF-8 file", path.display())
            } else {
                anyhow::Error::from(e)
            }
        })?;
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
        // Advance cursor directly. Avoids `move_cursor_right`'s cache query
        // on the hot typing path; the cache would be cold anyway because
        // `mark_document_changed` just cleared it.
        if c == '\n' {
            self.viewport.cursor_pos.0 += 1;
            self.viewport.cursor_pos.1 = 0;
        } else {
            self.viewport.cursor_pos.1 += char_display_width(c, self.viewport.cursor_pos.1);
        }
        self.modified = true;
    }

    pub fn delete_char(&mut self) {
        self.mark_anchor = None;
        if self.viewport.cursor_pos.1 > 0 {
            let pos =
                self.line_col_to_char_idx(self.viewport.cursor_pos.0, self.viewport.cursor_pos.1);
            if pos > 0 {
                self.save_undo_state();
                // Land the cursor on the deleted char's start column. Its
                // display width is position-dependent (a wide CJK char
                // occupies 2 columns, a tab up to TAB_WIDTH), so compute the
                // column from the char index before mutating the rope.
                let line_start = self.rope.line_to_char(self.viewport.cursor_pos.0);
                let new_col =
                    self.char_idx_to_display_col(self.viewport.cursor_pos.0, pos - 1 - line_start);
                self.rope.remove(pos - 1..pos);
                self.mark_document_changed(self.viewport.cursor_pos.0);
                self.viewport.cursor_pos.1 = new_col;
                self.modified = true;
            }
        } else if self.viewport.cursor_pos.0 > 0 {
            let pos = self.line_col_to_char_idx(self.viewport.cursor_pos.0, 0);
            if pos > 0 {
                self.save_undo_state();

                let junction_col = self.line_display_width_cached(self.viewport.cursor_pos.0 - 1);

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
        // The indent starts at column 0; it may contain hard tabs.
        self.viewport.cursor_pos.1 = str_display_width(&indent, 0);
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
            // Step over a whole character (wide chars span 2 display
            // columns): convert the display column to a char index, step the
            // index back past any zero-width chars, and convert back.
            let line = self.viewport.cursor_pos.0;
            let line_start = self.rope.line_to_char(line);
            let mut char_idx =
                self.line_col_to_char_idx(line, self.viewport.cursor_pos.1) - line_start;
            let mut new_col = self.viewport.cursor_pos.1;
            while new_col >= self.viewport.cursor_pos.1 && char_idx > 0 {
                char_idx -= 1;
                new_col = self.char_idx_to_display_col(line, char_idx);
            }
            self.viewport.cursor_pos.1 = new_col;
        } else if self.viewport.cursor_pos.0 > 0 {
            self.viewport.cursor_pos.0 -= 1;
            self.viewport.cursor_pos.1 = self.line_display_width_cached(self.viewport.cursor_pos.0);
        }
    }

    pub fn move_cursor_right(&mut self) {
        let line = self.viewport.cursor_pos.0;
        let line_len = self.line_display_width_cached(line);
        if self.viewport.cursor_pos.1 < line_len {
            // Step over a whole character (wide chars span 2 display
            // columns): convert the display column to a char index, advance
            // the index past any zero-width chars, and convert back.
            let line_start = self.rope.line_to_char(line);
            let mut char_idx =
                self.line_col_to_char_idx(line, self.viewport.cursor_pos.1) - line_start;
            let mut new_col = self.viewport.cursor_pos.1;
            while new_col <= self.viewport.cursor_pos.1 {
                char_idx += 1;
                new_col = self.char_idx_to_display_col(line, char_idx);
            }
            self.viewport.cursor_pos.1 = new_col;
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
        let line_len = self.line_display_width_cached(self.viewport.cursor_pos.0);
        self.viewport.cursor_pos.1 = self.viewport.cursor_pos.1.min(line_len);
    }

    /// Convert a char offset (number of chars from line start) to a display column,
    /// accounting for character widths (e.g. wide CJK characters, hard tabs).
    pub fn char_idx_to_display_col(&self, line: usize, char_offset: usize) -> usize {
        let rope_line = self.rope.line(line);
        let mut display_col = 0;
        for (i, ch) in rope_line.chars().enumerate() {
            if i >= char_offset || ch == '\n' {
                break;
            }
            display_col += char_display_width(ch, display_col);
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
            let w = char_display_width(ch, display_col);
            if display_col + w > col {
                // `col` falls inside this char's span (mid-tab, mid-CJK):
                // snap to the char itself rather than past it.
                break;
            }
            char_idx = i + 1;
            display_col += w;
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
                    // Clamp to the line's last rendered row: a cursor at the
                    // end of an exactly-full row would otherwise compute one
                    // sub-row past the rows the renderer produces.
                    let cursor_sub_row =
                        (self.viewport.cursor_pos.1 / content_width).min(rows.saturating_sub(1));
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
        let width = self.line_display_width_cached(line_idx);
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

        // `search_start_pos` is set once in `TabManager::start_find` and must
        // not be clobbered here — otherwise cancel/Esc restores the cursor
        // to the last match rather than the pre-find position.
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
                    // Matches store CHAR offsets; the cursor stores DISPLAY
                    // columns. The viewport scrolls the cursor into view on
                    // the next frame via `update_viewport_for_size`.
                    let display_col = self.char_idx_to_display_col(line, col);
                    self.viewport.cursor_pos = (line, display_col);
                    self.clamp_cursor_to_line();
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
            let display_col = self.char_idx_to_display_col(line, col);
            self.viewport.cursor_pos = (line, display_col);
            self.clamp_cursor_to_line();
            true
        } else {
            false
        }
    }

    pub fn find_previous_match(&mut self) -> bool {
        if let Some((line, col)) = self.search.navigate_match(false) {
            let display_col = self.char_idx_to_display_col(line, col);
            self.viewport.cursor_pos = (line, display_col);
            self.clamp_cursor_to_line();
            true
        } else {
            false
        }
    }

    pub fn cancel_search(&mut self) {
        let start_pos = self.search.cancel_search();
        self.viewport.cursor_pos = start_pos;
    }

    /// Replace every match of `search_term` with `replace_term`, honoring
    /// the active search modes (`use_regex`, `case_sensitive`) just like
    /// find does. The replacement text is inserted literally — no `$1`
    /// capture-group expansion. Returns the number of replacements made.
    pub fn perform_replace(&mut self, search_term: &str, replace_term: &str) -> usize {
        if search_term.is_empty() {
            return 0;
        }

        self.save_undo_state();

        // Locate matches with the same engine find uses, so replace honors
        // regex and case-insensitive modes.
        self.search.search_buffer = search_term.to_string();
        let spans = self.search.find_all_match_spans(&self.rope);

        // Convert (line, char_col, char_len) to absolute char ranges,
        // dropping overlapping matches (the literal scanner can report
        // overlaps) so reverse application stays sound.
        let first_line = spans.first().map(|&(line, _, _)| line).unwrap_or(0);
        let mut ranges: Vec<(usize, usize)> = Vec::with_capacity(spans.len());
        let mut prev_end = 0;
        for &(line, col, len) in &spans {
            let start = self.rope.line_to_char(line) + col;
            if start >= prev_end {
                ranges.push((start, start + len));
                prev_end = start + len;
            }
        }

        let replacements = ranges.len();
        if replacements > 0 {
            // Apply in reverse so earlier char indices stay valid.
            for &(start, end) in ranges.iter().rev() {
                self.rope.remove(start..end);
                self.rope.insert(start, replace_term);
            }
            self.modified = true;
            // Invalidate caches before clamping — the clamp queries the
            // (now stale) line width cache otherwise.
            self.mark_document_changed(first_line);
            self.clamp_cursor_to_line();
        }

        replacements
    }

    /// Find the next match of `search_term` at or after the interactive
    /// replace session's resume position (`search.replace_resume_char`),
    /// honoring the active search modes. Returns
    /// `(line, char_col, abs_char_pos, char_len)`.
    fn next_interactive_match(&mut self, search_term: &str) -> Option<(usize, usize, usize, usize)> {
        // Locate matches with the same engine find uses, so replace honors
        // regex and case-insensitive modes.
        self.search.search_buffer = search_term.to_string();
        let resume = self.search.replace_resume_char;
        self.search
            .find_all_match_spans(&self.rope)
            .into_iter()
            .find_map(|(line, col, len)| {
                let start = self.rope.line_to_char(line) + col;
                (start >= resume).then_some((line, col, start, len))
            })
    }

    /// Replace the next match of `search_term` in the interactive replace
    /// session, honoring the active search modes (`use_regex`,
    /// `case_sensitive`) just like find does. The replacement text is
    /// inserted literally — no `$1` capture-group expansion. Returns 1 if a
    /// replacement was made, 0 otherwise.
    pub fn perform_replace_interactive(&mut self, search_term: &str, replace_term: &str) -> usize {
        if search_term.is_empty() {
            return 0;
        }

        if let Some((line, col, char_pos, char_len)) = self.next_interactive_match(search_term) {
            self.save_undo_state();
            self.rope.remove(char_pos..char_pos + char_len);
            self.rope.insert(char_pos, replace_term);
            self.modified = true;

            // Resume just past the inserted text so a replacement that
            // still contains the pattern isn't re-matched (`a` -> `aa`
            // must terminate). Zero-width matches advance one extra char
            // so the session always makes progress.
            self.search.replace_resume_char =
                char_pos + replace_term.chars().count() + usize::from(char_len == 0);

            // Invalidate caches before clamping — the clamp queries the
            // (now stale) line width cache otherwise.
            self.mark_document_changed(line);

            // `col` is a CHAR offset; the cursor stores DISPLAY columns.
            let display_col = self.char_idx_to_display_col(line, col);
            self.viewport.cursor_pos = (line, display_col);
            self.clamp_cursor_to_line();

            return 1;
        }

        0
    }

    /// Skip the next match of the interactive replace session: advance the
    /// resume position past it and move the cursor there, leaving the text
    /// untouched. Returns true if there was a match to skip.
    pub fn skip_next_match(&mut self, search_term: &str) -> bool {
        if search_term.is_empty() {
            return false;
        }

        if let Some((line, col, char_pos, char_len)) = self.next_interactive_match(search_term) {
            // Always advance at least one char so zero-width matches can't
            // stall the session.
            self.search.replace_resume_char = char_pos + char_len.max(1);
            let display_col = self.char_idx_to_display_col(line, col);
            self.viewport.cursor_pos = (line, display_col);
            self.clamp_cursor_to_line();
            true
        } else {
            false
        }
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

        // Walk chars while tracking display col; advance through
        // non-whitespace then whitespace in a single pass.
        let mut col = 0;
        let mut dcol = 0;
        while col < line_chars.len() && dcol < display_col {
            dcol += char_display_width(line_chars[col], dcol);
            col += 1;
        }

        while col < line_chars.len() && !line_chars[col].is_whitespace() {
            dcol += char_display_width(line_chars[col], dcol);
            col += 1;
        }
        while col < line_chars.len() && line_chars[col].is_whitespace() {
            dcol += char_display_width(line_chars[col], dcol);
            col += 1;
        }

        if col >= line_chars.len() && line_idx < self.rope.len_lines().saturating_sub(1) {
            self.viewport.cursor_pos.0 += 1;
            self.viewport.cursor_pos.1 = 0;
        } else {
            self.viewport.cursor_pos.1 = dcol;
        }
    }

    /// Move cursor to the start of the previous word.
    pub fn move_word_left(&mut self) {
        let line_idx = self.viewport.cursor_pos.0;
        let rope_line = self.rope.line(line_idx);
        let display_col = self.viewport.cursor_pos.1;

        if display_col == 0 {
            if line_idx > 0 {
                self.viewport.cursor_pos.0 -= 1;
                self.viewport.cursor_pos.1 =
                    self.line_display_width_cached(self.viewport.cursor_pos.0);
            }
            return;
        }

        // Build parallel char + prefix-display-col arrays in a single pass,
        // so the backward scan can look up the target display col directly.
        let mut line_chars: Vec<char> = Vec::new();
        let mut prefix_dcol: Vec<usize> = vec![0];
        let mut running = 0usize;
        for ch in rope_line.chars().filter(|&c| c != '\n') {
            running += char_display_width(ch, running);
            line_chars.push(ch);
            prefix_dcol.push(running);
        }

        // Find char index at or just past display_col.
        let mut col: usize = 0;
        while col < line_chars.len() && prefix_dcol[col] < display_col {
            col += 1;
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

        self.viewport.cursor_pos.1 = prefix_dcol[col];
    }

    /// Jump to start of file.
    pub fn goto_start(&mut self) {
        self.viewport.cursor_pos = (0, 0);
    }

    /// Jump to end of file.
    pub fn goto_end(&mut self) {
        let last_line = self.rope.len_lines().saturating_sub(1);
        self.viewport.cursor_pos.0 = last_line;
        self.viewport.cursor_pos.1 = self.line_display_width_cached(last_line);
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

    /// Reset word completion cycling state. Call this when the cursor moves
    /// or any non-completion key is pressed.
    pub fn reset_word_complete(&mut self) {
        self.word_complete_candidates.clear();
        self.word_complete_index = 0;
        self.word_complete_cursor = None;
        self.word_complete_prefix_len = 0;
    }

    /// Word completion: find the partial word before cursor and complete it
    /// from matching words in the buffer. Subsequent calls cycle through
    /// alternatives.
    pub fn word_complete(&mut self) {
        // Check if we're continuing a previous completion cycle
        let is_cycling = self.word_complete_cursor == Some(self.viewport.cursor_pos)
            && !self.word_complete_candidates.is_empty();

        if is_cycling {
            // Undo the previous completion suffix
            let prev_candidate = &self.word_complete_candidates[self.word_complete_index];
            let prev_suffix_width =
                UnicodeWidthStr::width(&prev_candidate[self.word_complete_prefix_len..]);
            let suffix_start_col = self.viewport.cursor_pos.1 - prev_suffix_width;
            let start_pos = self.line_col_to_char_idx(self.viewport.cursor_pos.0, suffix_start_col);
            let end_pos =
                self.line_col_to_char_idx(self.viewport.cursor_pos.0, self.viewport.cursor_pos.1);
            self.rope.remove(start_pos..end_pos);
            self.mark_document_changed(self.viewport.cursor_pos.0);
            self.viewport.cursor_pos.1 = suffix_start_col;

            // Advance to next candidate
            self.word_complete_index =
                (self.word_complete_index + 1) % self.word_complete_candidates.len();

            // Insert the new candidate's suffix
            let candidate = self.word_complete_candidates[self.word_complete_index].clone();
            let suffix = &candidate[self.word_complete_prefix_len..];
            let pos =
                self.line_col_to_char_idx(self.viewport.cursor_pos.0, self.viewport.cursor_pos.1);
            self.rope.insert(pos, suffix);
            self.mark_document_changed(self.viewport.cursor_pos.0);
            self.viewport.cursor_pos.1 += UnicodeWidthStr::width(suffix);
            self.modified = true;
            self.word_complete_cursor = Some(self.viewport.cursor_pos);
            return;
        }

        // Fresh completion: scan for prefix and candidates
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
            current_display_col += char_display_width(ch, current_display_col);
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

        // Scan all words in the document for matches, collecting unique
        // candidates. Iterate rope chars directly to avoid per-line
        // `String` allocation; only clone the buffer for words that match
        // the prefix.
        let mut candidates: Vec<String> = Vec::new();
        let mut seen = std::collections::HashSet::new();
        let mut word_buf = String::new();
        let prefix_str = prefix.as_str();
        let prefix_len = prefix.len();
        let flush = |buf: &mut String,
                     seen: &mut std::collections::HashSet<String>,
                     candidates: &mut Vec<String>| {
            if buf.len() > prefix_len
                && buf.starts_with(prefix_str)
                && buf.as_str() != prefix_str
                && !seen.contains(buf.as_str())
            {
                seen.insert(buf.clone());
                candidates.push(buf.clone());
            }
            buf.clear();
        };
        for ch in self.rope.chars() {
            if ch.is_alphanumeric() || ch == '_' {
                word_buf.push(ch);
            } else if !word_buf.is_empty() {
                flush(&mut word_buf, &mut seen, &mut candidates);
            }
        }
        if !word_buf.is_empty() {
            flush(&mut word_buf, &mut seen, &mut candidates);
        }

        if candidates.is_empty() {
            return;
        }

        // Insert first candidate's suffix
        let suffix = &candidates[0][prefix.len()..];
        self.save_undo_state();
        let pos = self.line_col_to_char_idx(self.viewport.cursor_pos.0, self.viewport.cursor_pos.1);
        self.rope.insert(pos, suffix);
        self.mark_document_changed(self.viewport.cursor_pos.0);
        self.viewport.cursor_pos.1 += UnicodeWidthStr::width(suffix);
        self.modified = true;

        // Store cycling state
        self.word_complete_candidates = candidates;
        self.word_complete_index = 0;
        self.word_complete_prefix_len = prefix.len();
        self.word_complete_cursor = Some(self.viewport.cursor_pos);
    }

    pub fn invalidate_cache(&mut self) {
        self.line_width_cache.set(None);
    }

    /// Display width of a line, with single-cell memoization. Hits when the
    /// same line is queried twice in a row (common in cursor movement on a
    /// single line); a miss costs only a comparison plus the underlying
    /// computation. Cleared via `invalidate_cache` on any text mutation.
    pub fn line_display_width_cached(&self, line: usize) -> usize {
        if let Some((cached_line, cached_width)) = self.line_width_cache.get() {
            if cached_line == line {
                return cached_width;
            }
        }
        let w = line_display_width(&self.rope, line);
        self.line_width_cache.set(Some((line, w)));
        w
    }

    /// Invalidate highlighting and text caches from a given line onwards
    pub fn mark_document_changed(&mut self, from_line: usize) {
        self.dirty_generation = self.dirty_generation.wrapping_add(1);
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

    #[test]
    fn test_word_complete_cycling() {
        let mut e = Editor::new_for_test();
        e.rope = Rope::from_str("println private protect\npr\n");
        e.viewport.cursor_pos = (1, 2); // after "pr"

        // First completion: "println"
        e.word_complete();
        assert_eq!(e.rope.to_string(), "println private protect\nprintln\n");

        // Second press: cycle to "private"
        e.word_complete();
        assert_eq!(e.rope.to_string(), "println private protect\nprivate\n");

        // Third press: cycle to "protect"
        e.word_complete();
        assert_eq!(e.rope.to_string(), "println private protect\nprotect\n");

        // Fourth press: wrap around to "println"
        e.word_complete();
        assert_eq!(e.rope.to_string(), "println private protect\nprintln\n");
    }

    #[test]
    fn test_word_complete_reset_on_cursor_move() {
        let mut e = Editor::new_for_test();
        e.rope = Rope::from_str("println private\npr\n");
        e.viewport.cursor_pos = (1, 2); // after "pr"

        // First completion: "println"
        e.word_complete();
        assert_eq!(e.rope.to_string(), "println private\nprintln\n");

        // Reset (simulates any other key press or cursor move)
        e.reset_word_complete();

        // Move cursor to simulate typing or movement, then try again
        // After reset, word_complete starts fresh from the current buffer
        e.viewport.cursor_pos = (1, 7); // cursor at end of "println"
        e.word_complete();
        // Now scanning with prefix "println" - no other match, so unchanged
        assert_eq!(e.rope.to_string(), "println private\nprintln\n");
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

#[cfg(test)]
mod wide_char_tests {
    use super::*;

    #[test]
    fn test_insert_wide_char_advances_by_display_width() {
        let mut e = Editor::new_for_test();
        e.insert_char('あ');
        assert_eq!(e.viewport.cursor_pos, (0, 2));
        e.insert_char('x');
        assert_eq!(e.rope.to_string(), "あx");
        assert_eq!(e.viewport.cursor_pos, (0, 3));
    }

    #[test]
    fn test_backspace_after_wide_char() {
        let mut e = Editor::new_for_test();
        e.rope = Rope::from_str("あ");
        e.viewport.cursor_pos = (0, 2);
        e.delete_char();
        assert_eq!(e.rope.to_string(), "");
        assert_eq!(e.viewport.cursor_pos, (0, 0));
    }

    #[test]
    fn test_arrow_keys_step_over_wide_chars() {
        let mut e = Editor::new_for_test();
        e.rope = Rope::from_str("あx");
        e.viewport.cursor_pos = (0, 0);
        // Right over "あx": col goes 0 -> 2 -> 3, never landing mid-glyph.
        e.move_cursor_right();
        assert_eq!(e.viewport.cursor_pos, (0, 2));
        e.move_cursor_right();
        assert_eq!(e.viewport.cursor_pos, (0, 3));
        // And back: 3 -> 2 -> 0.
        e.move_cursor_left();
        assert_eq!(e.viewport.cursor_pos, (0, 2));
        e.move_cursor_left();
        assert_eq!(e.viewport.cursor_pos, (0, 0));
    }
}

#[cfg(test)]
mod hard_tab_tests {
    use super::*;

    #[test]
    fn test_char_idx_to_display_col_leading_tab() {
        let mut e = Editor::new_for_test();
        e.rope = Rope::from_str("\tx\n");
        // 'x' (char 1) sits at the first tab stop.
        assert_eq!(e.char_idx_to_display_col(0, 0), 0);
        assert_eq!(e.char_idx_to_display_col(0, 1), TAB_WIDTH);
    }

    #[test]
    fn test_char_idx_to_display_col_tab_advances_to_next_stop() {
        let mut e = Editor::new_for_test();
        e.rope = Rope::from_str("ab\tx\n");
        // The tab starts at col 2 and advances to the next TAB_WIDTH stop,
        // not by a fixed width.
        assert_eq!(e.char_idx_to_display_col(0, 2), 2);
        assert_eq!(e.char_idx_to_display_col(0, 3), TAB_WIDTH);
    }

    #[test]
    fn test_display_col_round_trip_snaps_mid_tab_to_tab_char() {
        let mut e = Editor::new_for_test();
        e.rope = Rope::from_str("\tx\n");
        // Clicking inside the tab's span resolves to the tab char itself...
        for col in 0..TAB_WIDTH {
            assert_eq!(e.line_col_to_char_idx(0, col), 0, "col {col}");
        }
        // ...and the tab stop boundary resolves to the char after it.
        assert_eq!(e.line_col_to_char_idx(0, TAB_WIDTH), 1);
        // Round trip from the snapped char index lands on the tab's column.
        let snapped = e.line_col_to_char_idx(0, TAB_WIDTH / 2);
        assert_eq!(e.char_idx_to_display_col(0, snapped), 0);
    }

    #[test]
    fn test_cursor_movement_steps_full_tab_width() {
        let mut e = Editor::new_for_test();
        e.rope = Rope::from_str("\tx\n");
        e.viewport.cursor_pos = (0, 0);
        e.move_cursor_right();
        assert_eq!(e.viewport.cursor_pos, (0, TAB_WIDTH));
        e.move_cursor_right();
        assert_eq!(e.viewport.cursor_pos, (0, TAB_WIDTH + 1));
        e.move_cursor_left();
        assert_eq!(e.viewport.cursor_pos, (0, TAB_WIDTH));
        e.move_cursor_left();
        assert_eq!(e.viewport.cursor_pos, (0, 0));
    }

    #[test]
    fn test_line_display_width_mixed_tabs_and_cjk() {
        let mut e = Editor::new_for_test();
        // "あ" occupies cols 0-1, so the tab starts at col 2 and advances to
        // the stop at TAB_WIDTH; 'x' adds one more column.
        e.rope = Rope::from_str("あ\tx\n");
        assert_eq!(e.line_display_width_cached(0), TAB_WIDTH + 1);
        // Two leading tabs: two full stops.
        e.rope = Rope::from_str("\t\t\n");
        e.invalidate_cache();
        assert_eq!(e.line_display_width_cached(0), 2 * TAB_WIDTH);
    }

    #[test]
    fn test_insert_tab_char_advances_to_next_stop() {
        let mut e = Editor::new_for_test();
        e.rope = Rope::from_str("ab\n");
        e.viewport.cursor_pos = (0, 2);
        e.insert_char('\t');
        assert_eq!(e.rope.to_string(), "ab\t\n");
        assert_eq!(e.viewport.cursor_pos, (0, TAB_WIDTH));
    }

    #[test]
    fn test_backspace_over_tab_returns_to_tab_start() {
        let mut e = Editor::new_for_test();
        e.rope = Rope::from_str("ab\t\n");
        e.viewport.cursor_pos = (0, TAB_WIDTH);
        e.delete_char();
        assert_eq!(e.rope.to_string(), "ab\n");
        assert_eq!(e.viewport.cursor_pos, (0, 2));
    }

    #[test]
    fn test_auto_indent_with_tab_indent_uses_tab_width() {
        let mut e = Editor::new_for_test();
        e.rope = Rope::from_str("\thello\n");
        e.viewport.cursor_pos = (0, TAB_WIDTH + 5);
        e.insert_newline(true);
        assert!(e.rope.to_string().starts_with("\thello\n\t"));
        assert_eq!(e.viewport.cursor_pos, (1, TAB_WIDTH));
    }

    #[test]
    fn test_str_display_width_position_dependent() {
        // Starting at col 0 the tab spans the full stop; starting at col 3
        // it only spans one column.
        assert_eq!(str_display_width("\t", 0), TAB_WIDTH);
        assert_eq!(str_display_width("\t", TAB_WIDTH - 1), 1);
        assert_eq!(str_display_width("a\tb", 0), TAB_WIDTH + 1);
    }
}

#[cfg(test)]
mod find_replace_tests {
    use super::*;

    #[test]
    fn test_find_jump_uses_display_columns() {
        let mut e = Editor::new_for_test();
        e.rope = Rope::from_str("ああ target\n");
        e.viewport.cursor_pos = (0, 0);
        assert!(e.perform_find("target"));
        // "target" starts at CHAR offset 3 but DISPLAY column 5 (2+2+1).
        assert_eq!(e.viewport.cursor_pos, (0, 5));
    }

    #[test]
    fn test_replace_all_honors_regex_mode() {
        let mut e = Editor::new_for_test();
        e.rope = Rope::from_str("a1 b22 c333\n");
        e.search.use_regex = true;
        let n = e.perform_replace(r"\d+", "N");
        assert_eq!(n, 3);
        assert_eq!(e.rope.to_string(), "aN bN cN\n");
    }

    #[test]
    fn test_replace_all_honors_case_insensitive_mode() {
        let mut e = Editor::new_for_test();
        e.rope = Rope::from_str("Hello hello HELLO\n");
        e.search.case_sensitive = false;
        let n = e.perform_replace("hello", "X");
        assert_eq!(n, 3);
        assert_eq!(e.rope.to_string(), "X X X\n");
    }

    #[test]
    fn test_replace_all_case_sensitive_mode() {
        let mut e = Editor::new_for_test();
        e.rope = Rope::from_str("Hello hello HELLO\n");
        e.search.case_sensitive = true;
        let n = e.perform_replace("hello", "X");
        assert_eq!(n, 1);
        assert_eq!(e.rope.to_string(), "Hello X HELLO\n");
    }

    #[test]
    fn test_replace_interactive_honors_regex_mode() {
        let mut e = Editor::new_for_test();
        e.rope = Rope::from_str("a1 b22\n");
        e.search.use_regex = true;
        let n = e.perform_replace_interactive(r"\d+", "N");
        assert_eq!(n, 1);
        assert_eq!(e.rope.to_string(), "aN b22\n");
    }

    #[test]
    fn test_replace_interactive_honors_case_insensitive_mode() {
        let mut e = Editor::new_for_test();
        e.rope = Rope::from_str("HELLO hello\n");
        e.search.case_sensitive = false;
        let n = e.perform_replace_interactive("hello", "x");
        assert_eq!(n, 1);
        assert_eq!(e.rope.to_string(), "x hello\n");
    }

    #[test]
    fn test_replace_interactive_cursor_at_display_column() {
        let mut e = Editor::new_for_test();
        e.rope = Rope::from_str("ああ target\n");
        let n = e.perform_replace_interactive("target", "T");
        assert_eq!(n, 1);
        // Replacement starts at CHAR offset 3 but DISPLAY column 5.
        assert_eq!(e.viewport.cursor_pos, (0, 5));
    }

    #[test]
    fn test_replace_interactive_advances_past_each_match() {
        let mut e = Editor::new_for_test();
        e.rope = Rope::from_str("aaa\n");
        // Replacement contains the pattern; each Y must still make forward
        // progress and the session must terminate.
        assert_eq!(e.perform_replace_interactive("a", "aa"), 1);
        assert_eq!(e.perform_replace_interactive("a", "aa"), 1);
        assert_eq!(e.perform_replace_interactive("a", "aa"), 1);
        assert_eq!(e.rope.to_string(), "aaaaaa\n");
        assert_eq!(e.perform_replace_interactive("a", "aa"), 0);
        assert_eq!(e.rope.to_string(), "aaaaaa\n");
    }

    #[test]
    fn test_replace_interactive_skip_keeps_match_and_continues() {
        let mut e = Editor::new_for_test();
        e.rope = Rope::from_str("x x x\n");
        // Y, N, Y: replace first, skip second, replace third.
        assert_eq!(e.perform_replace_interactive("x", "y"), 1);
        assert!(e.skip_next_match("x"));
        // Skip moved the cursor onto the skipped match.
        assert_eq!(e.viewport.cursor_pos, (0, 2));
        assert_eq!(e.perform_replace_interactive("x", "y"), 1);
        assert_eq!(e.rope.to_string(), "y x y\n");
        assert!(!e.skip_next_match("x"));
    }

    #[test]
    fn test_replace_interactive_resume_resets_per_session() {
        let mut e = Editor::new_for_test();
        e.rope = Rope::from_str("b b\n");
        assert_eq!(e.perform_replace_interactive("b", "c"), 1);
        assert_eq!(e.perform_replace_interactive("b", "c"), 1);
        assert_eq!(e.perform_replace_interactive("b", "c"), 0);
        assert_eq!(e.rope.to_string(), "c c\n");
        // A new session resets the resume position to the document start
        // (the input layer does this when entering the confirm phase).
        e.search.replace_resume_char = 0;
        assert_eq!(e.perform_replace_interactive("c", "d"), 1);
        assert_eq!(e.rope.to_string(), "d c\n");
    }
}
