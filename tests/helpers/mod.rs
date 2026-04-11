use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

// Re-export for convenience in test files
pub use rune::editor::Editor;
pub use rune::input::handle_key_event;
pub use rune::tabs::TabManager;

/// Builder for creating a TabManager with specific state for testing.
/// Bypasses filesystem and terminal dependencies.
pub struct TestEditor;

impl TestEditor {
    /// Create a TabManager with the given text content and cursor at (0, 0).
    pub fn with_content(content: &str) -> TabManager {
        let mut tabs = TabManager::new_for_test();
        tabs.active_editor_mut().rope = ropey::Rope::from_str(content);
        tabs
    }

    /// Create an empty TabManager.
    pub fn empty() -> TabManager {
        TabManager::new_for_test()
    }
}

/// Simulate a key press through the full input handling pipeline.
pub fn send_key(tabs: &mut TabManager, code: KeyCode) {
    let key = KeyEvent::new(code, KeyModifiers::NONE);
    let _ = handle_key_event(tabs, key);
}

/// Simulate a Ctrl+key press.
pub fn send_ctrl(tabs: &mut TabManager, c: char) {
    let key = KeyEvent::new(KeyCode::Char(c), KeyModifiers::CONTROL);
    let _ = handle_key_event(tabs, key);
}

/// Simulate an Alt+key press.
pub fn send_alt(tabs: &mut TabManager, c: char) {
    let key = KeyEvent::new(KeyCode::Char(c), KeyModifiers::ALT);
    let _ = handle_key_event(tabs, key);
}

/// Simulate typing a string character by character.
pub fn type_string(tabs: &mut TabManager, s: &str) {
    for c in s.chars() {
        send_key(tabs, KeyCode::Char(c));
    }
}

/// Send a sequence of key events.
pub fn send_keys(tabs: &mut TabManager, keys: &[KeyEvent]) {
    for key in keys {
        let _ = handle_key_event(tabs, *key);
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

/// Get the full text content of the active editor as a String.
pub fn content(tabs: &TabManager) -> String {
    tabs.active_editor().rope.to_string()
}

/// Get the cursor position as (line, col).
pub fn cursor(tabs: &TabManager) -> (usize, usize) {
    tabs.active_editor().viewport.cursor_pos
}

/// Set cursor position directly for test setup.
pub fn set_cursor(tabs: &mut TabManager, line: usize, col: usize) {
    tabs.active_editor_mut().viewport.cursor_pos = (line, col);
}
