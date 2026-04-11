mod helpers;
use helpers::*;
use crossterm::event::KeyCode;

#[test]
fn test_insert_char() {
    let mut tabs = TestEditor::with_content("hello\n");
    set_cursor(&mut tabs, 0, 5);
    type_string(&mut tabs, "!");
    assert_eq!(content(&tabs), "hello!\n");
    assert_eq!(cursor(&tabs), (0, 6));
}

#[test]
fn test_insert_newline() {
    let mut tabs = TestEditor::with_content("hello world\n");
    set_cursor(&mut tabs, 0, 5);
    send_key(&mut tabs, KeyCode::Enter);
    assert_eq!(content(&tabs), "hello\n world\n");
    assert_eq!(cursor(&tabs), (1, 0));
}

#[test]
fn test_backspace() {
    let mut tabs = TestEditor::with_content("hello\n");
    set_cursor(&mut tabs, 0, 5);
    send_key(&mut tabs, KeyCode::Backspace);
    assert_eq!(content(&tabs), "hell\n");
    assert_eq!(cursor(&tabs), (0, 4));
}

#[test]
fn test_backspace_joins_lines() {
    let mut tabs = TestEditor::with_content("hello\nworld\n");
    set_cursor(&mut tabs, 1, 0);
    send_key(&mut tabs, KeyCode::Backspace);
    assert_eq!(content(&tabs), "helloworld\n");
    assert_eq!(cursor(&tabs).0, 0);
}

#[test]
fn test_undo_redo() {
    let mut tabs = TestEditor::with_content("hello\n");
    set_cursor(&mut tabs, 0, 5);
    type_string(&mut tabs, "!");
    assert_eq!(content(&tabs), "hello!\n");
    send_ctrl(&mut tabs, 'z'); // undo
    assert_eq!(content(&tabs), "hello\n");
    send_ctrl(&mut tabs, 'r'); // redo
    assert_eq!(content(&tabs), "hello!\n");
}

#[test]
fn test_cursor_movement() {
    let mut tabs = TestEditor::with_content("hello\nworld\n");
    set_cursor(&mut tabs, 0, 0);
    send_key(&mut tabs, KeyCode::Right);
    assert_eq!(cursor(&tabs), (0, 1));
    send_key(&mut tabs, KeyCode::Down);
    assert_eq!(cursor(&tabs), (1, 1));
    send_key(&mut tabs, KeyCode::Left);
    assert_eq!(cursor(&tabs), (1, 0));
    send_key(&mut tabs, KeyCode::Up);
    assert_eq!(cursor(&tabs), (0, 0));
}

#[test]
fn test_home_end() {
    let mut tabs = TestEditor::with_content("hello world\n");
    set_cursor(&mut tabs, 0, 5);
    send_key(&mut tabs, KeyCode::Home);
    assert_eq!(cursor(&tabs).1, 0);
    send_key(&mut tabs, KeyCode::End);
    assert_eq!(cursor(&tabs).1, 11);
}
