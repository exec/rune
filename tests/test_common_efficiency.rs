#![cfg(feature = "terminal")]

use ropey::Rope;
use rune::editor::{line_display_width, Editor};

#[test]
fn bounded_clamping_matches_full_width_with_and_without_a_warm_cache() {
    for text in [
        "abc\n\n",
        "a\t界e\u{301}\r\nlast",
        "\0\u{301}界\txyz\n",
        "a\u{2028}bc",
    ] {
        let mut editor = Editor::new_buffer();
        editor.rope = Rope::from_str(text);
        for line in 0..editor.rope.len_lines() + 2 {
            for col in 0..=30 {
                for warm in [false, true] {
                    editor.invalidate_cache();
                    let valid_line = line.min(editor.rope.len_lines() - 1);
                    let width = line_display_width(&editor.rope, valid_line);
                    if warm {
                        editor.line_display_width_cached(valid_line);
                    }
                    editor.viewport.cursor_pos = (line, col);
                    editor.clamp_cursor_to_line();
                    assert_eq!(
                        editor.viewport.cursor_pos,
                        (valid_line, col.min(width)),
                        "{text:?}, {line}:{col}, warm={warm}"
                    );
                }
            }
        }
    }
}

#[test]
fn passive_mouse_events_do_not_request_redraw_or_clear_pending_redraw() {
    use crossterm::event::{KeyModifiers, MouseButton, MouseEvent, MouseEventKind};
    let mut tabs = rune::tabs::TabManager::new_for_test();
    for i in 0..1000 {
        for kind in [
            MouseEventKind::Moved,
            MouseEventKind::Up(MouseButton::Left),
            MouseEventKind::ScrollLeft,
            MouseEventKind::ScrollRight,
        ] {
            for pending in [false, true] {
                tabs.needs_redraw = pending;
                tabs.handle_mouse_event(
                    MouseEvent {
                        kind,
                        column: (i % 80) as u16,
                        row: 2,
                        modifiers: KeyModifiers::NONE,
                    },
                    40,
                );
                assert_eq!(tabs.needs_redraw, pending);
                assert_eq!(tabs.active_editor().viewport.cursor_pos, (0, 0));
            }
        }
    }
    tabs.needs_redraw = false;
    tabs.handle_mouse_event(
        MouseEvent {
            kind: MouseEventKind::Down(MouseButton::Left),
            column: 0,
            row: 2,
            modifiers: KeyModifiers::NONE,
        },
        40,
    );
    assert!(tabs.needs_redraw);
}
