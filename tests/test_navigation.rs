mod helpers;
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use helpers::*;

#[test]
fn test_word_right() {
    let mut tabs = TestEditor::with_content("hello world foo\n");
    set_cursor(&mut tabs, 0, 0);
    // Ctrl+Right jumps to next word
    let key = KeyEvent::new(KeyCode::Right, KeyModifiers::CONTROL);
    let _ = handle_key_event(&mut tabs, key);
    assert_eq!(cursor(&tabs).1, 6); // start of "world"
}

#[test]
fn test_word_right_wraps_to_next_line() {
    let mut tabs = TestEditor::with_content("hello\nworld\n");
    set_cursor(&mut tabs, 0, 0);
    // Jump past "hello" to end of line, then wrap
    let key = KeyEvent::new(KeyCode::Right, KeyModifiers::CONTROL);
    let _ = handle_key_event(&mut tabs, key);
    assert_eq!(cursor(&tabs), (1, 0)); // wrapped to next line
}

#[test]
fn test_word_left() {
    let mut tabs = TestEditor::with_content("hello world\n");
    set_cursor(&mut tabs, 0, 8);
    let key = KeyEvent::new(KeyCode::Left, KeyModifiers::CONTROL);
    let _ = handle_key_event(&mut tabs, key);
    assert_eq!(cursor(&tabs).1, 6); // start of "world"
}

#[test]
fn test_word_left_wraps_to_prev_line() {
    let mut tabs = TestEditor::with_content("hello\nworld\n");
    set_cursor(&mut tabs, 1, 0);
    let key = KeyEvent::new(KeyCode::Left, KeyModifiers::CONTROL);
    let _ = handle_key_event(&mut tabs, key);
    assert_eq!(cursor(&tabs).0, 0); // went to previous line
    assert_eq!(cursor(&tabs).1, 5); // end of "hello"
}

#[test]
fn test_goto_start_of_file() {
    let mut tabs = TestEditor::with_content("line1\nline2\nline3\n");
    set_cursor(&mut tabs, 2, 3);
    let key = KeyEvent::new(KeyCode::Home, KeyModifiers::CONTROL);
    let _ = handle_key_event(&mut tabs, key);
    assert_eq!(cursor(&tabs), (0, 0));
}

#[test]
fn test_goto_end_of_file() {
    let mut tabs = TestEditor::with_content("line1\nline2\nline3\n");
    set_cursor(&mut tabs, 0, 0);
    let key = KeyEvent::new(KeyCode::End, KeyModifiers::CONTROL);
    let _ = handle_key_event(&mut tabs, key);
    let last_line = tabs.active_editor().rope.len_lines().saturating_sub(1);
    assert_eq!(cursor(&tabs).0, last_line);
}

#[test]
fn test_bracket_matching_forward() {
    let mut tabs = TestEditor::with_content("fn main() {\n    println!(\"hi\");\n}\n");
    set_cursor(&mut tabs, 0, 10); // on the '{'
    send_alt(&mut tabs, ']');
    assert_eq!(cursor(&tabs), (2, 0)); // matching '}'
}

#[test]
fn test_bracket_matching_reverse() {
    let mut tabs = TestEditor::with_content("fn main() {\n    println!(\"hi\");\n}\n");
    set_cursor(&mut tabs, 2, 0); // on the '}'
    send_alt(&mut tabs, ']');
    assert_eq!(cursor(&tabs), (0, 10)); // matching '{'
}

#[test]
fn test_bracket_matching_parens() {
    let mut tabs = TestEditor::with_content("(a + (b * c))\n");
    set_cursor(&mut tabs, 0, 0); // on the first '('
    send_alt(&mut tabs, ']');
    assert_eq!(cursor(&tabs).1, 12); // matching closing ')'
}

#[test]
fn test_bracket_no_match() {
    let mut tabs = TestEditor::with_content("hello world\n");
    set_cursor(&mut tabs, 0, 3); // on 'l', not a bracket
    send_alt(&mut tabs, ']');
    // Cursor should not move
    assert_eq!(cursor(&tabs), (0, 3));
}

#[test]
fn test_cursor_position_info() {
    let mut tabs = TestEditor::with_content("hello\nworld\n");
    set_cursor(&mut tabs, 1, 3);
    send_ctrl(&mut tabs, 'c');
    assert!(tabs.status_message.contains("Line: 2"));
    assert!(tabs.status_message.contains("Col: 4"));
}

#[test]
fn test_cursor_position_info_at_start() {
    let mut tabs = TestEditor::with_content("hello\n");
    set_cursor(&mut tabs, 0, 0);
    send_ctrl(&mut tabs, 'c');
    assert!(tabs.status_message.contains("Line: 1"));
    assert!(tabs.status_message.contains("Col: 1"));
}
