use anyhow::Result;
use ropey::Rope;
use std::cell::Cell;
use std::collections::VecDeque;
use std::fs;
use std::io::{self, BufReader, Read};
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};
use unicode_width::{UnicodeWidthChar, UnicodeWidthStr};

use crate::constants;
use crate::hex::HexViewState;
use crate::search::SearchState;
use crate::syntax::SyntaxHighlighter;

pub(crate) fn line_comment_prefix(syntax: Option<&str>) -> Option<&'static str> {
    match syntax {
        Some("Rust") | Some("C") | Some("C++") | Some("Go") | Some("JavaScript")
        | Some("TypeScript") | Some("Java") | Some("Swift") | Some("Kotlin") | Some("Zig") => {
            Some("// ")
        }
        Some("Python")
        | Some("Ruby")
        | Some("Shell Script (Bash)")
        | Some("Perl")
        | Some("R")
        | Some("YAML")
        | Some("TOML") => Some("# "),
        Some("Lua") | Some("SQL") => Some("-- "),
        Some("HTML") | Some("XML") | Some("CSS") => None,
        _ => Some("// "),
    }
}

pub(crate) fn line_comment_at(line: ropey::RopeSlice<'_>, prefix: &str) -> Option<(usize, usize)> {
    let mut chars = line.chars().enumerate().peekable();
    while chars.peek().is_some_and(|(_, ch)| ch.is_whitespace()) {
        chars.next();
    }
    let start = chars.peek()?.0;
    let marker = prefix.trim_end();
    for expected in marker.chars() {
        if chars.next()?.1 != expected {
            return None;
        }
    }
    let len = marker.chars().count() + usize::from(chars.peek().is_some_and(|&(_, ch)| ch == ' '));
    Some((start, len))
}

pub(crate) fn unindent_len(line: ropey::RopeSlice<'_>, width: usize) -> usize {
    let mut count = 0;
    for ch in line.chars() {
        if ch == ' ' && count < width {
            count += 1;
        } else if ch == '\t' && count == 0 {
            return 1;
        } else {
            break;
        }
    }
    count
}

// Ropey's reader propagates Interrupted; preserve read_to_string's retry behavior.
struct RetryInterrupted<R>(R);

impl<R: Read> Read for RetryInterrupted<R> {
    fn read(&mut self, bytes: &mut [u8]) -> io::Result<usize> {
        loop {
            match self.0.read(bytes) {
                Err(error) if error.kind() == io::ErrorKind::Interrupted => continue,
                result => return result,
            }
        }
    }
}

#[derive(Clone, Debug)]
pub struct UndoState<P = (usize, usize)> {
    pub rope: Rope,
    pub cursor_pos: P,
}

/// Viewport and cursor state
#[derive(Default)]
pub struct ViewportState {
    pub cursor_pos: (usize, usize),
    pub viewport_offset: (usize, usize),
    pub wrapped_offset: usize,
    pub manual_scroll: bool,
}

/// Undo/redo management
#[derive(Default)]
pub struct UndoManager<P = (usize, usize)> {
    pub undo_stack: VecDeque<UndoState<P>>,
    pub redo_stack: VecDeque<UndoState<P>>,
}

impl<P: Clone> UndoManager<P> {
    pub fn save_state(&mut self, rope: &Rope, cursor_pos: P) {
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
    fn apply(&mut self, is_undo: bool, rope: &mut Rope, cursor_pos: &mut P) -> bool {
        let (from, to) = if is_undo {
            (&mut self.undo_stack, &mut self.redo_stack)
        } else {
            (&mut self.redo_stack, &mut self.undo_stack)
        };

        if let Some(state) = from.pop_back() {
            let current = UndoState {
                rope: rope.clone(),
                cursor_pos: cursor_pos.clone(),
            };
            to.push_back(current);
            *rope = state.rope;
            *cursor_pos = state.cursor_pos;
            true
        } else {
            false
        }
    }

    pub fn undo(&mut self, rope: &mut Rope, cursor_pos: &mut P) -> bool {
        self.apply(true, rope, cursor_pos)
    }

    pub fn redo(&mut self, rope: &mut Rope, cursor_pos: &mut P) -> bool {
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
    ConfirmOpen,
    ConfirmOverwrite,
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
    pub saved_content: Option<Rope>,
    newline: &'static str,
    insertion_cursor: Option<(u64, (usize, usize), usize)>,
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
    /// Memo for `line_col_to_char_idx`, as `(line, char_index, display_col)`.
    ///
    /// Invariant: `char_index` is the *first* char index on `line` whose prefix
    /// width equals `display_col`. That is what lets an exact hit on `display_col`
    /// return `char_index` directly -- zero-width characters (combining marks, a
    /// stray `\r`) put several indices at the same column, and this conversion is
    /// defined to yield the first. Only values that preserve the invariant may be
    /// stored here; `char_idx_to_display_col` therefore uses its own slot below.
    col_index_cache: Cell<Option<(usize, usize, usize)>>,
    // Keep both selection endpoints warm when redraws alternate between them.
    previous_col_index_cache: Cell<Option<(usize, usize, usize)>>,
    /// Memo for `char_idx_to_display_col`, same shape but no first-index
    /// requirement: char index -> width is unambiguous in that direction.
    ///
    /// Both conversions are O(column) walks from the start of the line and both
    /// run on every keystroke, so on a long line (minified JS/JSON, a base64 blob,
    /// a single-line SQL dump) they dominated typing and arrow-key latency. Both
    /// slots are cleared by `invalidate_cache` on every mutation.
    width_index_cache: Cell<Option<(usize, usize, usize)>>,
    /// Monotonic counter bumped on every document mutation. Consumers (e.g.
    /// the render layer) can sample this to detect whether the document
    /// changed since the last frame.
    pub dirty_generation: u64,
    /// Lowest line index mutated since a renderer last consumed this value, or
    /// `usize::MAX` when nothing has changed. Lets the render cache evict only the
    /// lines an edit could actually have affected instead of clearing wholesale --
    /// a one-char edit does not change how the lines above the cursor render.
    /// Accumulates the minimum across edits, so it stays correct when several
    /// happen between frames.
    pub dirty_from_line: usize,
    /// Exclusive end of pending dirty lines, or 0 with no pending edits.
    /// `usize::MAX` means line numbering may shift through the rest of the buffer.
    pub dirty_to_line: usize,
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
///
/// The fast path covers printable ASCII, where the display width is exactly the
/// byte length. Everything else — wide chars, hard tabs, and crucially control
/// characters such as the `\r` of a CRLF line ending — falls through to the
/// `char_display_width` accumulation loop. Routing all non-trivial input through
/// that one helper is what keeps this function consistent with
/// `char_idx_to_display_col`: `UnicodeWidthStr::width` counts a lone control
/// char as 1 column while `UnicodeWidthChar::width` reports `None` (0), and any
/// disagreement between the two makes `move_cursor_right` spin forever chasing a
/// column it can never reach.
pub fn line_display_width(rope: &Rope, line: usize) -> usize {
    let rope_line = rope.line(line);
    if let Some(s) = rope_line.as_str() {
        let s = s.trim_end_matches('\n');
        if s.bytes().all(|b| b.is_ascii_graphic() || b == b' ') {
            return s.len();
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
            saved_content: None,
            newline: "\n",
            insertion_cursor: None,
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
            col_index_cache: Cell::new(None),
            previous_col_index_cache: Cell::new(None),
            width_index_cache: Cell::new(None),
            dirty_generation: 0,
            dirty_from_line: usize::MAX,
            dirty_to_line: 0,
            buffer_id: NEXT_BUFFER_ID.fetch_add(1, Ordering::Relaxed),
        }
    }

    /// Create an Editor for unit tests (same as new_buffer).
    pub fn new_for_test() -> Self {
        Self::new_buffer()
    }

    pub fn load_file(&mut self, path: PathBuf) -> Result<()> {
        // Coalesce Ropey's small reads without retaining a full-file temporary.
        let rope = fs::File::open(&path)
            .and_then(|file| {
                Rope::from_reader(BufReader::with_capacity(64 * 1024, RetryInterrupted(file)))
            })
            .map_err(|e| {
                if e.kind() == std::io::ErrorKind::InvalidData {
                    anyhow::anyhow!("{}: binary or non-UTF-8 file", path.display())
                } else {
                    anyhow::Error::from(e)
                }
            })?;
        // Ropey keeps CRLF pairs within one chunk, including across read boundaries.
        let newline = if rope.chunks().any(|chunk| chunk.contains("\r\n")) {
            "\r\n"
        } else {
            "\n"
        };
        *self = Self::new_buffer();
        self.rope = rope;
        self.saved_content = Some(self.rope.clone());
        self.newline = newline;

        let first_line = self.rope.line(0).as_str().map(|s| s.trim_end_matches('\n'));
        self.syntax_name = self.highlighter.detect_syntax(Some(&path), first_line);
        self.highlighter.set_syntax(self.syntax_name.as_deref());

        self.display_name = path
            .file_name()
            .map(|n| n.to_string_lossy().to_string())
            .unwrap_or_else(|| "[untitled]".to_string());
        self.file_path = Some(path);
        self.modified = false;
        self.mark_document_changed(0);
        Ok(())
    }

    fn cursor_char_idx(&self) -> usize {
        if let Some((generation, cursor, index)) = self.insertion_cursor {
            if generation == self.dirty_generation && cursor == self.viewport.cursor_pos {
                return index;
            }
        }
        self.line_col_to_char_idx(self.viewport.cursor_pos.0, self.viewport.cursor_pos.1)
    }

    pub fn insert_char(&mut self, c: char) {
        self.mark_anchor = None;
        self.save_undo_state();
        let line = self.viewport.cursor_pos.0;
        let line_start = self.rope.line_to_char(line);
        let pos = self.cursor_char_idx();
        let exact_cursor = self
            .insertion_cursor
            .is_some_and(|(generation, cursor, index)| {
                generation == self.dirty_generation
                    && cursor == self.viewport.cursor_pos
                    && index == pos
            });
        if !exact_cursor
            && self.col_index_cache.get()
                != Some((line, pos - line_start, self.viewport.cursor_pos.1))
        {
            // A click inside a tab or wide glyph inserts at that glyph's start.
            self.viewport.cursor_pos.1 = self.char_idx_to_display_col(line, pos - line_start);
        }
        let old_line_count = self.rope.len_lines();
        self.rope.insert_char(pos, c);
        self.mark_local_edit(line, pos, old_line_count);
        // Advance cursor directly. Avoids `move_cursor_right`'s cache query
        // on the hot typing path; the cache would be cold anyway because
        // `mark_document_changed` just cleared it.
        if c == '\n' {
            self.viewport.cursor_pos.0 += 1;
            self.viewport.cursor_pos.1 = 0;
        } else {
            let w = char_display_width(c, self.viewport.cursor_pos.1);
            self.viewport.cursor_pos.1 += w;
            // Re-seed the column memo that `mark_document_changed` just cleared.
            // The post-insert cursor position is known exactly here, so the next
            // keystroke's `line_col_to_char_idx` can hit instead of re-walking the
            // line -- which is the whole cost of typing at the end of a long line.
            //
            // Only when `w > 0`: a zero-width char (a combining mark) leaves the
            // prefix width unchanged, so `pos + 1` would not be the *first* index
            // at that column and would violate the memo's invariant.
            if w > 0 {
                self.remember_column_index((
                    line,
                    pos + 1 - line_start,
                    self.viewport.cursor_pos.1,
                ));
            }
        }
        self.insertion_cursor = Some((self.dirty_generation, self.viewport.cursor_pos, pos + 1));
        self.modified = true;
    }

    /// Insert one literal text block as a single undoable edit.
    pub fn insert_text(&mut self, text: &str) {
        self.save_undo_state();
        self.mark_anchor = None;
        let insert_line = self.viewport.cursor_pos.0;
        let pos = self.line_col_to_char_idx(self.viewport.cursor_pos.0, self.viewport.cursor_pos.1);
        let end_col =
            (text.is_ascii() && text.bytes().all(|b| b >= b' ' && b != 0x7f)).then(|| {
                let line_start = self.rope.line_to_char(insert_line);
                self.char_idx_to_display_col(insert_line, pos - line_start) + text.len()
            });
        self.rope.insert(pos, text);
        self.modified = true;
        let end_pos = pos
            + if end_col.is_some() {
                text.len()
            } else {
                text.chars().count()
            };
        self.mark_document_changed(insert_line);
        let line = self.rope.char_to_line(end_pos);
        // Inserting between CR and LF can split a line even when the paste is ASCII.
        let display_col = match end_col {
            Some(col) if line == insert_line => col,
            _ => {
                let line_start = self.rope.line_to_char(line);
                self.char_idx_to_display_col(line, end_pos - line_start)
            }
        };
        self.viewport.cursor_pos = (line, display_col);
    }

    pub fn delete_char(&mut self) {
        self.mark_anchor = None;
        // A mouse click or `+LINE,COL` can park the cursor at a display column
        // inside a wide char or a tab's span. `line_col_to_char_idx` then snaps
        // back to that char's start, so a nonzero column can still resolve to
        // the first char of the line -- which is the line-join case, not the
        // delete-within-line case. Deciding on the resolved char index rather
        // than the display column keeps `pos - 1 - line_start` from underflowing.
        let line_start = self.rope.line_to_char(self.viewport.cursor_pos.0);
        let resolved = self.cursor_char_idx();
        if resolved > line_start {
            let pos = resolved;
            if pos > 0 {
                self.save_undo_state();
                // Land the cursor on the deleted char's start column. Its
                // display width is position-dependent (a wide CJK char
                // occupies 2 columns, a tab up to TAB_WIDTH), so compute the
                // column from the char index before mutating the rope.
                let new_col =
                    self.char_idx_to_display_col(self.viewport.cursor_pos.0, pos - 1 - line_start);
                let old_line_count = self.rope.len_lines();
                self.rope.remove(pos - 1..pos);
                self.mark_local_edit(self.viewport.cursor_pos.0, pos - 1, old_line_count);
                self.viewport.cursor_pos.1 = new_col;
                self.width_index_cache.set(Some((
                    self.viewport.cursor_pos.0,
                    pos - 1 - line_start,
                    new_col,
                )));
                self.insertion_cursor =
                    Some((self.dirty_generation, self.viewport.cursor_pos, pos - 1));
                self.modified = true;
            }
        } else if self.viewport.cursor_pos.0 > 0 {
            let pos = self.line_col_to_char_idx(self.viewport.cursor_pos.0, 0);
            if pos > 0 {
                self.save_undo_state();

                let junction_col = self.line_display_width_cached(self.viewport.cursor_pos.0 - 1);

                let start = if pos >= 2
                    && self.rope.char(pos - 2) == '\r'
                    && self.rope.char(pos - 1) == '\n'
                {
                    pos - 2
                } else {
                    pos - 1
                };
                let old_line_count = self.rope.len_lines();
                self.rope.remove(start..pos);
                self.mark_local_edit(self.viewport.cursor_pos.0 - 1, start, old_line_count);
                self.viewport.cursor_pos.0 -= 1;
                self.viewport.cursor_pos.1 = junction_col;
                self.modified = true;
            }
        }
    }

    pub fn insert_newline(&mut self, auto_indent: bool) {
        self.mark_anchor = None;
        self.save_undo_state();
        let pos = self.cursor_char_idx();

        let insert_str = self.newline_with_indent(pos, auto_indent);
        let indent = &insert_str[self.newline.len()..];
        self.rope.insert(pos, &insert_str);
        self.mark_document_changed(self.viewport.cursor_pos.0);
        self.viewport.cursor_pos.0 += 1;
        // The indent starts at column 0; it may contain hard tabs.
        self.viewport.cursor_pos.1 = str_display_width(indent, 0);
        self.modified = true;
    }

    pub(crate) fn newline_with_indent(&self, pos: usize, auto_indent: bool) -> String {
        let mut text = self.newline.to_owned();
        if auto_indent {
            let start = self.rope.line_to_char(self.rope.char_to_line(pos));
            for ch in self
                .rope
                .slice(start..pos)
                .chars()
                .take_while(|ch| matches!(ch, ' ' | '\t'))
            {
                text.push(ch);
            }
        }
        text
    }

    pub fn delete_char_forward(&mut self) {
        // Matches `insert_char` / `delete_char` / `insert_newline`: an edit
        // invalidates the mark, which would otherwise point at a line the rope
        // may no longer have.
        self.mark_anchor = None;
        let pos = self.cursor_char_idx();
        if pos < self.rope.len_chars() {
            self.save_undo_state();
            let end = if self.rope.char(pos) == '\r'
                && pos + 1 < self.rope.len_chars()
                && self.rope.char(pos + 1) == '\n'
            {
                pos + 2
            } else {
                pos + 1
            };
            let old_line_count = self.rope.len_lines();
            self.rope.remove(pos..end);
            self.mark_local_edit(self.viewport.cursor_pos.0, pos, old_line_count);
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
        let (line, target) = self.viewport.cursor_pos;
        let (offset, mut col) = match self.col_index_cache.get() {
            Some((cached_line, index, cached_col))
                if cached_line == line && cached_col == target =>
            {
                (index, cached_col)
            }
            _ => {
                let line_start = self.rope.line_to_char(line);
                let offset = self.line_col_to_char_idx(line, target) - line_start;
                (offset, self.char_idx_to_display_col(line, offset))
            }
        };
        for (step, ch) in self.rope.line(line).chars_at(offset).enumerate() {
            if ch == '\n' {
                break;
            }
            col += char_display_width(ch, col);
            if col > target {
                let index = offset + step + 1;
                self.viewport.cursor_pos.1 = col;
                self.width_index_cache.set(Some((line, index, col)));
                self.remember_column_index((line, index, col));
                return;
            }
        }
        if line < self.rope.len_lines().saturating_sub(1) {
            self.viewport.cursor_pos = (line + 1, 0);
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
        self.viewport.cursor_pos.0 = self.viewport.cursor_pos.0.min(self.rope.len_lines() - 1);
        let target = self.viewport.cursor_pos.1;
        if target == 0 {
            return;
        }
        let line = self.viewport.cursor_pos.0;
        if let Some((cached_line, width)) = self.line_width_cache.get() {
            if cached_line == line {
                self.viewport.cursor_pos.1 = target.min(width);
                return;
            }
        }
        let rope_line = self.rope.line(line);
        if let Some(text) = rope_line.as_str() {
            let text = text.trim_end_matches('\n');
            let prefix = &text.as_bytes()[..target.min(text.len())];
            if prefix.iter().all(|&b| b.is_ascii_graphic() || b == b' ') {
                if target >= text.len() {
                    self.line_width_cache.set(Some((line, text.len())));
                    self.viewport.cursor_pos.1 = text.len();
                }
                return;
            }
        }
        // Clamping only needs to know whether the line reaches the cursor column.
        let mut width = 0;
        for ch in rope_line.chars().filter(|&ch| ch != '\n') {
            width += char_display_width(ch, width);
            if width >= target {
                return;
            }
        }
        self.line_width_cache.set(Some((line, width)));
        self.viewport.cursor_pos.1 = width;
    }

    /// Convert a char offset (number of chars from line start) to a display column,
    /// accounting for character widths (e.g. wide CJK characters, hard tabs).
    pub fn char_idx_to_display_col(&self, line: usize, char_offset: usize) -> usize {
        // Clamp: a stale `mark_anchor` or viewport offset can outlive the lines
        // it referred to (see `delete_char_forward`), and ropey panics on an
        // out-of-range line index rather than saturating.
        let line = line.min(self.rope.len_lines().saturating_sub(1));
        let rope_line = self.rope.line(line);

        if let Some((cached_line, mut i, mut col)) = self.width_index_cache.get() {
            if cached_line == line && i > char_offset && i - char_offset < char_offset {
                let mut chars = rope_line.chars_at(i);
                while i > char_offset {
                    let ch = chars.prev().unwrap();
                    if ch == '\t' {
                        break;
                    } // Tab widths require the preceding column.
                    col -= char_display_width(ch, 0);
                    i -= 1;
                }
                if i == char_offset {
                    self.width_index_cache.set(Some((line, i, col)));
                    return col;
                }
            }
        }

        // Resume from the memo when it refers to this line and sits at or before
        // the requested offset; otherwise start from the beginning of the line.
        // `chars_at` seeks in O(log n), so resuming is genuinely cheaper than
        // rescanning -- which is what made every keystroke O(column) on long lines.
        let (mut i, mut display_col) = match self.width_index_cache.get() {
            Some((cached_line, cached_i, cached_col))
                if cached_line == line && cached_i <= char_offset =>
            {
                (cached_i, cached_col)
            }
            _ => (0, 0),
        };

        for ch in rope_line.chars_at(i) {
            if i >= char_offset || ch == '\n' {
                break;
            }
            display_col += char_display_width(ch, display_col);
            i += 1;
        }

        // Invariant: `display_col` is the width of this line's first `i` chars.
        self.width_index_cache.set(Some((line, i, display_col)));
        display_col
    }

    pub fn line_col_to_char_idx(&self, line: usize, col: usize) -> usize {
        // Clamp for the same reason as `char_idx_to_display_col`: callers such as
        // `get_selection_range` resolve positions that may name a line the rope
        // no longer has.
        let line = line.min(self.rope.len_lines().saturating_sub(1));
        let line_start = self.rope.line_to_char(line);
        let rope_line = self.rope.line(line);

        let cached = [
            self.col_index_cache.get(),
            self.previous_col_index_cache.get(),
        ];
        let nearest = cached
            .iter()
            .flatten()
            .filter(|&&(cached_line, _, _)| cached_line == line)
            .min_by_key(|&&(_, _, cached_col)| cached_col.abs_diff(col))
            .copied();
        if let Some((_, cached_i, cached_col)) = nearest {
            if cached_col == col {
                self.remember_column_index((line, cached_i, cached_col));
                return line_start + cached_i;
            }
        }

        if let Some((_, mut i, mut display_col)) = nearest {
            // Near the line start, walking forward costs less than undoing a long prefix.
            if display_col > col && display_col - col < col {
                let mut chars = rope_line.chars_at(i);
                while display_col > col {
                    let ch = chars.prev().unwrap();
                    if ch == '\t' {
                        break;
                    }
                    i -= 1;
                    display_col -= char_display_width(ch, 0);
                }
                if display_col <= col {
                    if display_col == col {
                        while let Some(ch) = chars.prev() {
                            if ch == '\t' || char_display_width(ch, 0) > 0 {
                                break;
                            }
                            i -= 1;
                        }
                        self.remember_column_index((line, i, display_col));
                    }
                    return line_start + i;
                }
            }
        }

        let (mut char_idx, mut display_col) = cached
            .iter()
            .flatten()
            .filter(|&&(cached_line, _, cached_col)| cached_line == line && cached_col < col)
            .max_by_key(|&&(_, _, cached_col)| cached_col)
            .map(|&(_, i, col)| (i, col))
            .unwrap_or((0, 0));

        // Tracks whether the walk actually landed on `col`. If it stopped early --
        // end of line, or `col` falling inside a wide char's span -- then trailing
        // zero-width chars may already have been consumed at the final width, so
        // `char_idx` would not be the first index there and storing it would break
        // the invariant the exact-hit path above depends on.
        let mut reached_target = false;

        for ch in rope_line.chars_at(char_idx) {
            if display_col >= col {
                reached_target = true;
                break;
            }
            if ch == '\n' {
                break;
            }
            let w = char_display_width(ch, display_col);
            if display_col + w > col {
                // `col` falls inside this char's span (mid-tab, mid-CJK):
                // snap to the char itself rather than past it.
                break;
            }
            char_idx += 1;
            display_col += w;
        }

        if reached_target {
            self.remember_column_index((line, char_idx, display_col));
        }
        line_start + char_idx
    }

    fn remember_column_index(&self, entry: (usize, usize, usize)) {
        if self.col_index_cache.get() != Some(entry) {
            self.previous_col_index_cache
                .set(self.col_index_cache.get());
            self.col_index_cache.set(Some(entry));
        }
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

        if self.viewport.manual_scroll {
            return;
        }
        let content_width = editor_width.saturating_sub(line_num_width);

        if word_wrap {
            // Word wrap mode: no horizontal scrolling
            self.viewport.viewport_offset.1 = 0;
            self.update_viewport_vertical_word_wrap(editor_height, content_width);
        } else {
            self.viewport.wrapped_offset = 0;
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
            self.viewport.wrapped_offset = 0;
        }

        if self.viewport.wrapped_offset > 0 {
            self.viewport.wrapped_offset = self
                .viewport
                .wrapped_offset
                .min(self.wrapped_line_height(self.viewport.viewport_offset.0, content_width) - 1);
        }
        let cursor_sub = (self.viewport.cursor_pos.1 / content_width)
            .min(self.wrapped_line_height(cursor_line, content_width) - 1);
        if self.viewport.viewport_offset.0 == cursor_line
            && self.viewport.wrapped_offset > cursor_sub
        {
            self.viewport.wrapped_offset = cursor_sub;
        }
        let mut cursor_row = cursor_sub;
        for line in self.viewport.viewport_offset.0..cursor_line {
            cursor_row += self.wrapped_line_height(line, content_width);
            if cursor_row >= editor_height + self.viewport.wrapped_offset {
                break;
            }
        }
        if cursor_row < editor_height + self.viewport.wrapped_offset {
            return;
        }
        self.viewport.viewport_offset.0 = cursor_line;
        self.viewport.wrapped_offset = cursor_sub.saturating_sub(editor_height - 1);
        let mut available = (editor_height - 1).saturating_sub(cursor_sub);
        while self.viewport.viewport_offset.0 > 0 {
            let previous = self.viewport.viewport_offset.0 - 1;
            let height = self.wrapped_line_height(previous, content_width);
            if height > available {
                break;
            }
            available -= height;
            self.viewport.viewport_offset.0 = previous;
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

    #[cfg(feature = "terminal")]
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
            MouseEventKind::ScrollDown
                if self.viewport.viewport_offset.0
                    < self.rope.len_lines().saturating_sub(terminal_height) =>
            {
                self.viewport.viewport_offset.0 = (self.viewport.viewport_offset.0
                    + constants::SCROLL_SPEED)
                    .min(self.rope.len_lines().saturating_sub(terminal_height));
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
                // The hex cursor moves by raw bytes, so it routinely sits inside a
                // multi-byte char. `byte_to_char` snaps to the containing char;
                // slicing a `String` at this offset used to panic instead. Going
                // through the rope also avoids materializing the whole document.
                let byte_offset = state.cursor.min(self.rope.len_bytes());
                let char_idx = self.rope.byte_to_char(byte_offset);
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

        // Convert the text cursor to a byte offset via the rope (O(log n)) before
        // materializing, then hand the buffer over without a second copy.
        let char_idx =
            self.line_col_to_char_idx(self.viewport.cursor_pos.0, self.viewport.cursor_pos.1);
        let byte_offset = self.rope.char_to_byte(char_idx.min(self.rope.len_chars()));
        let bytes = self.rope.to_string().into_bytes();

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

            let matches = &self.search.search_matches;
            let before_cursor = |&(line, col): &(usize, usize)| {
                self.rope.line_to_char(line) + col < cursor_char_idx
            };
            // Common first-result and wraparound cases need no binary search.
            let index = if !before_cursor(&matches[0]) || before_cursor(matches.last().unwrap()) {
                0
            } else {
                matches.partition_point(before_cursor)
            };
            self.search.current_match_index = Some(index);

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

        // Locate matches with the same engine find uses, so replace honors
        // regex and case-insensitive modes.
        self.search.search_buffer = search_term.to_string();
        let spans = self.search.match_spans(&self.rope, 0, usize::MAX);

        // Convert (line, char_col, char_len) to absolute char ranges,
        // dropping overlapping matches (the literal scanner can report
        // overlaps) so reverse application stays sound.
        let first_line = spans.first().map(|&(line, _, _)| line).unwrap_or(0);
        let mut ranges: Vec<(usize, usize)> = Vec::with_capacity(spans.len());
        let mut prev_end = 0;
        let mut offset_line = first_line;
        let mut line_start = self.rope.line_to_char(first_line);
        let mut lines = self.rope.lines_at(first_line);
        for &(line, col, len) in &spans {
            if line == offset_line + 1 {
                line_start += lines.next().unwrap().len_chars();
            } else if line != offset_line {
                line_start = self.rope.line_to_char(line);
                lines = self.rope.lines_at(line);
            }
            offset_line = line;
            let start = line_start + col;
            if start >= prev_end {
                ranges.push((start, start + len));
                prev_end = start + len;
            }
        }

        let replacements = ranges.len();
        if replacements > 0 {
            self.save_undo_state();
            let first = ranges[0].0;
            let last = ranges.last().unwrap().1;
            // Rebuild dense bulk edits once; sparse edits keep sharing untouched text.
            if replacements >= 32 && (last - first) / replacements <= 128 {
                let mut builder = ropey::RopeBuilder::new();
                let mut previous = first;
                for &(start, end) in &ranges {
                    for chunk in self.rope.slice(previous..start).chunks() {
                        builder.append(chunk);
                    }
                    builder.append(replace_term);
                    previous = end;
                }
                let replacement = builder.finish();
                let suffix = self.rope.split_off(last);
                self.rope.remove(first..);
                self.rope.append(replacement);
                self.rope.append(suffix);
            } else {
                for &(start, end) in ranges.iter().rev() {
                    self.rope.remove(start..end);
                    self.rope.insert(start, replace_term);
                }
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
    fn next_interactive_match(
        &mut self,
        search_term: &str,
    ) -> Option<(usize, usize, usize, usize)> {
        // Locate matches with the same engine find uses, so replace honors
        // regex and case-insensitive modes.
        self.search.search_buffer = search_term.to_string();
        let resume = self.search.replace_resume_char;
        self.search
            .first_match_span(&self.rope, resume)
            .and_then(|(line, col, len)| {
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

    /// Change line prefixes; the caller handles undo, cursor, and cache state.
    pub(crate) fn edit_line_prefixes(
        &mut self,
        start: usize,
        end: usize,
        prefix: &str,
        mut edit: impl FnMut(usize, ropey::RopeSlice<'_>) -> (usize, usize),
    ) {
        if start > end || start >= self.rope.len_lines() {
            return;
        }
        let end = end.min(self.rope.len_lines() - 1);
        let count = end - start + 1;
        if count >= 32 {
            let first = self.rope.line_to_char(start);
            let last = if end + 1 < self.rope.len_lines() {
                self.rope.line_to_char(end + 1)
            } else {
                self.rope.len_chars()
            };
            // Like dense replacement, rebuild short lines once rather than editing the tree per line.
            if (last - first) / count <= 128 {
                let mut builder = ropey::RopeBuilder::new();
                for (index, line) in self.rope.lines_at(start).take(count).enumerate() {
                    let (offset, len) = edit(start + index, line);
                    if offset != 0 {
                        for chunk in line.slice(..offset).chunks() {
                            builder.append(chunk);
                        }
                    }
                    builder.append(prefix);
                    for chunk in line.slice(offset + len..).chunks() {
                        builder.append(chunk);
                    }
                }
                let replacement = builder.finish();
                let suffix = self.rope.split_off(last);
                self.rope.remove(first..);
                self.rope.append(replacement);
                self.rope.append(suffix);
                return;
            }
        }
        for line in (start..=end).rev() {
            let first = self.rope.line_to_char(line);
            let (offset, len) = edit(line, self.rope.line(line));
            let position = first + offset;
            if len > 0 {
                self.rope.remove(position..position + len);
            }
            if !prefix.is_empty() {
                self.rope.insert(position, prefix);
            }
        }
    }

    /// Toggle line comment on selected lines (or current line).
    pub fn toggle_comment(&mut self) {
        let Some(comment_str) = line_comment_prefix(self.syntax_name.as_deref()) else {
            return;
        };

        let (start_line, end_line) = self.get_affected_lines();
        self.save_undo_state();

        let all_commented = (start_line..=end_line).all(|line_idx| {
            line_idx >= self.rope.len_lines()
                || line_comment_at(self.rope.line(line_idx), comment_str).is_some()
        });

        self.edit_line_prefixes(
            start_line,
            end_line,
            if all_commented { "" } else { comment_str },
            |_, line| {
                if all_commented {
                    line_comment_at(line, comment_str).unwrap_or((0, 0))
                } else {
                    (0, 0)
                }
            },
        );

        self.mark_anchor = None;
        self.modified = true;
        self.mark_document_changed(start_line);
        self.clamp_cursor_to_line();
    }

    /// Move cursor to the start of the next word.
    pub fn move_word_right(&mut self) {
        let line = self.viewport.cursor_pos.0;
        let target_col = self.viewport.cursor_pos.1;
        let line_start = self.rope.line_to_char(line);
        let index = self.line_col_to_char_idx(line, target_col) - line_start;
        let mut col = self.char_idx_to_display_col(line, index);
        let rope_line = self.rope.line(line);
        let mut chars = rope_line
            .chars_at(index)
            .take_while(|&c| c != '\n')
            .peekable();
        // Match the existing behavior for clicks inside a tab or wide character.
        if col < target_col {
            if let Some(ch) = chars.next() {
                col += char_display_width(ch, col);
            }
        }
        while let Some(&ch) = chars.peek() {
            if ch.is_whitespace() {
                break;
            }
            col += char_display_width(ch, col);
            chars.next();
        }
        while let Some(&ch) = chars.peek() {
            if !ch.is_whitespace() {
                break;
            }
            col += char_display_width(ch, col);
            chars.next();
        }
        if chars.peek().is_none() && line + 1 < self.rope.len_lines() {
            self.viewport.cursor_pos = (line + 1, 0);
        } else {
            self.viewport.cursor_pos.1 = col;
        }
    }

    /// Move cursor to the start of the previous word.
    pub fn move_word_left(&mut self) {
        let line = self.viewport.cursor_pos.0;
        let target_col = self.viewport.cursor_pos.1;
        if target_col == 0 {
            if line > 0 {
                self.viewport.cursor_pos = (line - 1, self.line_display_width_cached(line - 1));
            }
            return;
        }
        let rope_line = self.rope.line(line);
        let mut index = self.line_col_to_char_idx(line, target_col) - self.rope.line_to_char(line);
        if self.char_idx_to_display_col(line, index) < target_col
            && index < rope_line.len_chars()
            && rope_line.char(index) != '\n'
        {
            index += 1;
        }
        let mut chars = rope_line.chars_at(index).reversed().peekable();
        while chars.peek().is_some_and(|c| c.is_whitespace()) {
            chars.next();
            index -= 1;
        }
        while chars.peek().is_some_and(|c| !c.is_whitespace()) {
            chars.next();
            index -= 1;
        }
        self.viewport.cursor_pos.1 = self.char_idx_to_display_col(line, index);
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
        let pos = self.cursor_char_idx();
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
            for (offset, c) in self.rope.chars_at(pos + 1).enumerate() {
                let i = pos + 1 + offset;
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
            for (offset, c) in self.rope.chars_at(pos).reversed().enumerate() {
                let i = pos - 1 - offset;
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
            let pos =
                self.line_col_to_char_idx(self.viewport.cursor_pos.0, self.viewport.cursor_pos.1);
            let suffix = &self.word_complete_candidates[self.word_complete_index]
                [self.word_complete_prefix_len..];
            let width = UnicodeWidthStr::width(suffix);
            self.rope.insert(pos, suffix);
            self.mark_document_changed(self.viewport.cursor_pos.0);
            self.viewport.cursor_pos.1 += width;
            self.modified = true;
            self.word_complete_cursor = Some(self.viewport.cursor_pos);
            return;
        }

        let pos = self.cursor_char_idx();
        let prefix_chars = self
            .rope
            .chars_at(pos)
            .reversed()
            .take_while(|c| c.is_alphanumeric() || *c == '_')
            .count();
        let prefix = self.rope.slice(pos - prefix_chars..pos).to_string();

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
            if buf.len() > prefix_len && buf.starts_with(prefix_str) && !seen.contains(buf.as_str())
            {
                seen.insert(buf.clone());
                candidates.push(buf.clone());
            }
            buf.clear();
        };
        for ch in self.rope.chunks().flat_map(str::chars) {
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
        let pos = self.cursor_char_idx();
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
        // Both position memos must be cleared with the width cache: any edit can
        // shift every char index after the edit point.
        self.col_index_cache.set(None);
        self.previous_col_index_cache.set(None);
        self.width_index_cache.set(None);
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

    /// Invalidate highlighting and text caches from a given line onwards.
    pub fn mark_document_changed(&mut self, from_line: usize) {
        self.mark_changed_range(from_line, usize::MAX);
    }

    /// Track an insertion/deletion at one position, preserving unshifted line caches.
    pub(crate) fn mark_local_edit(
        &mut self,
        from_line: usize,
        position: usize,
        old_line_count: usize,
    ) {
        let after_line = self.rope.char_to_line(position.min(self.rope.len_chars()));
        let first = from_line.min(after_line);
        if self.rope.len_lines() == old_line_count {
            // CRLF boundary changes can touch a neighboring line without changing the count.
            self.mark_changed_range(first, from_line.max(after_line).saturating_add(1));
        } else {
            self.mark_document_changed(first);
        }
    }

    pub(crate) fn mark_changed_range(&mut self, first: usize, end: usize) {
        self.dirty_generation = self.dirty_generation.wrapping_add(1);
        self.dirty_from_line = self.dirty_from_line.min(first);
        self.dirty_to_line = self.dirty_to_line.max(end);
        self.highlighter.invalidate_cache_range(first, end);
        self.invalidate_cache();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn streaming_reader_retries_interrupts_and_handles_short_utf8_reads() {
        struct ShortReads {
            data: io::Cursor<Vec<u8>>,
            count: usize,
            limit: usize,
        }
        impl Read for ShortReads {
            fn read(&mut self, bytes: &mut [u8]) -> io::Result<usize> {
                self.count += 1;
                if self.count % 3 == 1 {
                    return Err(io::ErrorKind::Interrupted.into());
                }
                let length = bytes.len().min(self.limit);
                self.data.read(&mut bytes[..length])
            }
        }
        let text = "界🙂e\u{301}\r\nnext\n".repeat(300);
        for limit in [1, 2, 3, 7, 2049] {
            let reader = ShortReads {
                data: io::Cursor::new(text.as_bytes().to_vec()),
                count: 0,
                limit,
            };
            let rope = Rope::from_reader(BufReader::with_capacity(
                64 * 1024,
                RetryInterrupted(reader),
            ))
            .unwrap();
            assert_eq!(rope.to_string(), text);
            assert_eq!(rope.len_lines(), Rope::from_str(&text).len_lines());
        }
    }

    #[test]
    fn streaming_reader_propagates_hard_io_errors_after_a_prefix() {
        struct FailingRead(bool);
        impl Read for FailingRead {
            fn read(&mut self, bytes: &mut [u8]) -> io::Result<usize> {
                if self.0 {
                    return Err(io::ErrorKind::PermissionDenied.into());
                }
                self.0 = true;
                bytes[..5].copy_from_slice(b"valid");
                Ok(5)
            }
        }
        let error = Rope::from_reader(BufReader::with_capacity(
            64 * 1024,
            RetryInterrupted(FailingRead(false)),
        ))
        .unwrap_err();
        assert_eq!(error.kind(), io::ErrorKind::PermissionDenied);
    }

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
