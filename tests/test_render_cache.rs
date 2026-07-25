//! Differential tests for the per-line render cache in `ui.rs`.
//!
//! The cache now evicts only lines at or after the edit point instead of clearing
//! wholesale, which is exactly the change that can leave stale spans on screen.
//! A stale span renders wrong text without crashing, so these compare an
//! incrementally-edited render against a from-scratch render of the same final
//! content: any difference is staleness.

use ratatui::backend::TestBackend;
use ratatui::Terminal;
use ropey::Rope;
use rune::tabs::TabManager;

const W: u16 = 60;
const H: u16 = 20;

/// Capture only the editor pane. Row 0 is the tab bar and the last two rows are
/// the status and help bars; all three legitimately differ between an edited
/// buffer and a fresh one (modified marker, cursor position readout), which says
/// nothing about whether cached spans went stale.
fn render(tabs: &mut TabManager) -> String {
    let mut terminal = Terminal::new(TestBackend::new(W, H)).unwrap();
    terminal.draw(|f| rune::ui::draw_ui(f, tabs)).unwrap();
    let buf = terminal.backend().buffer();
    let mut out = String::new();
    for y in 1..(H - 2) {
        for x in 0..W {
            out.push_str(buf[(x, y)].symbol());
        }
        out.push('\n');
    }
    out
}

/// Render `content` in a brand-new manager: a fresh `buffer_id` forces a full
/// cache clear, so this is the ground truth for "what should be on screen".
fn render_fresh(content: &str, name: &str) -> String {
    let mut tabs = TabManager::new_for_test();
    tabs.active_editor_mut().rope = Rope::from_str(content);
    tabs.active_editor_mut().display_name = name.to_string();
    render(&mut tabs)
}

fn doc(lines: usize) -> String {
    (0..lines)
        .map(|i| format!("fn line_{i}() {{ let x = {i}; }}"))
        .collect::<Vec<_>>()
        .join("\n")
        + "\n"
}

/// Editing a line must not leave any other line stale, wherever the edit lands.
#[test]
fn edit_at_each_line_matches_a_fresh_render() {
    for edit_line in [0usize, 1, 5, 10, 15] {
        let mut tabs = TabManager::new_for_test();
        tabs.active_editor_mut().rope = Rope::from_str(&doc(30));
        tabs.active_editor_mut().display_name = "x.rs".to_string();

        // Warm the cache over the visible window.
        let _ = render(&mut tabs);

        tabs.active_editor_mut().viewport.cursor_pos = (edit_line, 0);
        tabs.active_editor_mut().insert_char('Z');
        let after = render(&mut tabs);

        let expected = render_fresh(&tabs.active_editor().rope.to_string(), "x.rs");
        assert_eq!(
            after, expected,
            "stale render after editing line {edit_line}"
        );
    }
}

/// Inserting a newline shifts every following line down; none may stay cached.
#[test]
fn newline_insertion_shifts_lines_without_staleness() {
    for edit_line in [0usize, 3, 9] {
        let mut tabs = TabManager::new_for_test();
        tabs.active_editor_mut().rope = Rope::from_str(&doc(30));
        tabs.active_editor_mut().display_name = "x.rs".to_string();
        let _ = render(&mut tabs);

        tabs.active_editor_mut().viewport.cursor_pos = (edit_line, 0);
        tabs.active_editor_mut().insert_newline(false);
        let after = render(&mut tabs);

        let expected = render_fresh(&tabs.active_editor().rope.to_string(), "x.rs");
        assert_eq!(after, expected, "stale render after newline at {edit_line}");
    }
}

/// Deleting a line pulls the ones below it up.
#[test]
fn deletion_matches_a_fresh_render() {
    for edit_line in [0usize, 4, 11] {
        let mut tabs = TabManager::new_for_test();
        tabs.active_editor_mut().rope = Rope::from_str(&doc(30));
        tabs.active_editor_mut().display_name = "x.rs".to_string();
        let _ = render(&mut tabs);

        tabs.active_editor_mut().viewport.cursor_pos = (edit_line, 0);
        for _ in 0..5 {
            tabs.active_editor_mut().delete_char_forward();
        }
        let after = render(&mut tabs);

        let expected = render_fresh(&tabs.active_editor().rope.to_string(), "x.rs");
        assert_eq!(
            after, expected,
            "stale render after deleting at {edit_line}"
        );
    }
}

/// Several edits between frames: `dirty_from_line` must accumulate the minimum,
/// or the earliest-edited line stays stale.
#[test]
fn multiple_edits_between_frames_use_the_lowest_line() {
    let mut tabs = TabManager::new_for_test();
    tabs.active_editor_mut().rope = Rope::from_str(&doc(30));
    tabs.active_editor_mut().display_name = "x.rs".to_string();
    let _ = render(&mut tabs);

    // Edit low, then high, with no render in between.
    tabs.active_editor_mut().viewport.cursor_pos = (12, 0);
    tabs.active_editor_mut().insert_char('B');
    tabs.active_editor_mut().viewport.cursor_pos = (2, 0);
    tabs.active_editor_mut().insert_char('A');

    let after = render(&mut tabs);
    let expected = render_fresh(&tabs.active_editor().rope.to_string(), "x.rs");
    assert_eq!(after, expected, "lowest edited line was not honoured");
}

/// Undo replaces the whole rope; nothing may survive.
#[test]
fn undo_matches_a_fresh_render() {
    let mut tabs = TabManager::new_for_test();
    tabs.active_editor_mut().rope = Rope::from_str(&doc(30));
    tabs.active_editor_mut().display_name = "x.rs".to_string();
    let _ = render(&mut tabs);

    tabs.active_editor_mut().viewport.cursor_pos = (7, 0);
    tabs.active_editor_mut().insert_char('Q');
    let _ = render(&mut tabs);

    tabs.undo();
    let after = render(&mut tabs);
    let expected = render_fresh(&tabs.active_editor().rope.to_string(), "x.rs");
    assert_eq!(after, expected, "undo left stale spans on screen");
}

/// Scrolling then editing: entries for off-screen lines must not come back wrong.
#[test]
fn scroll_then_edit_matches_a_fresh_render() {
    let mut tabs = TabManager::new_for_test();
    tabs.active_editor_mut().rope = Rope::from_str(&doc(200));
    tabs.active_editor_mut().display_name = "x.rs".to_string();

    for offset in [0usize, 40, 90, 10, 0] {
        tabs.active_editor_mut().viewport.viewport_offset.0 = offset;
        let _ = render(&mut tabs);
    }

    tabs.active_editor_mut().viewport.viewport_offset.0 = 0;
    tabs.active_editor_mut().viewport.cursor_pos = (3, 0);
    tabs.active_editor_mut().insert_char('W');
    let after = render(&mut tabs);

    let mut fresh = TabManager::new_for_test();
    fresh.active_editor_mut().rope = Rope::from_str(&tabs.active_editor().rope.to_string());
    fresh.active_editor_mut().display_name = "x.rs".to_string();
    fresh.active_editor_mut().viewport.viewport_offset.0 = 0;
    let expected = render(&mut fresh);

    assert_eq!(after, expected, "stale render after scrolling then editing");
}

/// The cache must stay bounded when scrolling a large document read-only.
/// Scrolling does not bump `dirty_generation`, so nothing else evicts.
#[test]
fn scrolling_a_large_document_does_not_grow_without_bound() {
    let mut tabs = TabManager::new_for_test();
    tabs.active_editor_mut().rope = Rope::from_str(&doc(20_000));
    tabs.active_editor_mut().display_name = "x.rs".to_string();

    for offset in (0..19_000).step_by(7) {
        tabs.active_editor_mut().viewport.viewport_offset.0 = offset;
        let _ = render(&mut tabs);
    }

    // Correctness must survive pruning: the final view still has to be right.
    let offset = tabs.active_editor().viewport.viewport_offset.0;
    let after = render(&mut tabs);

    let mut fresh = TabManager::new_for_test();
    fresh.active_editor_mut().rope = Rope::from_str(&doc(20_000));
    fresh.active_editor_mut().display_name = "x.rs".to_string();
    fresh.active_editor_mut().viewport.viewport_offset.0 = offset;
    let expected = render(&mut fresh);

    assert_eq!(after, expected, "pruning corrupted the visible render");
}
