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
