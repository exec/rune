#![cfg(feature = "terminal")]

use ratatui::{backend::TestBackend, buffer::Cell, layout::Position, Terminal};
use ropey::Rope;
use rune::tabs::TabManager;
const W: u16 = 80;
const H: u16 = 12;

fn pane(tabs: &mut TabManager) -> (Vec<Cell>, Position) {
    let mut terminal = Terminal::new(TestBackend::new(W, H)).unwrap();
    terminal.draw(|f| rune::ui::draw_ui(f, tabs)).unwrap();
    let mut cells = Vec::new();
    for y in 1..H - 2 {
        for x in 0..W {
            cells.push(terminal.backend().buffer()[(x, y)].clone());
        }
    }
    (cells, terminal.get_cursor_position().unwrap())
}

fn check_fresh(tabs: &mut TabManager) {
    let actual = pane(tabs);
    let mut fresh = TabManager::new_for_test();
    fresh.config = tabs.config.clone();
    let source = tabs.active_editor();
    let e = fresh.active_editor_mut();
    e.rope = source.rope.clone();
    e.syntax_name = Some("Rust".into());
    e.highlighter.set_syntax(Some("Rust"));
    e.viewport.cursor_pos = source.viewport.cursor_pos;
    e.viewport.viewport_offset = source.viewport.viewport_offset;
    e.viewport.wrapped_offset = source.viewport.wrapped_offset;
    e.viewport.manual_scroll = source.viewport.manual_scroll;
    e.mark_anchor = source.mark_anchor;
    e.search.search_buffer = source.search.search_buffer.clone();
    e.search.use_regex = source.search.use_regex;
    e.search.case_sensitive = source.search.case_sensitive;
    e.search.search_matches = source.search.search_matches.clone();
    e.search.search_match_lengths = source.search.search_match_lengths.clone();
    e.search.current_match_index = source.search.current_match_index;
    assert_eq!(
        actual,
        pane(&mut fresh),
        "text={:?}, cursor={:?}",
        source.rope.to_string(),
        source.viewport.cursor_pos
    );
    pane(tabs); // Restore this editor's cache after the fresh reference changed buffer IDs.
}

fn setup(text: &str, wrap: bool, whitespace: bool, search: bool) -> TabManager {
    let mut tabs = TabManager::new_for_test();
    tabs.config.word_wrap = wrap;
    tabs.config.show_whitespace = whitespace;
    tabs.config.show_line_numbers = whitespace;
    let e = tabs.active_editor_mut();
    e.rope = Rope::from_str(text);
    e.syntax_name = Some("Rust".into());
    e.highlighter.set_syntax(Some("Rust"));
    if search {
        e.search.use_regex = true;
        e.perform_find("a|界");
    }
    tabs
}

#[test]
fn local_edits_and_line_break_boundaries_match_fresh_cells_styles_and_cursor() {
    let text = "fn alpha() { let value = true; } ".repeat(3) + "\r\n";
    let text = text.repeat(12) + "a\rb\r\n界e\u{301}\n\nlast";
    for (wrap, whitespace) in [(false, false), (false, true), (true, true)] {
        for search in [false, true] {
            for position in [
                (0, 0),
                (1, 3),
                (12, 1),
                (13, 0),
                (13, 2),
                (14, 1),
                (14, 2),
                (15, 0),
                (16, 6),
            ] {
                for operation in 0..15 {
                    let mut tabs = setup(&text, wrap, whitespace, search);
                    tabs.active_editor_mut().viewport.cursor_pos = position;
                    pane(&mut tabs);
                    match operation {
                        0 => tabs.active_editor_mut().delete_char(),
                        1 => tabs.active_editor_mut().delete_char_forward(),
                        2 => tabs.handle_tab_insertion(),
                        3 => tabs.active_editor_mut().insert_newline(false),
                        _ => tabs.active_editor_mut().insert_char(
                            [
                                'x', '\t', '\n', '\r', '\u{b}', '\u{c}', '\u{85}', '\u{2028}',
                                '\u{2029}', '\u{301}', '界',
                            ][operation - 4],
                        ),
                    }
                    check_fresh(&mut tabs);
                }
            }
        }
    }
}

#[test]
fn batched_edits_undo_redo_and_scrolling_match_fresh_rendering() {
    let text = ("fn alpha() { let value = true; }\r\n").repeat(100);
    for wrap in [false, true] {
        let mut tabs = setup(&text, wrap, true, false);
        pane(&mut tabs);
        for line in [8, 2, 5] {
            let e = tabs.active_editor_mut();
            e.viewport.cursor_pos = (line, 0);
            e.insert_char('x');
        }
        check_fresh(&mut tabs);
        tabs.active_editor_mut().viewport.cursor_pos = (1, 0);
        tabs.active_editor_mut().insert_newline(false);
        tabs.active_editor_mut().viewport.cursor_pos = (7, 2);
        tabs.active_editor_mut().insert_char('z');
        check_fresh(&mut tabs);
        tabs.undo();
        check_fresh(&mut tabs);
        tabs.redo();
        check_fresh(&mut tabs);
        for line in [50, 90, 0, 5] {
            let e = tabs.active_editor_mut();
            e.viewport.cursor_pos = (line, 0);
            e.viewport.viewport_offset = (line, 0);
            check_fresh(&mut tabs);
        }
    }
}

#[test]
fn syntax_changes_refresh_rendered_styles_without_a_text_edit() {
    let mut tabs = setup("fn alpha() { let value = true; }\n", false, false, false);
    tabs.active_editor_mut().highlighter.set_syntax(None);
    let before = pane(&mut tabs);
    tabs.active_editor_mut()
        .highlighter
        .set_syntax(Some("Rust"));
    let after = pane(&mut tabs);
    assert_ne!(
        before.0[0].fg, after.0[0].fg,
        "changing syntax must refresh cached colors"
    );
    check_fresh(&mut tabs);
}
