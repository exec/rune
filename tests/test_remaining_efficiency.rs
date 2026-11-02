use ropey::Rope;
use rune::editor::{char_display_width, line_display_width, Editor};

fn reference_right(rope: &Rope, line: usize, col: usize) -> (usize, usize) {
    let width = line_display_width(rope, line);
    if col < width {
        let mut prefix = 0;
        for ch in rope.line(line).chars().filter(|&ch| ch != '\n') {
            prefix += char_display_width(ch, prefix);
            if prefix > col {
                return (line, prefix);
            }
        }
    } else if line + 1 < rope.len_lines() {
        return (line + 1, 0);
    }
    (line, col)
}

#[test]
fn streamed_right_arrow_matches_full_width_reference() {
    for text in [
        "abc\n\nend",
        "a\t界e\u{301}\r\nlast",
        "\0\u{301}界\txyz\n",
        "a\u{2028}bc",
        "\t\t",
        "e\u{301}\u{308}",
    ] {
        let mut editor = Editor::new_buffer();
        editor.rope = Rope::from_str(text);
        for line in 0..editor.rope.len_lines() {
            for col in 0..=line_display_width(&editor.rope, line) + 3 {
                let expected = reference_right(&editor.rope, line, col);
                editor.viewport.cursor_pos = (line, col);
                editor.move_cursor_right();
                assert_eq!(
                    editor.viewport.cursor_pos, expected,
                    "{text:?} {line}:{col}"
                );
                // Exact-hit cache entries must still resolve to the first index at that column.
                let (line, col) = expected;
                let mut fresh = Editor::new_buffer();
                fresh.rope = editor.rope.clone();
                assert_eq!(
                    editor.line_col_to_char_idx(line, col),
                    fresh.line_col_to_char_idx(line, col)
                );
            }
        }
    }
}

#[test]
fn typing_and_right_arrow_preserve_text_and_undo() {
    let mut editor = Editor::new_buffer();
    editor.rope = Rope::from_str("a\t界e\u{301}z");
    editor.insert_char('x');
    editor.move_cursor_right();
    assert_eq!(editor.viewport.cursor_pos, (0, 2));
    editor.insert_char('β');
    editor.move_cursor_right();
    assert_eq!(editor.rope.to_string(), "xaβ\t界e\u{301}z");
    assert_eq!(editor.viewport.cursor_pos, (0, 4));
    assert!(editor
        .undo_manager
        .undo(&mut editor.rope, &mut editor.viewport.cursor_pos));
    assert_eq!(editor.rope.to_string(), "xa\t界e\u{301}z");
}

#[test]
fn typing_inside_wide_characters_snaps_to_the_inserted_character() {
    for text in ["\tx", "界x", "a\t界x"] {
        for col in 0..=line_display_width(&Rope::from_str(text), 0) + 2 {
            for typed in ['X', 'β', '\t', '\u{301}'] {
                let mut editor = Editor::new_buffer();
                editor.rope = Rope::from_str(text);
                editor.viewport.cursor_pos = (0, col);
                let pos = editor.line_col_to_char_idx(0, col);
                let before = editor.char_idx_to_display_col(0, pos);
                editor.insert_char(typed);
                assert_eq!(
                    editor.viewport.cursor_pos,
                    (0, before + char_display_width(typed, before)),
                    "{text:?}, col={col}, typed={typed:?}"
                );
                editor.move_cursor_right();
                let mut fresh = Editor::new_buffer();
                fresh.rope = editor.rope.clone();
                let (line, col) = editor.viewport.cursor_pos;
                assert_eq!(
                    editor.line_col_to_char_idx(line, col),
                    fresh.line_col_to_char_idx(line, col)
                );
            }
        }
    }
}
