#![cfg(feature = "terminal")]

use ropey::Rope;
use rune::{editor::Editor, tabs::TabManager};

#[test]
fn uncomment_stream_matches_string_reference_and_undo() {
    for (syntax, marker) in [("Rust", "//"), ("Python", "#"), ("SQL", "--")] {
        for indent in ["", " \t", "\u{3000}\u{a0}"] {
            for suffix in ["hello 界", " no-space", "", "\r\n", "x // nested"] {
                for space in ["", " "] {
                    let text = format!("{indent}{marker}{space}{suffix}");
                    let mut editor = Editor::new_buffer();
                    editor.rope = Rope::from_str(&text);
                    editor.syntax_name = Some(syntax.into());
                    let pos = text.find(marker).unwrap();
                    let len =
                        marker.len() + usize::from(text[pos + marker.len()..].starts_with(' '));
                    let expected = format!("{}{}", &text[..pos], &text[pos + len..]);
                    editor.toggle_comment();
                    assert_eq!(editor.rope.to_string(), expected, "{text:?}");
                    assert!(editor
                        .undo_manager
                        .undo(&mut editor.rope, &mut editor.viewport.cursor_pos));
                    assert_eq!(editor.rope.to_string(), text);
                }
            }
        }
    }
}

#[test]
fn shortening_lines_clamps_cursor_after_invalidating_cached_width() {
    let mut tabs = TabManager::new_for_test();
    let e = tabs.active_editor_mut();
    e.rope = Rope::from_str("// x");
    e.viewport.cursor_pos = (0, 4);
    e.clamp_cursor_to_line();
    e.toggle_comment();
    assert_eq!(e.viewport.cursor_pos, (0, 1));

    let e = tabs.active_editor_mut();
    e.rope = Rope::from_str("    x");
    e.mark_document_changed(0);
    e.viewport.cursor_pos = (0, 5);
    e.clamp_cursor_to_line();
    tabs.config.tab_width = 4;
    tabs.unindent_lines();
    assert_eq!(tabs.active_editor().viewport.cursor_pos, (0, 1));
}
