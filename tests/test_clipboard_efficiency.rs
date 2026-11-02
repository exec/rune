#![cfg(feature = "terminal")]

use ropey::Rope;
use rune::tabs::TabManager;

#[test]
fn cut_selection_invalidates_old_width_positions_before_placing_cursor() {
    let mut tabs = TabManager::new_for_test();
    let e = tabs.active_editor_mut();
    e.rope = Rope::from_str("aaa界abcdef");
    e.mark_anchor = Some((0, 3));
    e.viewport.cursor_pos = (0, 5);
    assert_eq!(e.char_idx_to_display_col(0, 4), 5);
    tabs.cut();
    assert_eq!(tabs.active_editor().rope.to_string(), "aaaabcdef");
    assert_eq!(tabs.active_editor().viewport.cursor_pos, (0, 3));
    assert_eq!(tabs.clipboard, vec!["界"]);
}

#[test]
fn cut_selection_does_not_seek_a_stale_position_past_the_shortened_line() {
    let mut tabs = TabManager::new_for_test();
    let e = tabs.active_editor_mut();
    e.rope = Rope::from_str("0123456789");
    e.mark_anchor = Some((0, 8));
    e.viewport.cursor_pos = (0, 9);
    assert_eq!(e.char_idx_to_display_col(0, 10), 10);
    tabs.cut();
    assert_eq!(tabs.active_editor().rope.to_string(), "012345679");
    assert_eq!(tabs.active_editor().viewport.cursor_pos, (0, 8));
}

#[test]
fn chunk_copies_preserve_exact_line_and_selection_bytes() {
    for text in [
        "a\t界e\u{301}\r\nlast".to_string(),
        "\0\u{1b}[31m\n終\u{2028}last".into(),
        "abcdefgh界".repeat(2000) + "\nlast",
    ] {
        let original = Rope::from_str(&text);
        let mut tabs = TabManager::new_for_test();
        tabs.active_editor_mut().rope = original.clone();
        for line in 0..original.len_lines() {
            tabs.active_editor_mut().viewport.cursor_pos = (line, 0);
            tabs.copy();
            assert_eq!(
                tabs.clipboard,
                vec![original.line(line).chars().collect::<String>()]
            );
            assert_eq!(tabs.active_editor().rope.to_string(), text);
        }
        let e = tabs.active_editor_mut();
        e.mark_anchor = Some((0, 1));
        e.viewport.cursor_pos = (1, 2);
        let (start, end) = e.get_selection_range().unwrap();
        let expected: String = original.slice(start..end).chars().collect();
        tabs.copy();
        assert_eq!(tabs.clipboard, vec![expected.clone()]);
        let e = tabs.active_editor_mut();
        e.mark_anchor = Some((0, 1));
        e.viewport.cursor_pos = (1, 2);
        tabs.cut();
        assert_eq!(tabs.clipboard, vec![expected]);
        let mut after = original.clone();
        after.remove(start..end);
        assert_eq!(tabs.active_editor().rope.to_string(), after.to_string());
        tabs.undo();
        assert_eq!(tabs.active_editor().rope.to_string(), text);
    }
}

#[test]
fn inline_pastes_match_fresh_rope_cursor_calculation_and_undo() {
    use rune::editor::{line_display_width, Editor};
    for text in ["original\n", "a\t界e\u{301}\r\nlast", "\u{301}界\txyz"] {
        for payload in [
            "",
            "abc 123",
            "界e\u{301}",
            "\t",
            "a\n",
            "\r\n",
            "a\u{2028}b",
            "\0abc",
            "\u{1b}[31m",
        ] {
            for parts in [
                vec![payload.to_string()],
                vec![String::new(), payload.to_string(), String::new()],
            ] {
                for col in 0..=line_display_width(&Rope::from_str(text), 0) + 2 {
                    let mut tabs = TabManager::new_for_test();
                    tabs.clipboard = parts.clone();
                    let e = tabs.active_editor_mut();
                    e.rope = Rope::from_str(text);
                    e.viewport.cursor_pos = (0, col);
                    e.mark_anchor = Some((0, 0));
                    let pos = e.line_col_to_char_idx(0, col);
                    let mut expected = Editor::new_buffer();
                    expected.rope = e.rope.clone();
                    expected.rope.insert(pos, payload);
                    let end = pos + payload.chars().count();
                    let line = expected.rope.char_to_line(end);
                    let offset = end - expected.rope.line_to_char(line);
                    let expected_cursor = (line, expected.char_idx_to_display_col(line, offset));
                    e.char_idx_to_display_col(0, e.rope.line(0).len_chars());
                    tabs.paste_inline();
                    assert_eq!(
                        tabs.active_editor().rope.to_string(),
                        expected.rope.to_string(),
                        "{text:?} {payload:?} col={col}"
                    );
                    assert_eq!(
                        tabs.active_editor().viewport.cursor_pos,
                        expected_cursor,
                        "{text:?} {payload:?} col={col}"
                    );
                    assert!(tabs.active_editor().mark_anchor.is_none());
                    assert_eq!(tabs.clipboard, parts);
                    tabs.undo();
                    assert_eq!(tabs.active_editor().rope.to_string(), text);
                }
            }
        }
    }
}
