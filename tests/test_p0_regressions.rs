//! Regression guards for the P0 crash / data-loss findings of the v1.5.3 review.
//!
//! Every test here failed (hung, panicked, or corrupted the buffer) before the
//! accompanying fix. They are deliberately narrow: each pins one specific
//! byte-vs-char-vs-display-column confusion rather than general editor behavior.

use ropey::Rope;
use rune::editor::{char_display_width, line_display_width, str_display_width, Editor};
use std::sync::mpsc;
use std::thread;
use std::time::Duration;

/// `line_display_width` must agree with char-by-char accumulation for every
/// input, including control characters. A disagreement makes
/// `move_cursor_right` chase a display column that can never be reached.
#[test]
fn line_display_width_agrees_with_char_accumulation() {
    for content in [
        "hello\r\n",       // CRLF -- the common real-world case
        "ab\u{7}\n",       // BEL
        "a\u{7f}b\n",      // DEL
        "x\u{85}y\n",      // C1 NEL (multi-byte control)
        "日本語\n",        // wide chars
        "\tindent\n",      // hard tab
        "a\tb\u{3000}c\n", // tab + ideographic space
        "plain ascii\n",
        "\n",
        "",
    ] {
        let rope = Rope::from_str(content);
        let line = rope.line(0);
        let mut expected = 0usize;
        for c in line.chars().filter(|&c| c != '\n') {
            expected += char_display_width(c, expected);
        }
        assert_eq!(
            line_display_width(&rope, 0),
            expected,
            "width disagreement for {content:?}"
        );
    }
}

/// Pressing Right at the last reachable column of a CRLF line used to spin
/// forever at 100% CPU, losing every unsaved buffer in every tab.
#[test]
fn move_cursor_right_terminates_on_crlf_line() {
    let (tx, rx) = mpsc::channel();
    thread::spawn(move || {
        let mut ed = Editor::new_buffer();
        ed.rope = Rope::from_str("hello\r\nsecond\r\n");
        for _ in 0..10 {
            ed.move_cursor_right();
        }
        let _ = tx.send(ed.viewport.cursor_pos);
    });
    let pos = rx
        .recv_timeout(Duration::from_secs(10))
        .expect("move_cursor_right hung on a CRLF line");
    // Must have advanced past the end of line 0 rather than sticking.
    assert_eq!(pos.0, 1, "cursor never left the first line: {pos:?}");
}

/// Control chars also must not wedge the loop mid-line.
#[test]
fn move_cursor_right_terminates_on_control_char_line() {
    let (tx, rx) = mpsc::channel();
    thread::spawn(move || {
        let mut ed = Editor::new_buffer();
        ed.rope = Rope::from_str("ab\u{7}\ncd\n");
        for _ in 0..8 {
            ed.move_cursor_right();
        }
        let _ = tx.send(ed.viewport.cursor_pos);
    });
    rx.recv_timeout(Duration::from_secs(10))
        .expect("move_cursor_right hung on a control-char line");
}

/// `delete_char_forward` must clear the mark like every other edit path, and the
/// column helpers must clamp, so a mark on a since-deleted line cannot panic.
#[test]
fn stale_mark_anchor_does_not_panic() {
    let mut ed = Editor::new_buffer();
    ed.rope = Rope::from_str("a\n");
    ed.mark_anchor = Some((1, 0));
    ed.viewport.cursor_pos = (0, 0);
    ed.delete_char_forward();
    assert!(
        ed.mark_anchor.is_none(),
        "delete_char_forward left a stale mark"
    );
    ed.delete_char_forward();
    // Would previously panic inside ropey: "index past end of Rope".
    assert!(ed.get_selection_range().is_none());
}

/// Even if a mark somehow names a line beyond the rope, resolving it must saturate.
#[test]
fn out_of_range_mark_line_is_clamped() {
    let mut ed = Editor::new_buffer();
    ed.rope = Rope::from_str("a\n");
    ed.mark_anchor = Some((999, 0));
    ed.viewport.cursor_pos = (0, 0);
    let range = ed.get_selection_range();
    assert!(range.is_some());
    let _ = ed.char_idx_to_display_col(999, 5);
    let _ = ed.line_col_to_char_idx(999, 5);
}

/// Uncommenting a line whose indentation is multi-byte used to eat trailing code,
/// because `str::find`'s byte offset was passed to `Rope::remove`'s char range.
#[test]
fn toggle_comment_uncomment_handles_multibyte_indent() {
    let mut ed = Editor::new_buffer();
    ed.rope = Rope::from_str("\u{3000}// hello\n");
    ed.syntax_name = Some("Rust".to_string());
    ed.viewport.cursor_pos = (0, 0);
    ed.toggle_comment();
    assert_eq!(
        ed.rope.to_string(),
        "\u{3000}hello\n",
        "comment marker not removed cleanly / code destroyed"
    );
}

/// The same confusion could push the removal range past the end of the rope.
#[test]
fn toggle_comment_uncomment_multibyte_indent_no_panic() {
    let mut ed = Editor::new_buffer();
    ed.rope = Rope::from_str("\u{3000}\u{3000}//");
    ed.syntax_name = Some("Rust".to_string());
    ed.viewport.cursor_pos = (0, 0);
    ed.toggle_comment(); // previously: "Char range out of bounds: 6..8, length 4"
    assert_eq!(ed.rope.to_string(), "\u{3000}\u{3000}");
}

/// NBSP indentation, the other common multi-byte-whitespace case.
#[test]
fn toggle_comment_uncomment_handles_nbsp_indent() {
    let mut ed = Editor::new_buffer();
    ed.rope = Rope::from_str("\u{a0}\u{a0}# x\n");
    ed.syntax_name = Some("Python".to_string());
    ed.viewport.cursor_pos = (0, 0);
    ed.toggle_comment();
    assert_eq!(ed.rope.to_string(), "\u{a0}\u{a0}x\n");
}

/// The hex cursor advances by raw bytes, so leaving hex view with it parked
/// inside a multi-byte char used to panic on a non-char-boundary slice.
#[test]
fn exit_hex_view_mid_character_does_not_panic() {
    for content in ["\u{e9}x\n", "\u{3000}abc\n", "\u{1f600}z\n"] {
        for byte_cursor in 0..content.len() {
            let mut ed = Editor::new_buffer();
            ed.rope = Rope::from_str(content);
            ed.toggle_hex_view();
            ed.hex_state
                .as_mut()
                .expect("hex view should be active")
                .cursor = byte_cursor;
            ed.toggle_hex_view(); // previously: "not a char boundary"
            assert!(ed.hex_state.is_none());
        }
    }
}

/// A click can land the cursor at a display column inside a wide char or tab;
/// `line_col_to_char_idx` snaps back to the line start, which used to underflow
/// `pos - 1 - line_start` (debug panic; silent cursor corruption in release).
#[test]
fn backspace_at_column_inside_leading_wide_char() {
    let mut ed = Editor::new_buffer();
    ed.rope = Rope::from_str("abc\n\u{65e5}x\n");
    // Column 1 is the right half of the leading wide char on line 1.
    ed.viewport.cursor_pos = (1, 1);
    ed.delete_char();
    // Must join with the previous line rather than underflow or corrupt state.
    assert_eq!(ed.rope.to_string(), "abc\u{65e5}x\n");
    assert_eq!(ed.viewport.cursor_pos.0, 0);
    // And the follow-up keystroke must stay consistent.
    ed.delete_char();
    assert_eq!(ed.rope.to_string(), "ab\u{65e5}x\n");
}

#[test]
fn backspace_at_column_inside_leading_tab() {
    let mut ed = Editor::new_buffer();
    ed.rope = Rope::from_str("abc\n\tdef\n");
    ed.viewport.cursor_pos = (1, 2); // inside the tab's span
    ed.delete_char();
    assert_eq!(ed.rope.to_string(), "abc\tdef\n");
    assert_eq!(ed.viewport.cursor_pos.0, 0);
}

/// The `ui.rs` render cache is keyed on `dirty_generation`, so any operation
/// that replaces the rope must bump it or the previous frame's spans stay on
/// screen. Undo used to leave it untouched, making undo look like a no-op.
#[test]
fn undo_and_redo_bump_dirty_generation() {
    let mut tabs = rune::tabs::TabManager::new_for_test();
    tabs.active_editor_mut().rope = Rope::from_str("hello\n");
    tabs.active_editor_mut().insert_char('X');

    let after_edit = tabs.active_editor().dirty_generation;
    tabs.undo();
    let after_undo = tabs.active_editor().dirty_generation;
    assert_ne!(
        after_edit, after_undo,
        "undo did not bump dirty_generation; the render cache will serve stale spans"
    );

    tabs.redo();
    assert_ne!(
        after_undo,
        tabs.active_editor().dirty_generation,
        "redo did not bump dirty_generation"
    );
}

/// Opening a file into the current tab reuses the `Editor`, so `buffer_id` is
/// unchanged and only `dirty_generation` can invalidate the render cache.
#[test]
fn load_file_bumps_dirty_generation() {
    use std::io::Write;
    let mut f = tempfile::NamedTempFile::new().unwrap();
    writeln!(f, "BBBBB").unwrap();

    let mut ed = Editor::new_buffer();
    ed.rope = Rope::from_str("AAAAA\n");
    let before = ed.dirty_generation;
    ed.load_file(f.path().to_path_buf()).unwrap();
    assert_ne!(
        before, ed.dirty_generation,
        "load_file did not bump dirty_generation; the pane will show the previous file"
    );
}

/// `str_display_width` is the shared helper the above all depend on.
#[test]
fn str_display_width_counts_control_chars_as_zero() {
    assert_eq!(str_display_width("hello\r", 0), 5);
    assert_eq!(str_display_width("\u{65e5}", 0), 2);
    assert_eq!(str_display_width("\t", 0), 4);
}
