use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

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
