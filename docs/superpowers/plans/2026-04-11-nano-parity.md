# Nano Feature Parity Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Implement all missing nano features to achieve full nano parity, with a test harness to validate each feature.

**Architecture:** Add a test harness module with a `TestEditor` builder and key simulation helpers. Then implement features in dependency order: clipboard primitives first, then selection mode (which depends on clipboard), then editing features that depend on selection (indent/comment), then navigation and search enhancements. Each feature includes tests written by the same implementer.

**Tech Stack:** Rust, ropey, crossterm (KeyEvent simulation for tests), regex crate (already in Cargo.toml)

---

## File Structure

| File | Action | Responsibility |
|------|--------|---------------|
| `tests/helpers/mod.rs` | Create | `TestEditor` builder, key simulation, assertion helpers |
| `tests/test_clipboard.rs` | Create | Tests for cut/copy/paste |
| `tests/test_selection.rs` | Create | Tests for mark mode and selection operations |
| `tests/test_navigation.rs` | Create | Tests for word jump, file start/end, bracket match |
| `tests/test_editing.rs` | Create | Tests for auto-indent, indent/unindent, comment, delete fwd |
| `tests/test_search.rs` | Create | Tests for regex search |
| `src/editor.rs` | Modify | Add clipboard, selection, auto-indent, word nav, bracket match, cursor info |
| `src/input.rs` | Modify | Wire all new keybindings |
| `src/ui.rs` | Modify | Selection highlighting, whitespace display, cursor position display, updated help modal |
| `src/config.rs` | Modify | Add auto_indent, whitespace_display, constant_cursor_position config options |

---

### Task 1: Test Harness

**Files:**
- Create: `tests/helpers/mod.rs`
- Create: `tests/test_editing.rs` (initial smoke tests for existing features)

- [ ] **Step 1: Create `tests/helpers/mod.rs` with TestEditor builder and helpers**

```rust
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use std::path::PathBuf;

// Re-export for convenience in test files
pub use rune::editor::{Editor, InputMode};
pub use rune::input::handle_key_event;

/// Builder for creating an Editor with specific state for testing.
/// Bypasses filesystem and terminal dependencies.
pub struct TestEditor;

impl TestEditor {
    /// Create an editor with the given text content and cursor at (0, 0).
    pub fn with_content(content: &str) -> Editor {
        let mut editor = Editor::new_for_test();
        editor.rope = ropey::Rope::from_str(content);
        editor
    }

    /// Create an empty editor.
    pub fn empty() -> Editor {
        Editor::new_for_test()
    }
}

/// Simulate a key press through the full input handling pipeline.
pub fn send_key(editor: &mut Editor, code: KeyCode) {
    let key = KeyEvent::new(code, KeyModifiers::NONE);
    let _ = handle_key_event(editor, key);
}

/// Simulate a Ctrl+key press.
pub fn send_ctrl(editor: &mut Editor, c: char) {
    let key = KeyEvent::new(KeyCode::Char(c), KeyModifiers::CONTROL);
    let _ = handle_key_event(editor, key);
}

/// Simulate an Alt+key press.
pub fn send_alt(editor: &mut Editor, c: char) {
    let key = KeyEvent::new(KeyCode::Char(c), KeyModifiers::ALT);
    let _ = handle_key_event(editor, key);
}

/// Simulate typing a string character by character.
pub fn type_string(editor: &mut Editor, s: &str) {
    for c in s.chars() {
        send_key(editor, KeyCode::Char(c));
    }
}

/// Send a sequence of key events.
pub fn send_keys(editor: &mut Editor, keys: &[KeyEvent]) {
    for key in keys {
        let _ = handle_key_event(editor, *key);
    }
}

/// Create a KeyEvent for Ctrl+key.
pub fn ctrl(c: char) -> KeyEvent {
    KeyEvent::new(KeyCode::Char(c), KeyModifiers::CONTROL)
}

/// Create a KeyEvent for Alt+key.
pub fn alt(c: char) -> KeyEvent {
    KeyEvent::new(KeyCode::Char(c), KeyModifiers::ALT)
}

/// Create a KeyEvent for a plain key.
pub fn key(code: KeyCode) -> KeyEvent {
    KeyEvent::new(code, KeyModifiers::NONE)
}

/// Get the full text content of the editor as a String.
pub fn content(editor: &Editor) -> String {
    editor.rope.to_string()
}

/// Get the cursor position as (line, col).
pub fn cursor(editor: &Editor) -> (usize, usize) {
    editor.viewport.cursor_pos
}

/// Set cursor position directly for test setup.
pub fn set_cursor(editor: &mut Editor, line: usize, col: usize) {
    editor.viewport.cursor_pos = (line, col);
}
```

- [ ] **Step 2: Add `new_for_test()` constructor to Editor**

In `src/editor.rs`, add a constructor that doesn't load config from disk:

```rust
    /// Create an Editor for testing — doesn't touch filesystem for config.
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
        }
    }
```

- [ ] **Step 3: Make modules public for integration tests**

In `src/main.rs`, change the module declarations to `pub mod` so integration tests can access them. Also add a `lib.rs`:

Create `src/lib.rs`:
```rust
pub mod constants {
    pub use crate::main_constants::*;
}

pub mod config;
pub mod editor;
pub mod hex;
pub mod input;
pub mod search;
pub mod syntax;
pub mod ui;
```

Actually, the simpler approach: convert `src/main.rs` module declarations to `pub mod` and add a `src/lib.rs` that re-exports. In `src/main.rs`, keep the modules private but add a `src/lib.rs`:

```rust
// src/lib.rs
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

pub mod config;
pub mod editor;
pub mod hex;
pub mod input;
pub mod search;
pub mod syntax;
pub mod ui;
```

Then update `src/main.rs` to use the lib:
```rust
use rune::constants;
use rune::editor;
use rune::input;
use rune::ui;
```

Remove the `mod` declarations and `constants` module from `main.rs`, keeping only `main()` and `run_editor()`.

- [ ] **Step 4: Create initial smoke tests**

Create `tests/test_editing.rs`:
```rust
mod helpers;
use helpers::*;
use crossterm::event::KeyCode;

#[test]
fn test_insert_char() {
    let mut editor = TestEditor::with_content("hello\n");
    set_cursor(&mut editor, 0, 5);
    type_string(&mut editor, "!");
    assert_eq!(content(&editor), "hello!\n");
    assert_eq!(cursor(&editor), (0, 6));
}

#[test]
fn test_insert_newline() {
    let mut editor = TestEditor::with_content("hello world\n");
    set_cursor(&mut editor, 0, 5);
    send_key(&mut editor, KeyCode::Enter);
    assert_eq!(content(&editor), "hello\n world\n");
    assert_eq!(cursor(&editor), (1, 0));
}

#[test]
fn test_backspace() {
    let mut editor = TestEditor::with_content("hello\n");
    set_cursor(&mut editor, 0, 5);
    send_key(&mut editor, KeyCode::Backspace);
    assert_eq!(content(&editor), "hell\n");
    assert_eq!(cursor(&editor), (0, 4));
}

#[test]
fn test_backspace_joins_lines() {
    let mut editor = TestEditor::with_content("hello\nworld\n");
    set_cursor(&mut editor, 1, 0);
    send_key(&mut editor, KeyCode::Backspace);
    assert_eq!(content(&editor), "helloworld\n");
    assert_eq!(cursor(&editor).0, 0);
}

#[test]
fn test_undo_redo() {
    let mut editor = TestEditor::with_content("hello\n");
    set_cursor(&mut editor, 0, 5);
    type_string(&mut editor, "!");
    assert_eq!(content(&editor), "hello!\n");
    send_ctrl(&mut editor, 'z'); // undo
    assert_eq!(content(&editor), "hello\n");
    send_ctrl(&mut editor, 'r'); // redo
    assert_eq!(content(&editor), "hello!\n");
}

#[test]
fn test_cursor_movement() {
    let mut editor = TestEditor::with_content("hello\nworld\n");
    set_cursor(&mut editor, 0, 0);
    send_key(&mut editor, KeyCode::Right);
    assert_eq!(cursor(&editor), (0, 1));
    send_key(&mut editor, KeyCode::Down);
    assert_eq!(cursor(&editor), (1, 1));
    send_key(&mut editor, KeyCode::Left);
    assert_eq!(cursor(&editor), (1, 0));
    send_key(&mut editor, KeyCode::Up);
    assert_eq!(cursor(&editor), (0, 0));
}

#[test]
fn test_home_end() {
    let mut editor = TestEditor::with_content("hello world\n");
    set_cursor(&mut editor, 0, 5);
    send_key(&mut editor, KeyCode::Home);
    assert_eq!(cursor(&editor).1, 0);
    send_key(&mut editor, KeyCode::End);
    assert_eq!(cursor(&editor).1, 11);
}
```

- [ ] **Step 5: Verify tests pass**

Run: `cargo test`
Expected: All tests pass.

- [ ] **Step 6: Commit**

```bash
git add src/lib.rs src/main.rs src/editor.rs tests/
git commit -m "feat: add test harness with TestEditor builder and smoke tests"
```

---

### Task 2: Clipboard — Cut Line, Copy Line, Paste (Ctrl+K, Alt+6, Ctrl+U)

**Files:**
- Modify: `src/editor.rs` — add clipboard buffer, cut_line, copy_line, paste methods
- Modify: `src/input.rs` — wire Ctrl+K, Alt+6, Ctrl+U keybindings
- Create: `tests/test_clipboard.rs`

- [ ] **Step 1: Write clipboard tests**

Create `tests/test_clipboard.rs`:
```rust
mod helpers;
use helpers::*;
use crossterm::event::KeyCode;

#[test]
fn test_cut_line() {
    let mut editor = TestEditor::with_content("line1\nline2\nline3\n");
    set_cursor(&mut editor, 1, 0);
    send_ctrl(&mut editor, 'k');
    assert_eq!(content(&editor), "line1\nline3\n");
    assert_eq!(cursor(&editor), (1, 0));
}

#[test]
fn test_cut_last_line() {
    let mut editor = TestEditor::with_content("line1\nline2\n");
    set_cursor(&mut editor, 1, 0);
    send_ctrl(&mut editor, 'k');
    assert_eq!(content(&editor), "line1\n");
    assert_eq!(cursor(&editor).0, 0);
}

#[test]
fn test_paste_after_cut() {
    let mut editor = TestEditor::with_content("line1\nline2\nline3\n");
    set_cursor(&mut editor, 0, 0);
    send_ctrl(&mut editor, 'k'); // cut line1
    assert_eq!(content(&editor), "line2\nline3\n");
    set_cursor(&mut editor, 1, 0);
    send_ctrl(&mut editor, 'u'); // paste
    assert_eq!(content(&editor), "line2\nline1\nline3\n");
}

#[test]
fn test_copy_line() {
    let mut editor = TestEditor::with_content("line1\nline2\n");
    set_cursor(&mut editor, 0, 0);
    send_alt(&mut editor, '6'); // copy line
    // Content unchanged
    assert_eq!(content(&editor), "line1\nline2\n");
    set_cursor(&mut editor, 1, 0);
    send_ctrl(&mut editor, 'u'); // paste
    assert_eq!(content(&editor), "line1\nline1\nline2\n");
}

#[test]
fn test_paste_empty_clipboard() {
    let mut editor = TestEditor::with_content("hello\n");
    set_cursor(&mut editor, 0, 0);
    send_ctrl(&mut editor, 'u'); // paste with empty clipboard
    assert_eq!(content(&editor), "hello\n"); // no change
}

#[test]
fn test_multiple_cuts_accumulate() {
    let mut editor = TestEditor::with_content("a\nb\nc\n");
    set_cursor(&mut editor, 0, 0);
    send_ctrl(&mut editor, 'k'); // cut "a"
    send_ctrl(&mut editor, 'k'); // cut "b" (consecutive cuts accumulate in nano)
    // Now clipboard contains "a\nb\n"
    assert_eq!(content(&editor), "c\n");
    send_ctrl(&mut editor, 'u'); // paste
    assert_eq!(content(&editor), "a\nb\nc\n");
}

#[test]
fn test_cut_undo() {
    let mut editor = TestEditor::with_content("line1\nline2\n");
    set_cursor(&mut editor, 0, 0);
    send_ctrl(&mut editor, 'k');
    assert_eq!(content(&editor), "line2\n");
    send_ctrl(&mut editor, 'z'); // undo
    assert_eq!(content(&editor), "line1\nline2\n");
}
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cargo test test_clipboard`
Expected: Compilation errors — `send_alt` may not trigger cut/paste yet, and clipboard doesn't exist.

- [ ] **Step 3: Add clipboard state to Editor**

In `src/editor.rs`, add to the `Editor` struct:
```rust
    pub clipboard: Vec<String>,
    pub last_cut_line: Option<usize>, // track consecutive cuts for accumulation
```

Initialize in both `new()` and `new_for_test()`:
```rust
    clipboard: Vec::new(),
    last_cut_line: None,
```

- [ ] **Step 4: Implement cut_line, copy_line, paste**

Add these methods to `impl Editor` in `src/editor.rs`:

```rust
    /// Cut the current line (or append to clipboard if consecutive cut).
    pub fn cut_line(&mut self) {
        let line_idx = self.viewport.cursor_pos.0;
        if line_idx >= self.rope.len_lines() {
            return;
        }

        self.save_undo_state();

        // Get the line content including newline
        let line_start = self.rope.line_to_char(line_idx);
        let line_end = if line_idx + 1 < self.rope.len_lines() {
            self.rope.line_to_char(line_idx + 1)
        } else {
            self.rope.len_chars()
        };

        let line_text: String = self.rope.slice(line_start..line_end).chars().collect();

        // Accumulate if consecutive cut on adjacent line
        if self.last_cut_line == Some(line_idx) || self.last_cut_line == Some(line_idx + 1) {
            // Append to existing clipboard for consecutive cuts (nano behavior)
        } else {
            self.clipboard.clear();
        }
        self.clipboard.push(line_text);
        self.last_cut_line = Some(line_idx);

        // Remove the line from the document
        self.rope.remove(line_start..line_end);

        // Adjust cursor position
        let max_line = self.rope.len_lines().saturating_sub(1);
        if self.viewport.cursor_pos.0 > max_line {
            self.viewport.cursor_pos.0 = max_line;
        }
        self.viewport.cursor_pos.1 = 0;
        self.clamp_cursor_to_line();

        self.modified = true;
        self.mark_document_changed(line_idx);
    }

    /// Copy the current line to clipboard.
    pub fn copy_line(&mut self) {
        let line_idx = self.viewport.cursor_pos.0;
        if line_idx >= self.rope.len_lines() {
            return;
        }

        let line_start = self.rope.line_to_char(line_idx);
        let line_end = if line_idx + 1 < self.rope.len_lines() {
            self.rope.line_to_char(line_idx + 1)
        } else {
            self.rope.len_chars()
        };

        let line_text: String = self.rope.slice(line_start..line_end).chars().collect();
        self.clipboard.clear();
        self.clipboard.push(line_text);
        self.last_cut_line = None;

        self.set_temporary_status_message("Copied 1 line".to_string());
    }

    /// Paste clipboard contents at cursor position (inserts above current line).
    pub fn paste(&mut self) {
        if self.clipboard.is_empty() {
            return;
        }

        self.save_undo_state();

        let paste_text: String = self.clipboard.join("");
        let insert_pos = self.rope.line_to_char(self.viewport.cursor_pos.0);

        self.rope.insert(insert_pos, &paste_text);
        self.modified = true;

        // Move cursor to the start of the pasted text
        self.viewport.cursor_pos.1 = 0;
        self.mark_document_changed(self.viewport.cursor_pos.0);

        let lines_pasted = paste_text.matches('\n').count();
        self.set_temporary_status_message(format!("Pasted {} line(s)", lines_pasted.max(1)));
    }

    /// Reset cut accumulation tracking (called on any non-cut action).
    pub fn reset_cut_tracking(&mut self) {
        self.last_cut_line = None;
    }
```

- [ ] **Step 5: Wire keybindings in `src/input.rs`**

In `handle_normal`, add before the `// Navigation` comment:

```rust
        (KeyModifiers::CONTROL, KeyCode::Char('k')) => {
            editor.cut_line();
        }
        (KeyModifiers::CONTROL, KeyCode::Char('u')) => {
            editor.paste();
        }
        (KeyModifiers::ALT, KeyCode::Char('6')) => {
            editor.copy_line();
        }
```

Also add `editor.reset_cut_tracking();` at the top of `handle_normal` for any key that isn't Ctrl+K, or alternatively reset tracking inside every non-cut method. The simpler approach: add it at the start of `handle_normal` and then have `cut_line` re-set it:

Actually, the cleanest approach: call `editor.reset_cut_tracking()` at the start of `handle_normal`, before the match. The `cut_line` method will set `last_cut_line` after `reset_cut_tracking` runs, so consecutive cuts still work.

Add at the very beginning of `handle_normal`:
```rust
fn handle_normal(editor: &mut Editor, key: KeyEvent) -> Result<bool> {
    // Reset cut accumulation for any key that isn't Ctrl+K
    if !(key.modifiers == KeyModifiers::CONTROL && key.code == KeyCode::Char('k')) {
        editor.reset_cut_tracking();
    }

    match (key.modifiers, key.code) {
        // ... existing code
```

- [ ] **Step 6: Run tests**

Run: `cargo test test_clipboard`
Expected: All clipboard tests pass.

- [ ] **Step 7: Commit**

```bash
git add src/editor.rs src/input.rs tests/test_clipboard.rs
git commit -m "feat: implement cut line, copy line, paste (Ctrl+K, Alt+6, Ctrl+U)"
```

---

### Task 3: Selection / Mark Mode (Alt+A)

**Files:**
- Modify: `src/editor.rs` — add mark/selection state, selection-aware cut/copy
- Modify: `src/input.rs` — wire Alt+A, modify cut/copy to be selection-aware
- Modify: `src/ui.rs` — render selection highlighting
- Create: `tests/test_selection.rs`

- [ ] **Step 1: Write selection tests**

Create `tests/test_selection.rs`:
```rust
mod helpers;
use helpers::*;
use crossterm::event::KeyCode;

#[test]
fn test_toggle_mark() {
    let mut editor = TestEditor::with_content("hello\n");
    set_cursor(&mut editor, 0, 2);
    send_alt(&mut editor, 'a'); // start mark
    assert!(editor.mark_anchor.is_some());
    send_alt(&mut editor, 'a'); // toggle mark off
    assert!(editor.mark_anchor.is_none());
}

#[test]
fn test_cut_selection() {
    let mut editor = TestEditor::with_content("hello world\n");
    set_cursor(&mut editor, 0, 0);
    send_alt(&mut editor, 'a'); // start mark at col 0
    // Move cursor to col 5
    for _ in 0..5 {
        send_key(&mut editor, KeyCode::Right);
    }
    send_ctrl(&mut editor, 'k'); // cut selection "hello"
    assert_eq!(content(&editor), " world\n");
    assert!(editor.mark_anchor.is_none()); // mark cleared after cut
}

#[test]
fn test_copy_selection() {
    let mut editor = TestEditor::with_content("hello world\n");
    set_cursor(&mut editor, 0, 0);
    send_alt(&mut editor, 'a');
    for _ in 0..5 {
        send_key(&mut editor, KeyCode::Right);
    }
    send_alt(&mut editor, '6'); // copy selection
    assert_eq!(content(&editor), "hello world\n"); // unchanged
    set_cursor(&mut editor, 0, 11);
    send_ctrl(&mut editor, 'u'); // paste at end
    // "hello" should be pasted
}

#[test]
fn test_selection_across_lines() {
    let mut editor = TestEditor::with_content("hello\nworld\n");
    set_cursor(&mut editor, 0, 3);
    send_alt(&mut editor, 'a');
    send_key(&mut editor, KeyCode::Down); // select from col 3 line 0 to col 3 line 1
    send_ctrl(&mut editor, 'k'); // cut
    assert_eq!(content(&editor), "helrld\n"); // "lo\nwo" was cut
}

#[test]
fn test_mark_cleared_on_non_navigation() {
    let mut editor = TestEditor::with_content("hello\n");
    set_cursor(&mut editor, 0, 0);
    send_alt(&mut editor, 'a');
    type_string(&mut editor, "x"); // typing clears mark
    assert!(editor.mark_anchor.is_none());
}
```

- [ ] **Step 2: Add mark state to Editor**

In `src/editor.rs`, add to `Editor` struct:
```rust
    /// Mark anchor point for selection. When Some, text between anchor and cursor is selected.
    pub mark_anchor: Option<(usize, usize)>,
```

Initialize as `None` in both constructors.

- [ ] **Step 3: Implement selection methods**

Add to `impl Editor`:

```rust
    /// Toggle mark (start/stop selection).
    pub fn toggle_mark(&mut self) {
        if self.mark_anchor.is_some() {
            self.mark_anchor = None;
            self.set_temporary_status_message("Mark unset".to_string());
        } else {
            self.mark_anchor = Some(self.viewport.cursor_pos);
            self.set_temporary_status_message("Mark set".to_string());
        }
        self.needs_redraw = true;
    }

    /// Get the selection range as (start_char_idx, end_char_idx) where start < end.
    /// Returns None if no selection is active.
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

    /// Cut the selection (or the current line if no selection).
    pub fn cut(&mut self) {
        if let Some((start, end)) = self.get_selection_range() {
            if start == end {
                self.cut_line();
                return;
            }
            self.save_undo_state();
            let selected: String = self.rope.slice(start..end).chars().collect();
            self.clipboard = vec![selected];
            self.last_cut_line = None;
            self.rope.remove(start..end);

            // Position cursor at selection start
            let line = self.rope.char_to_line(start.min(self.rope.len_chars().saturating_sub(1)));
            let line_start = self.rope.line_to_char(line);
            let col_chars = start.saturating_sub(line_start);
            let mut display_col = 0;
            for (i, ch) in self.rope.line(line).chars().enumerate() {
                if i >= col_chars || ch == '\n' { break; }
                display_col += UnicodeWidthChar::width(ch).unwrap_or(0);
            }
            self.viewport.cursor_pos = (line, display_col);

            self.mark_anchor = None;
            self.modified = true;
            self.mark_document_changed(line);
        } else {
            self.cut_line();
        }
    }

    /// Copy the selection (or the current line if no selection).
    pub fn copy(&mut self) {
        if let Some((start, end)) = self.get_selection_range() {
            if start == end {
                self.copy_line();
                return;
            }
            let selected: String = self.rope.slice(start..end).chars().collect();
            self.clipboard = vec![selected];
            self.last_cut_line = None;
            self.mark_anchor = None;
            self.set_temporary_status_message("Copied selection".to_string());
        } else {
            self.copy_line();
        }
    }

    /// Paste clipboard at current cursor position (inline, not above line).
    pub fn paste_inline(&mut self) {
        if self.clipboard.is_empty() {
            return;
        }

        self.save_undo_state();
        self.mark_anchor = None;

        let paste_text: String = self.clipboard.join("");
        let pos = self.line_col_to_char_idx(self.viewport.cursor_pos.0, self.viewport.cursor_pos.1);

        self.rope.insert(pos, &paste_text);
        self.modified = true;

        // Move cursor to end of pasted text
        let end_pos = pos + paste_text.chars().count();
        let line = self.rope.char_to_line(end_pos.min(self.rope.len_chars().saturating_sub(1)));
        let line_start = self.rope.line_to_char(line);
        let col_chars = end_pos.saturating_sub(line_start);
        let mut display_col = 0;
        for (i, ch) in self.rope.line(line).chars().enumerate() {
            if i >= col_chars || ch == '\n' { break; }
            display_col += UnicodeWidthChar::width(ch).unwrap_or(0);
        }
        self.viewport.cursor_pos = (line, display_col);
        self.mark_document_changed(self.viewport.cursor_pos.0);
    }
```

- [ ] **Step 4: Update cut/copy/paste keybindings to use selection-aware versions**

In `src/input.rs` `handle_normal`, change the clipboard bindings:

```rust
        (KeyModifiers::CONTROL, KeyCode::Char('k')) => {
            editor.cut();
        }
        (KeyModifiers::CONTROL, KeyCode::Char('u')) => {
            editor.paste_inline();
        }
        (KeyModifiers::ALT, KeyCode::Char('6')) => {
            editor.copy();
        }
        (KeyModifiers::ALT, KeyCode::Char('a')) => {
            editor.toggle_mark();
        }
```

Add mark clearing on any editing operation — add at the start of `insert_char`, `delete_char`, `insert_newline` in `src/editor.rs`:
```rust
    self.mark_anchor = None;
```

- [ ] **Step 5: Add selection highlighting in `src/ui.rs`**

In the text rendering section of `draw_ui`, after computing `styled_spans` for a line, add selection highlighting. Before pushing to `lines`, wrap the spans with selection highlighting if a selection is active. This requires knowing which character positions on the current line are within the selection range.

Add a helper function to `ui.rs`:
```rust
fn apply_selection_highlighting(
    spans: Vec<Span<'static>>,
    line_idx: usize,
    editor: &Editor,
) -> Vec<Span<'static>> {
    let (anchor, cursor) = match editor.mark_anchor {
        Some(anchor) => (anchor, editor.viewport.cursor_pos),
        None => return spans,
    };

    // Convert anchor and cursor to char indices
    let anchor_idx = editor.line_col_to_char_idx(anchor.0, anchor.1);
    let cursor_idx = editor.line_col_to_char_idx(cursor.0, cursor.1);
    let (sel_start, sel_end) = if anchor_idx <= cursor_idx {
        (anchor_idx, cursor_idx)
    } else {
        (cursor_idx, anchor_idx)
    };

    let line_start_char = editor.rope.line_to_char(line_idx);
    let line_end_char = if line_idx + 1 < editor.rope.len_lines() {
        editor.rope.line_to_char(line_idx + 1)
    } else {
        editor.rope.len_chars()
    };

    // Check if this line intersects the selection
    if sel_end <= line_start_char || sel_start >= line_end_char {
        return spans; // No overlap
    }

    // For simplicity, if any part of the line is selected, apply reverse video to those characters
    let sel_start_in_line = sel_start.saturating_sub(line_start_char);
    let sel_end_in_line = (sel_end - line_start_char).min(line_end_char - line_start_char);

    let mut result = Vec::new();
    let mut char_pos = 0;
    for span in spans {
        let span_len = span.content.chars().count();
        let span_end = char_pos + span_len;

        if span_end <= sel_start_in_line || char_pos >= sel_end_in_line {
            // Entirely outside selection
            result.push(span);
        } else if char_pos >= sel_start_in_line && span_end <= sel_end_in_line {
            // Entirely inside selection
            result.push(Span::styled(
                span.content.to_string(),
                span.style.bg(Color::White).fg(Color::Black),
            ));
        } else {
            // Partially inside — split the span
            let chars: Vec<char> = span.content.chars().collect();
            let mut i = 0;
            while i < chars.len() {
                let abs_pos = char_pos + i;
                let in_sel = abs_pos >= sel_start_in_line && abs_pos < sel_end_in_line;
                let start_i = i;
                while i < chars.len() {
                    let p = char_pos + i;
                    let p_in_sel = p >= sel_start_in_line && p < sel_end_in_line;
                    if p_in_sel != in_sel { break; }
                    i += 1;
                }
                let text: String = chars[start_i..i].iter().collect();
                let style = if in_sel {
                    span.style.bg(Color::White).fg(Color::Black)
                } else {
                    span.style
                };
                result.push(Span::styled(text, style));
            }
        }
        char_pos = span_end;
    }
    result
}
```

Call this after `apply_search_highlighting` in the rendering loop:
```rust
let final_spans = apply_selection_highlighting(styled_spans, line_idx, editor);
lines.push(Line::from(final_spans));
```

- [ ] **Step 6: Run tests**

Run: `cargo test`
Expected: All tests pass (clipboard + selection + existing smoke tests).

- [ ] **Step 7: Commit**

```bash
git add src/editor.rs src/input.rs src/ui.rs tests/test_selection.rs
git commit -m "feat: implement mark/selection mode with selection-aware cut/copy/paste"
```

---

### Task 4: Delete Forward + Auto-Indent

**Files:**
- Modify: `src/editor.rs` — add `delete_char_forward`, `insert_newline_with_indent`
- Modify: `src/input.rs` — wire Delete key, modify Enter behavior
- Modify: `src/config.rs` — add `auto_indent` config option
- Create: `tests/test_editing.rs` (add more tests)

- [ ] **Step 1: Write tests**

Add to `tests/test_editing.rs`:
```rust
#[test]
fn test_delete_forward() {
    let mut editor = TestEditor::with_content("hello\n");
    set_cursor(&mut editor, 0, 0);
    send_key(&mut editor, KeyCode::Delete);
    assert_eq!(content(&editor), "ello\n");
    assert_eq!(cursor(&editor), (0, 0));
}

#[test]
fn test_delete_forward_joins_lines() {
    let mut editor = TestEditor::with_content("hello\nworld\n");
    set_cursor(&mut editor, 0, 5);
    send_key(&mut editor, KeyCode::Delete); // delete the \n
    assert_eq!(content(&editor), "helloworld\n");
}

#[test]
fn test_delete_forward_at_end_of_file() {
    let mut editor = TestEditor::with_content("hello");
    set_cursor(&mut editor, 0, 5);
    send_key(&mut editor, KeyCode::Delete); // should do nothing
    assert_eq!(content(&editor), "hello");
}

#[test]
fn test_auto_indent() {
    let mut editor = TestEditor::with_content("    hello\n");
    editor.config.auto_indent = true;
    set_cursor(&mut editor, 0, 9); // end of "    hello"
    send_key(&mut editor, KeyCode::Enter);
    // New line should have same indentation
    assert_eq!(content(&editor), "    hello\n    \n");
    assert_eq!(cursor(&editor), (1, 4));
}

#[test]
fn test_auto_indent_disabled() {
    let mut editor = TestEditor::with_content("    hello\n");
    editor.config.auto_indent = false;
    set_cursor(&mut editor, 0, 9);
    send_key(&mut editor, KeyCode::Enter);
    assert_eq!(content(&editor), "    hello\n\n");
    assert_eq!(cursor(&editor), (1, 0));
}
```

- [ ] **Step 2: Add `auto_indent` to Config**

In `src/config.rs`:
```rust
pub struct Config {
    pub mouse_enabled: bool,
    pub show_line_numbers: bool,
    pub tab_width: usize,
    pub word_wrap: bool,
    pub auto_indent: bool,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            mouse_enabled: true,
            show_line_numbers: false,
            tab_width: super::constants::DEFAULT_TAB_WIDTH,
            word_wrap: false,
            auto_indent: true, // on by default like nano
        }
    }
}
```

- [ ] **Step 3: Implement delete_char_forward**

In `src/editor.rs`:
```rust
    pub fn delete_char_forward(&mut self) {
        let pos = self.line_col_to_char_idx(self.viewport.cursor_pos.0, self.viewport.cursor_pos.1);
        if pos < self.rope.len_chars() {
            self.save_undo_state();
            self.rope.remove(pos..pos + 1);
            self.mark_document_changed(self.viewport.cursor_pos.0);
            self.modified = true;
        }
    }
```

- [ ] **Step 4: Update insert_newline for auto-indent**

Replace the existing `insert_newline` in `src/editor.rs`:
```rust
    pub fn insert_newline(&mut self) {
        self.mark_anchor = None;
        self.save_undo_state();
        let pos = self.line_col_to_char_idx(self.viewport.cursor_pos.0, self.viewport.cursor_pos.1);

        // Compute indentation from current line if auto_indent is on
        let indent = if self.config.auto_indent {
            let rope_line = self.rope.line(self.viewport.cursor_pos.0);
            let mut indent_str = String::new();
            for ch in rope_line.chars() {
                if ch == ' ' || ch == '\t' {
                    indent_str.push(ch);
                } else {
                    break;
                }
            }
            indent_str
        } else {
            String::new()
        };

        let insert_str = format!("\n{}", indent);
        self.rope.insert(pos, &insert_str);
        self.mark_document_changed(self.viewport.cursor_pos.0);
        self.viewport.cursor_pos.0 += 1;
        self.viewport.cursor_pos.1 = indent.width();
        self.modified = true;
    }
```

- [ ] **Step 5: Wire Delete key in `src/input.rs`**

In `handle_normal`, add after the Backspace handler:
```rust
        (_, KeyCode::Delete) => editor.delete_char_forward(),
```

- [ ] **Step 6: Add auto-indent toggle to options menu**

In `handle_options_menu` in `src/input.rs`, add a new option:
```rust
        KeyCode::Char('i') | KeyCode::Char('I') => {
            editor.config.auto_indent = !editor.config.auto_indent;
            editor.set_temporary_status_message(format!(
                "Auto-indent: {}",
                if editor.config.auto_indent { "ON" } else { "OFF" }
            ));
            editor.input_mode = InputMode::Normal;
            editor.needs_redraw = true;
        }
```

Update the options help bar in `src/ui.rs` to include `I: Auto-indent`:
```rust
        InputMode::OptionsMenu => (
            "M: Mouse  L: Line Nums  W: Wrap  T: Tab  I: Indent  Esc: Back".to_string(),
            String::new(),
        ),
```

- [ ] **Step 7: Run tests and commit**

Run: `cargo test`
Expected: All tests pass.

```bash
git add src/editor.rs src/input.rs src/config.rs src/ui.rs tests/test_editing.rs
git commit -m "feat: add delete forward, auto-indent on Enter"
```

---

### Task 5: Word Jump, Start/End of File, Bracket Matching, Cursor Position Info

**Files:**
- Modify: `src/editor.rs` — add word_left, word_right, goto_start, goto_end, match_bracket, cursor_info
- Modify: `src/input.rs` — wire Ctrl+Left/Right, Ctrl+Home/End, Alt+], Ctrl+C
- Create: `tests/test_navigation.rs`

- [ ] **Step 1: Write tests**

Create `tests/test_navigation.rs`:
```rust
mod helpers;
use helpers::*;
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

#[test]
fn test_word_right() {
    let mut editor = TestEditor::with_content("hello world foo\n");
    set_cursor(&mut editor, 0, 0);
    // Ctrl+Right jumps to next word
    let key = KeyEvent::new(KeyCode::Right, KeyModifiers::CONTROL);
    let _ = handle_key_event(&mut editor, key);
    assert_eq!(cursor(&editor).1, 6); // start of "world"
}

#[test]
fn test_word_left() {
    let mut editor = TestEditor::with_content("hello world\n");
    set_cursor(&mut editor, 0, 8);
    let key = KeyEvent::new(KeyCode::Left, KeyModifiers::CONTROL);
    let _ = handle_key_event(&mut editor, key);
    assert_eq!(cursor(&editor).1, 6); // start of "world"
}

#[test]
fn test_goto_start_of_file() {
    let mut editor = TestEditor::with_content("line1\nline2\nline3\n");
    set_cursor(&mut editor, 2, 3);
    let key = KeyEvent::new(KeyCode::Home, KeyModifiers::CONTROL);
    let _ = handle_key_event(&mut editor, key);
    assert_eq!(cursor(&editor), (0, 0));
}

#[test]
fn test_goto_end_of_file() {
    let mut editor = TestEditor::with_content("line1\nline2\nline3\n");
    set_cursor(&mut editor, 0, 0);
    let key = KeyEvent::new(KeyCode::End, KeyModifiers::CONTROL);
    let _ = handle_key_event(&mut editor, key);
    assert_eq!(cursor(&editor).0, 2); // last line with content
}

#[test]
fn test_bracket_matching() {
    let mut editor = TestEditor::with_content("fn main() {\n    println!(\"hi\");\n}\n");
    set_cursor(&mut editor, 0, 11); // on the '{'
    send_alt(&mut editor, ']');
    assert_eq!(cursor(&editor), (2, 0)); // matching '}'
}

#[test]
fn test_bracket_matching_reverse() {
    let mut editor = TestEditor::with_content("fn main() {\n    println!(\"hi\");\n}\n");
    set_cursor(&mut editor, 2, 0); // on the '}'
    send_alt(&mut editor, ']');
    assert_eq!(cursor(&editor), (0, 11)); // matching '{'
}

#[test]
fn test_cursor_position_info() {
    let mut editor = TestEditor::with_content("hello\nworld\n");
    set_cursor(&mut editor, 1, 3);
    send_ctrl(&mut editor, 'c');
    assert!(editor.status_message.contains("Line: 2"));
    assert!(editor.status_message.contains("Col: 4"));
}
```

- [ ] **Step 2: Implement word navigation**

In `src/editor.rs`:
```rust
    /// Move cursor to the start of the next word.
    pub fn move_word_right(&mut self) {
        let line_idx = self.viewport.cursor_pos.0;
        let rope_line = self.rope.line(line_idx);
        let line_chars: Vec<char> = rope_line.chars().filter(|&c| c != '\n').collect();
        let mut col = self.viewport.cursor_pos.1;

        // Skip current word characters
        while col < line_chars.len() && !line_chars[col].is_whitespace() {
            col += 1;
        }
        // Skip whitespace
        while col < line_chars.len() && line_chars[col].is_whitespace() {
            col += 1;
        }

        if col >= line_chars.len() && line_idx < self.rope.len_lines().saturating_sub(1) {
            // Wrap to next line
            self.viewport.cursor_pos.0 += 1;
            self.viewport.cursor_pos.1 = 0;
        } else {
            self.viewport.cursor_pos.1 = col;
        }
        self.needs_redraw = true;
    }

    /// Move cursor to the start of the previous word.
    pub fn move_word_left(&mut self) {
        let line_idx = self.viewport.cursor_pos.0;
        let rope_line = self.rope.line(line_idx);
        let line_chars: Vec<char> = rope_line.chars().filter(|&c| c != '\n').collect();
        let mut col = self.viewport.cursor_pos.1;

        if col == 0 {
            if line_idx > 0 {
                self.viewport.cursor_pos.0 -= 1;
                self.viewport.cursor_pos.1 =
                    line_display_width(&self.rope, self.viewport.cursor_pos.0);
            }
            self.needs_redraw = true;
            return;
        }

        // Move back past whitespace
        while col > 0 && line_chars.get(col.saturating_sub(1)).map_or(false, |c| c.is_whitespace()) {
            col -= 1;
        }
        // Move back past word characters
        while col > 0 && line_chars.get(col.saturating_sub(1)).map_or(false, |c| !c.is_whitespace()) {
            col -= 1;
        }

        self.viewport.cursor_pos.1 = col;
        self.needs_redraw = true;
    }

    /// Jump to start of file.
    pub fn goto_start(&mut self) {
        self.viewport.cursor_pos = (0, 0);
        self.needs_redraw = true;
    }

    /// Jump to end of file.
    pub fn goto_end(&mut self) {
        let last_line = self.rope.len_lines().saturating_sub(1);
        self.viewport.cursor_pos.0 = last_line;
        self.viewport.cursor_pos.1 = line_display_width(&self.rope, last_line);
        self.needs_redraw = true;
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
                if c == ch { depth += 1; }
                if c == target { depth -= 1; }
                if depth == 0 {
                    let line = self.rope.char_to_line(i);
                    let line_start = self.rope.line_to_char(line);
                    let col_chars = i - line_start;
                    let mut display_col = 0;
                    for (j, jch) in self.rope.line(line).chars().enumerate() {
                        if j >= col_chars { break; }
                        display_col += UnicodeWidthChar::width(jch).unwrap_or(0);
                    }
                    self.viewport.cursor_pos = (line, display_col);
                    self.needs_redraw = true;
                    return;
                }
            }
        } else {
            let mut i = pos;
            while i > 0 {
                i -= 1;
                let c = self.rope.char(i);
                if c == ch { depth += 1; }
                if c == target { depth -= 1; }
                if depth == 0 {
                    let line = self.rope.char_to_line(i);
                    let line_start = self.rope.line_to_char(line);
                    let col_chars = i - line_start;
                    let mut display_col = 0;
                    for (j, jch) in self.rope.line(line).chars().enumerate() {
                        if j >= col_chars { break; }
                        display_col += UnicodeWidthChar::width(jch).unwrap_or(0);
                    }
                    self.viewport.cursor_pos = (line, display_col);
                    self.needs_redraw = true;
                    return;
                }
            }
        }
    }

    /// Show cursor position information.
    pub fn show_cursor_info(&mut self) {
        let line = self.viewport.cursor_pos.0 + 1;
        let col = self.viewport.cursor_pos.1 + 1;
        let total_lines = self.rope.len_lines();
        let total_chars = self.rope.len_chars();
        let char_idx = self.line_col_to_char_idx(self.viewport.cursor_pos.0, self.viewport.cursor_pos.1);
        self.set_temporary_status_message(format!(
            "Line: {}/{} | Col: {} | Char: {}/{}", line, total_lines, col, char_idx + 1, total_chars
        ));
    }
```

- [ ] **Step 3: Wire keybindings**

In `handle_normal` in `src/input.rs`, update navigation section:

```rust
        // Navigation
        (KeyModifiers::CONTROL, KeyCode::Home) => editor.goto_start(),
        (KeyModifiers::CONTROL, KeyCode::End) => editor.goto_end(),
        (KeyModifiers::CONTROL, KeyCode::Left) => editor.move_word_left(),
        (KeyModifiers::CONTROL, KeyCode::Right) => editor.move_word_right(),
        (KeyModifiers::ALT, KeyCode::Char(']')) => editor.match_bracket(),
        (KeyModifiers::CONTROL, KeyCode::Char('c')) => editor.show_cursor_info(),
        (_, KeyCode::Up) => editor.move_cursor_up(),
        // ... rest unchanged
```

Note: Ctrl+C is currently unbound, so this is safe.

- [ ] **Step 4: Run tests and commit**

Run: `cargo test`
Expected: All tests pass.

```bash
git add src/editor.rs src/input.rs tests/test_navigation.rs
git commit -m "feat: add word jump, start/end of file, bracket matching, cursor info"
```

---

### Task 6: Indent/Unindent Block + Comment Toggle

**Files:**
- Modify: `src/editor.rs` — add indent_selection, unindent_selection, toggle_comment
- Modify: `src/input.rs` — wire Alt+}, Alt+{, Alt+;

- [ ] **Step 1: Write tests**

Add to `tests/test_editing.rs`:
```rust
#[test]
fn test_indent_selection() {
    let mut editor = TestEditor::with_content("hello\nworld\n");
    set_cursor(&mut editor, 0, 0);
    send_alt(&mut editor, 'a'); // start mark
    send_key(&mut editor, KeyCode::Down); // select both lines
    send_alt(&mut editor, '}'); // indent
    assert_eq!(content(&editor), "    hello\n    world\n");
}

#[test]
fn test_unindent_selection() {
    let mut editor = TestEditor::with_content("    hello\n    world\n");
    set_cursor(&mut editor, 0, 0);
    send_alt(&mut editor, 'a');
    send_key(&mut editor, KeyCode::Down);
    send_alt(&mut editor, '{'); // unindent
    assert_eq!(content(&editor), "hello\nworld\n");
}

#[test]
fn test_comment_toggle() {
    let mut editor = TestEditor::with_content("hello\nworld\n");
    editor.syntax_name = Some("Rust".to_string());
    set_cursor(&mut editor, 0, 0);
    send_alt(&mut editor, ';'); // comment single line
    assert_eq!(content(&editor), "// hello\nworld\n");
    send_alt(&mut editor, ';'); // uncomment
    assert_eq!(content(&editor), "hello\nworld\n");
}

#[test]
fn test_comment_toggle_python() {
    let mut editor = TestEditor::with_content("hello\n");
    editor.syntax_name = Some("Python".to_string());
    set_cursor(&mut editor, 0, 0);
    send_alt(&mut editor, ';');
    assert_eq!(content(&editor), "# hello\n");
}
```

- [ ] **Step 2: Implement indent/unindent/comment**

In `src/editor.rs`:
```rust
    /// Indent selected lines (or current line if no selection) by tab_width spaces.
    pub fn indent_lines(&mut self) {
        let (start_line, end_line) = self.get_affected_lines();
        self.save_undo_state();

        let indent: String = " ".repeat(self.config.tab_width);

        for line_idx in (start_line..=end_line).rev() {
            if line_idx < self.rope.len_lines() {
                let line_start = self.rope.line_to_char(line_idx);
                self.rope.insert(line_start, &indent);
            }
        }

        self.mark_anchor = None;
        self.modified = true;
        self.mark_document_changed(start_line);
        self.set_temporary_status_message(format!("Indented {} line(s)", end_line - start_line + 1));
    }

    /// Unindent selected lines (or current line) by up to tab_width spaces.
    pub fn unindent_lines(&mut self) {
        let (start_line, end_line) = self.get_affected_lines();
        self.save_undo_state();

        for line_idx in (start_line..=end_line).rev() {
            if line_idx < self.rope.len_lines() {
                let line_start = self.rope.line_to_char(line_idx);
                let mut spaces_to_remove = 0;
                for ch in self.rope.line(line_idx).chars() {
                    if ch == ' ' && spaces_to_remove < self.config.tab_width {
                        spaces_to_remove += 1;
                    } else if ch == '\t' && spaces_to_remove == 0 {
                        spaces_to_remove = 1;
                        break;
                    } else {
                        break;
                    }
                }
                if spaces_to_remove > 0 {
                    self.rope.remove(line_start..line_start + spaces_to_remove);
                }
            }
        }

        self.mark_anchor = None;
        self.modified = true;
        self.clamp_cursor_to_line();
        self.mark_document_changed(start_line);
    }

    /// Toggle line comment on selected lines (or current line).
    pub fn toggle_comment(&mut self) {
        let comment_str = match self.syntax_name.as_deref() {
            Some("Rust") | Some("C") | Some("C++") | Some("Go") | Some("JavaScript")
            | Some("TypeScript") | Some("Java") | Some("Swift") | Some("Kotlin")
            | Some("Zig") => "// ",
            Some("Python") | Some("Ruby") | Some("Shell Script (Bash)") | Some("Perl")
            | Some("R") | Some("YAML") | Some("TOML") => "# ",
            Some("Lua") | Some("SQL") => "-- ",
            Some("HTML") | Some("XML") | Some("CSS") => return, // block comment languages, skip for now
            _ => "// ", // default
        };

        let (start_line, end_line) = self.get_affected_lines();
        self.save_undo_state();

        // Check if all lines are already commented
        let all_commented = (start_line..=end_line).all(|line_idx| {
            if line_idx < self.rope.len_lines() {
                let rope_line = self.rope.line(line_idx);
                let trimmed: String = rope_line.chars().collect::<String>();
                let trimmed = trimmed.trim_start();
                trimmed.starts_with(comment_str.trim_end())
            } else {
                true
            }
        });

        if all_commented {
            // Uncomment
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
                        self.rope.remove(line_start + pos..line_start + pos + remove_len);
                    }
                }
            }
        } else {
            // Comment
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

    /// Get the range of lines affected by the current selection, or just the cursor line.
    fn get_affected_lines(&self) -> (usize, usize) {
        if let Some(anchor) = self.mark_anchor {
            let start = anchor.0.min(self.viewport.cursor_pos.0);
            let end = anchor.0.max(self.viewport.cursor_pos.0);
            (start, end)
        } else {
            (self.viewport.cursor_pos.0, self.viewport.cursor_pos.0)
        }
    }
```

- [ ] **Step 3: Wire keybindings**

In `handle_normal` in `src/input.rs`:
```rust
        (KeyModifiers::ALT, KeyCode::Char('}')) => {
            editor.indent_lines();
        }
        (KeyModifiers::ALT, KeyCode::Char('{')) => {
            editor.unindent_lines();
        }
        (KeyModifiers::ALT, KeyCode::Char(';')) => {
            editor.toggle_comment();
        }
```

- [ ] **Step 4: Run tests and commit**

Run: `cargo test`
Expected: All pass.

```bash
git add src/editor.rs src/input.rs tests/test_editing.rs
git commit -m "feat: add indent/unindent block and comment toggle"
```

---

### Task 7: Working Regex Search

**Files:**
- Modify: `src/search.rs` — implement regex matching in `find_all_matches`
- Create: `tests/test_search.rs`

- [ ] **Step 1: Write tests**

Create `tests/test_search.rs`:
```rust
mod helpers;
use helpers::*;

#[test]
fn test_regex_search() {
    let mut editor = TestEditor::with_content("hello123 world456\n");
    editor.search.use_regex = true;
    editor.search.search_buffer = r"\d+".to_string();
    let matches = editor.search.find_all_matches(&editor.rope);
    assert_eq!(matches.len(), 2);
}

#[test]
fn test_regex_search_case_insensitive() {
    let mut editor = TestEditor::with_content("Hello HELLO hello\n");
    editor.search.use_regex = true;
    editor.search.case_sensitive = false;
    editor.search.search_buffer = "hello".to_string();
    let matches = editor.search.find_all_matches(&editor.rope);
    assert_eq!(matches.len(), 3);
}

#[test]
fn test_literal_search_unchanged() {
    let mut editor = TestEditor::with_content("hello world\n");
    editor.search.use_regex = false;
    editor.search.search_buffer = "world".to_string();
    let matches = editor.search.find_all_matches(&editor.rope);
    assert_eq!(matches.len(), 1);
    assert_eq!(matches[0], (0, 6));
}

#[test]
fn test_invalid_regex_no_crash() {
    let mut editor = TestEditor::with_content("hello\n");
    editor.search.use_regex = true;
    editor.search.search_buffer = "[invalid".to_string();
    let matches = editor.search.find_all_matches(&editor.rope);
    assert_eq!(matches.len(), 0); // Invalid regex should return no matches
}
```

- [ ] **Step 2: Implement regex search**

In `src/search.rs`, update `find_all_matches` to use the `regex` crate when `use_regex` is true:

Add at the top of `src/search.rs`:
```rust
use regex::Regex;
```

Update `find_all_matches`:
```rust
    pub fn find_all_matches(&mut self, rope: &ropey::Rope) -> Vec<(usize, usize)> {
        let search_term = &self.search_buffer;
        if search_term.is_empty() {
            return Vec::new();
        }

        if self.use_regex {
            return self.find_all_regex_matches(rope);
        }

        // ... existing literal search code unchanged ...
    }

    fn find_all_regex_matches(&self, rope: &ropey::Rope) -> Vec<(usize, usize)> {
        let pattern = if self.case_sensitive {
            Regex::new(&self.search_buffer)
        } else {
            Regex::new(&format!("(?i){}", &self.search_buffer))
        };

        let re = match pattern {
            Ok(re) => re,
            Err(_) => return Vec::new(), // invalid regex
        };

        let mut matches = Vec::new();

        for line_idx in 0..rope.len_lines() {
            let rope_line = rope.line(line_idx);
            let owned_line: String;
            let line_str = match rope_line.as_str() {
                Some(s) => s,
                None => {
                    owned_line = rope_line.chars().collect::<String>();
                    &owned_line
                }
            };
            let line_content = line_str.trim_end_matches('\n');

            for m in re.find_iter(line_content) {
                // Convert byte offset to char offset
                let char_pos = line_content[..m.start()].chars().count();
                matches.push((line_idx, char_pos));
            }
        }

        matches.sort_by(|a, b| a.0.cmp(&b.0).then(a.1.cmp(&b.1)));
        matches
    }
```

- [ ] **Step 3: Run tests and commit**

Run: `cargo test test_search`
Expected: All pass.

```bash
git add src/search.rs tests/test_search.rs
git commit -m "feat: implement working regex search"
```

---

### Task 8: Whitespace Display + Constant Cursor Position

**Files:**
- Modify: `src/config.rs` — add `show_whitespace` option
- Modify: `src/editor.rs` — add toggle methods
- Modify: `src/input.rs` — wire Alt+P for whitespace toggle
- Modify: `src/ui.rs` — render whitespace indicators, optional constant cursor position in status bar

- [ ] **Step 1: Add config options**

In `src/config.rs`, add to `Config`:
```rust
    pub show_whitespace: bool,
    pub constant_cursor_position: bool,
```

With defaults:
```rust
    show_whitespace: false,
    constant_cursor_position: false,
```

- [ ] **Step 2: Add whitespace rendering in `src/ui.rs`**

In the rendering loop, after computing `line_content`, if `editor.config.show_whitespace` is true, replace spaces with `·` and tabs with `→` before applying syntax highlighting. Add a helper function:

```rust
fn render_whitespace(text: &str) -> String {
    text.replace(' ', "\u{00B7}").replace('\t', "\u{2192}")
}
```

Apply this transformation on the display text when `show_whitespace` is enabled. The actual rope content stays unchanged — only the rendering changes.

In the rendering loop where `line_content` is used for display, apply the transformation:
```rust
let display_content = if editor.config.show_whitespace {
    render_whitespace(line_content)
} else {
    line_content.to_string()
};
```

Use `display_content` for rendering spans but keep `line_content` for search highlighting logic.

- [ ] **Step 3: Add constant cursor position to status bar**

In `draw_ui` in `src/ui.rs`, when `editor.config.constant_cursor_position` is true and there's no active status message, always show cursor position. This is already partially done — the status bar shows `Ln X, Col Y`. Make it always visible when `constant_cursor_position` is on, even over temporary messages.

Actually, the status bar already shows cursor position. The `constant_cursor_position` option just means it's always visible (even when temporary messages would normally cover it). For simplicity, when this option is on, append the position to any status message:

In the status bar section of `draw_ui`, at the very end after computing `status_text`, if `constant_cursor_position` is on:
```rust
    let status_text = if editor.config.constant_cursor_position && !status_text.is_empty()
        && editor.input_mode == InputMode::Normal
    {
        format!("{} | Ln {}, Col {}",
            status_text,
            editor.viewport.cursor_pos.0 + 1,
            editor.viewport.cursor_pos.1 + 1
        )
    } else {
        status_text
    };
```

- [ ] **Step 4: Wire keybindings**

In `handle_normal` in `src/input.rs`:
```rust
        (KeyModifiers::ALT, KeyCode::Char('p')) => {
            editor.config.show_whitespace = !editor.config.show_whitespace;
            editor.set_temporary_status_message(format!(
                "Whitespace display: {}",
                if editor.config.show_whitespace { "ON" } else { "OFF" }
            ));
            editor.needs_redraw = true;
        }
```

Add whitespace toggle to options menu too:
```rust
        KeyCode::Char('p') | KeyCode::Char('P') => {
            editor.config.show_whitespace = !editor.config.show_whitespace;
            editor.set_temporary_status_message(format!(
                "Whitespace: {}",
                if editor.config.show_whitespace { "ON" } else { "OFF" }
            ));
            editor.input_mode = InputMode::Normal;
            editor.needs_redraw = true;
        }
```

- [ ] **Step 5: Run tests and commit**

Run: `cargo test && cargo clippy`
Expected: All pass.

```bash
git add src/config.rs src/editor.rs src/input.rs src/ui.rs
git commit -m "feat: add whitespace display toggle and constant cursor position"
```

---

### Task 9: Help Modal Update + Final Polish

**Files:**
- Modify: `src/ui.rs` — update help modal content with all new keybindings

- [ ] **Step 1: Update help modal**

Replace the help modal content string in `draw_help_modal` in `src/ui.rs` with:

```rust
    let help_content = r#"
 __________ ____ _____________________________
 \______   \    |   \      \   \_   _____/
  |       _/    |   /   |   \   |    __)_
  |    |   \    |  /    |    \  |        \
  |____|_  /______/\____|__  / /_______  /
         \/                \/          \/

         A nano-inspired text editor

─────────────────────────────────────────
               FILE OPERATIONS
─────────────────────────────────────────
^Q / ^X  Quit editor
^S       Save file
^W       Save as (write file)
^O       Options menu

─────────────────────────────────────────
                 EDITING
─────────────────────────────────────────
^Z       Undo
^R       Redo
^K       Cut line/selection
^U       Paste
M-6      Copy line/selection
M-A      Toggle mark (selection)
M-}      Indent selection
M-{      Unindent selection
M-;      Toggle comment
Delete   Delete forward

─────────────────────────────────────────
               NAVIGATION
─────────────────────────────────────────
^F       Find text
^\       Replace text
^G       Go to line
^C       Cursor position info
^V       Page down
^Y       Page up
^Home    Start of file
^End     End of file
^Left    Previous word
^Right   Next word
M-]      Match bracket
Arrows   Move cursor

─────────────────────────────────────────
                  VIEW
─────────────────────────────────────────
^B       Hex view (live buffer)
M-P      Toggle whitespace display

─────────────────────────────────────────
                OPTIONS
─────────────────────────────────────────
^O       Open options menu
  M      Toggle mouse mode
  L      Toggle line numbers
  W      Toggle word wrap
  T      Set tab width
  I      Toggle auto-indent
  P      Toggle whitespace

─────────────────────────────────────────
          Press ^H or Esc to close
─────────────────────────────────────────"#;
```

Note: `M-` prefix means Alt+ (Meta).

- [ ] **Step 2: Run full test suite and clippy**

Run: `cargo test && cargo clippy`
Expected: All pass, zero warnings.

- [ ] **Step 3: Manual verification**

Test in a real terminal:
1. Open a file → Ctrl+K cuts line → Ctrl+U pastes it
2. Alt+A starts selection → arrow keys extend → Ctrl+K cuts selection
3. Ctrl+Left/Right jumps words
4. Ctrl+Home/End jumps to file start/end
5. Alt+] matches brackets
6. Alt+} indents, Alt+{ unindents
7. Alt+; comments/uncomments
8. Delete key deletes forward
9. Enter auto-indents
10. Alt+P shows whitespace
11. Ctrl+C shows cursor position
12. Ctrl+F then type regex with Alt+R → regex works
13. Ctrl+H shows updated help modal

- [ ] **Step 4: Commit**

```bash
git add src/ui.rs
git commit -m "feat: update help modal with all new keybindings"
```

---

## Self-Review Checklist

**Spec coverage:**
- [x] Cut line (Ctrl+K) — Task 2
- [x] Paste (Ctrl+U) — Task 2
- [x] Copy line (Alt+6) — Task 2
- [x] Mark/selection (Alt+A) — Task 3
- [x] Selection-aware cut/copy — Task 3
- [x] Selection highlighting — Task 3
- [x] Delete forward (Delete key) — Task 4
- [x] Auto-indent — Task 4
- [x] Word jump (Ctrl+Left/Right) — Task 5
- [x] Start/end of file (Ctrl+Home/End) — Task 5
- [x] Bracket matching (Alt+]) — Task 5
- [x] Cursor position info (Ctrl+C) — Task 5
- [x] Indent/unindent block (Alt+}/Alt+{) — Task 6
- [x] Comment toggle (Alt+;) — Task 6
- [x] Working regex search — Task 7
- [x] Whitespace display (Alt+P) — Task 8
- [x] Constant cursor position — Task 8
- [x] Updated help modal — Task 9

**Not included (out of scope for 1.4):**
- Multi-buffer support (major architecture change)
- File browser (complex UI component)
- Spell checking (external tool integration)
- Linting/formatting (external tool integration)
- Macro recording (needs keystream capture)
- Verbatim input (niche use case)
- Execute command / pipe (shell integration)
- Custom keybindings via config (config parser complexity)
- Suspend (conflicts with Ctrl+Z = undo)
- Word completion (requires word index)
- Backup files (filesystem ops)

These can be targeted for 1.5+.
