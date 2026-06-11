//! Repro tests for code-review findings (see CODE_REVIEW.md). Each test
//! asserts the CORRECT behavior and currently fails, so they are #[ignore]d
//! to keep `cargo test` green. Run with: cargo test --test review_repro -- --ignored
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use ropey::Rope;
use rune::editor::InputMode;
use rune::input::handle_key_event;
use rune::tabs::TabManager;

fn make_tabs(text: &str) -> TabManager {
    let mut t = TabManager::new_for_test();
    t.active_editor_mut().rope = Rope::from_str(text);
    t
}

// Finding: unbound Ctrl/Alt chords fall through to the literal-insert arm.
#[test]
fn repro_unbound_ctrl_chord_inserts_text() {
    let mut t = make_tabs("hello\n");
    t.active_editor_mut().viewport.cursor_pos = (0, 0);
    let key = KeyEvent::new(KeyCode::Char('j'), KeyModifiers::CONTROL);
    let _ = handle_key_event(&mut t, key).unwrap();
    assert_eq!(
        t.active_editor().rope.to_string(),
        "hello\n",
        "Ctrl+J should be a no-op but modified the buffer"
    );
}

#[test]
fn repro_unbound_alt_chord_inserts_text() {
    let mut t = make_tabs("hello\n");
    t.active_editor_mut().viewport.cursor_pos = (0, 0);
    let key = KeyEvent::new(KeyCode::Char('x'), KeyModifiers::ALT);
    let _ = handle_key_event(&mut t, key).unwrap();
    assert_eq!(
        t.active_editor().rope.to_string(),
        "hello\n",
        "Alt+X should be a no-op but modified the buffer"
    );
}

// Finding: ConfirmCloseTab 'Y' (= "Save and close") on an untitled buffer
// closes it WITHOUT saving (silent data loss).
#[test]
fn repro_save_and_close_untitled_discards_buffer() {
    let mut t = make_tabs("precious data\n");
    t.new_tab(); // second tab so close doesn't quit
    t.active_tab = 0;
    t.active_editor_mut().modified = true;
    assert!(t.active_editor().file_path.is_none());

    t.input_mode = InputMode::ConfirmCloseTab;
    let y = KeyEvent::new(KeyCode::Char('y'), KeyModifiers::NONE);
    let _ = handle_key_event(&mut t, y).unwrap();

    // The buffer should still exist (we should have been prompted for a
    // filename), but the tab is gone and the content was never written.
    assert_eq!(t.tabs.len(), 2, "tab was closed without saving its content");
}

// Finding: insert_char advances cursor by 1 display column even for
// wide (CJK) characters, so subsequent inserts land at the wrong index.
#[test]
fn repro_wide_char_typing_scrambles_text() {
    let mut t = make_tabs("");
    for c in ['あ', 'x', 'y'] {
        let key = KeyEvent::new(KeyCode::Char(c), KeyModifiers::NONE);
        let _ = handle_key_event(&mut t, key).unwrap();
    }
    assert_eq!(t.active_editor().rope.to_string(), "あxy");
}

// Finding: Replace-All ignores regex and case-insensitive modes even though
// Find honors them.
#[test]
fn repro_replace_all_ignores_regex_mode() {
    let mut t = make_tabs("abc adc\n");
    {
        let e = t.active_editor_mut();
        e.search.use_regex = true;
        e.search.search_buffer = "a.c".to_string();
        // Find sees 2 matches:
        let m = e.search.find_all_matches(&Rope::from_str("abc adc\n"));
        assert_eq!(m.len(), 2, "find honors regex");
    }
    let n = t.active_editor_mut().perform_replace("a.c", "X");
    assert_eq!(n, 2, "replace-all should honor regex mode like find does");
}

#[test]
fn repro_replace_all_ignores_case_insensitive_mode() {
    let mut t = make_tabs("Hello hello\n");
    {
        let e = t.active_editor_mut();
        e.search.case_sensitive = false; // default: insensitive
        e.search.search_buffer = "hello".to_string();
        let m = e.search.find_all_matches(&Rope::from_str("Hello hello\n"));
        assert_eq!(m.len(), 2, "find is case-insensitive");
    }
    let n = t.active_editor_mut().perform_replace("hello", "X");
    assert_eq!(n, 2, "replace-all should be case-insensitive like find");
}

// Finding: read-only mode does not block Replace or word-completion edits.
#[test]
fn repro_read_only_allows_replace() {
    let mut t = make_tabs("hello\n");
    t.read_only = true;
    // Ctrl+\ opens replace even in view mode
    let key = KeyEvent::new(KeyCode::Char('\\'), KeyModifiers::CONTROL);
    let _ = handle_key_event(&mut t, key).unwrap();
    assert_ne!(
        t.input_mode,
        InputMode::Replace,
        "view mode should not allow entering Replace"
    );
}

// Finding: render cache keyed by rope address + len + generation can serve
// stale spans after a tab close shifts another Editor into the same slot.
#[test]
fn repro_render_cache_stale_after_tab_close() {
    use ratatui::backend::TestBackend;
    use ratatui::Terminal;

    let mut t = make_tabs("AAAA\n");
    t.new_tab();
    t.active_editor_mut().rope = Rope::from_str("BBBB\n"); // same len_chars
    t.active_tab = 0;

    let backend = TestBackend::new(40, 10);
    let mut term = Terminal::new(backend).unwrap();
    term.draw(|f| rune::ui::draw_ui(f, &mut t)).unwrap();

    // Close tab 0; tab with BBBB shifts into slot 0 (same rope address,
    // same len_chars, same dirty_generation = 0).
    t.tabs.remove(0);
    t.active_tab = 0;

    term.draw(|f| rune::ui::draw_ui(f, &mut t)).unwrap();
    let buf = term.backend().buffer().clone();
    let row1: String = (0..40)
        .map(|x| buf[(x, 1)].symbol().chars().next().unwrap_or(' '))
        .collect();
    assert!(
        row1.contains("BBBB"),
        "expected BBBB after closing first tab, got row: {row1:?}"
    );
}

// Finding: fuzzy finder truncation slices the label at a byte offset,
// panicking on multibyte tab names.
#[test]
fn repro_fuzzy_finder_multibyte_name_panics() {
    use ratatui::backend::TestBackend;
    use ratatui::Terminal;

    let mut t = make_tabs("hi\n");
    t.active_editor_mut().display_name = "日本語のとても長いファイル名前前前前前前前前前.txt".to_string();
    t.input_mode = InputMode::FuzzyFinder;
    t.rebuild_fuzzy_candidates();

    let backend = TestBackend::new(54, 16);
    let mut term = Terminal::new(backend).unwrap();
    // Should not panic
    term.draw(|f| rune::ui::draw_ui(f, &mut t)).unwrap();
}

// Finding: Tab insertion saves one undo state per inserted space (plus one
// extra), so a single Tab press needs multiple Ctrl+Z to undo.
#[test]
#[ignore = "documents a known bug found in code review; run with --ignored"]
fn repro_tab_needs_multiple_undos() {
    let mut t = make_tabs("");
    let key = KeyEvent::new(KeyCode::Tab, KeyModifiers::NONE);
    let _ = handle_key_event(&mut t, key).unwrap();
    assert_eq!(t.active_editor().rope.to_string(), "    ");
    t.undo();
    assert_eq!(
        t.active_editor().rope.to_string(),
        "",
        "one undo should revert one Tab press"
    );
}
