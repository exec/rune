use ropey::Rope;
use rune::editor::{line_display_width, Editor};
use rune::tabs::TabManager;

#[test]
fn dense_and_sparse_replacement_match_regex_and_preserve_undo() {
    let dense = "界 aa aaaa café\r\n".repeat(80);
    let sparse = ("aa".to_string() + &"x".repeat(10_000) + "\n").repeat(40);
    for text in [&dense, &sparse] {
        for (pattern, regex) in [("aa", false), ("a+", true), ("a*", true)] {
            for replacement in ["", "$literal\n界"] {
                let source = if regex {
                    pattern.to_string()
                } else {
                    regex::escape(pattern)
                };
                let re = regex::RegexBuilder::new(&source)
                    .case_insensitive(true)
                    .build()
                    .unwrap();
                let expected = re.replace_all(text, regex::NoExpand(replacement));
                let mut e = Editor::new_buffer();
                e.rope = Rope::from_str(text);
                e.search.use_regex = regex;
                let count = e.perform_replace(pattern, replacement);
                assert_eq!(count, re.find_iter(text).count());
                assert_eq!(
                    e.rope.to_string(),
                    expected,
                    "{pattern:?} -> {replacement:?}"
                );
                assert!(e.undo_manager.undo(&mut e.rope, &mut e.viewport.cursor_pos));
                assert_eq!(e.rope.to_string(), *text);
            }
        }
    }
}

#[test]
fn repeated_backspace_preserves_exact_unicode_positions() {
    let mut expected = "a界e\u{301}\tz".repeat(300);
    let mut e = Editor::new_buffer();
    e.rope = Rope::from_str(&expected);
    e.viewport.cursor_pos = (0, line_display_width(&e.rope, 0));
    for _ in 0..400 {
        e.delete_char();
        expected.pop();
        assert_eq!(e.rope.to_string(), expected);
        assert_eq!(e.viewport.cursor_pos.1, line_display_width(&e.rope, 0));
    }
}

#[test]
fn wrapped_navigation_keeps_cursor_visible_after_jumps_and_resizes() {
    let text: String = (0..150).map(|i| "界a".repeat(i % 40) + "\n").collect();
    let mut e = Editor::new_buffer();
    e.rope = Rope::from_str(&text);
    let mut seed = 17u64;
    for _ in 0..400 {
        seed = seed.wrapping_mul(6364136223846793005).wrapping_add(1);
        let line = (seed as usize >> 8) % e.rope.len_lines();
        let col = (seed as usize >> 16) % (line_display_width(&e.rope, line) + 1);
        let width = (seed as usize >> 24) % 40 + 1;
        let height = (seed as usize >> 32) % 12 + 1;
        e.viewport.cursor_pos = (line, col);
        e.update_viewport_for_size(height, width, 0, true);
        let top = e.viewport.viewport_offset.0;
        let skipped = e.viewport.wrapped_offset;
        assert!(top <= line);
        assert!(skipped < e.wrapped_line_height(top, width));
        let cursor_sub = (col / width).min(e.wrapped_line_height(line, width) - 1);
        let rows: usize = (top..line).map(|i| e.wrapped_line_height(i, width)).sum();
        assert!(rows + cursor_sub >= skipped);
        assert!(rows + cursor_sub - skipped < height);
    }
}

#[test]
fn cached_search_tracks_modes_and_unicode_resume_offsets() {
    let rope = Rope::from_str("Aa.a İa\nfoo café");
    let mut search = rune::search::SearchState::default();
    search.search_buffer = ".".into();
    assert_eq!(search.match_spans(&rope, 0, 100), vec![(0, 2, 1)]);
    search.use_regex = true;
    assert!(search.match_spans(&rope, 0, 100).len() > 1);
    search.use_regex = false;
    search.search_buffer = "a".into();
    assert_eq!(search.match_spans(&rope, 0, 100).len(), 5);
    search.case_sensitive = true;
    assert_eq!(search.match_spans(&rope, 0, 100).len(), 4);
    assert_eq!(search.match_spans(&rope, 6, 1), vec![(0, 6, 1)]);
    search.search_buffer = "[".into();
    search.use_regex = true;
    assert!(search.match_spans(&rope, 0, 100).is_empty());
    search.use_regex = false;
    assert!(search.match_spans(&Rope::from_str("["), 0, 100).len() == 1);
}

#[test]
fn streaming_save_detects_shorter_longer_and_changed_disk_contents() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("document");
    let original = "alpha界β\n".repeat(4000).into_bytes();
    let mut shortened = original.clone();
    shortened.pop();
    let mut extended = original.clone();
    extended.push(b'x');
    let mut changed = original.clone();
    changed[8192] ^= 1;
    for external in [shortened, extended, changed] {
        std::fs::write(&path, &original).unwrap();
        let mut tabs = TabManager::new_for_test();
        tabs.open_in_current_tab(path.clone()).unwrap();
        tabs.active_editor_mut().insert_char('界');
        std::fs::write(&path, &external).unwrap();
        assert!(tabs.perform_save(path.clone()).is_err());
        assert_eq!(std::fs::read(&path).unwrap(), external);
        assert!(tabs.active_editor().modified);
        std::fs::write(&path, &original).unwrap();
        tabs.perform_save(path.clone()).unwrap();
        assert_eq!(
            std::fs::read_to_string(&path).unwrap(),
            "界".to_string() + std::str::from_utf8(&original).unwrap()
        );
    }
}

#[test]
fn completion_reads_unicode_prefix_without_copying_the_whole_line() {
    let padding = "x".repeat(100_000) + " ";
    let mut e = Editor::new_buffer();
    e.rope = Rope::from_str(&(padding.clone() + "界al\n界alpha 界alpine"));
    e.viewport.cursor_pos = (0, padding.len() + 4);
    e.word_complete();
    assert!(e.rope.line(0).to_string().ends_with("界alpha\n"));
    e.word_complete();
    assert!(e.rope.line(0).to_string().ends_with("界alpine\n"));
}

#[test]
fn wrapped_render_cache_matches_fresh_buffers_after_edits_and_mode_changes() {
    use ratatui::{backend::TestBackend, Terminal};
    fn render(tabs: &mut TabManager) -> ratatui::buffer::Buffer {
        let mut terminal = Terminal::new(TestBackend::new(30, 10)).unwrap();
        terminal.draw(|f| rune::ui::draw_ui(f, tabs)).unwrap();
        terminal.backend().buffer().clone()
    }
    let mut tabs = TabManager::new_for_test();
    tabs.config.word_wrap = true;
    tabs.config.show_line_numbers = true;
    tabs.active_editor_mut().rope =
        Rope::from_str(&"fn wide() { let 界 = \"value\"; }\n".repeat(30));
    tabs.active_editor_mut()
        .highlighter
        .set_syntax(Some("Rust"));
    for line in [0, 3, 10, 4] {
        render(&mut tabs);
        tabs.active_editor_mut().viewport.cursor_pos = (line, 4);
        tabs.active_editor_mut().insert_char('Z');
        tabs.config.show_whitespace = !tabs.config.show_whitespace;
        let actual = render(&mut tabs);
        let mut fresh = TabManager::new_for_test();
        fresh.config = tabs.config.clone();
        let old = tabs.active_editor();
        let new = fresh.active_editor_mut();
        new.rope = old.rope.clone();
        new.modified = old.modified;
        new.viewport.cursor_pos = old.viewport.cursor_pos;
        new.viewport.viewport_offset = old.viewport.viewport_offset;
        new.viewport.wrapped_offset = old.viewport.wrapped_offset;
        new.highlighter.set_syntax(Some("Rust"));
        assert_eq!(actual, render(&mut fresh));
        tabs.config.word_wrap = !tabs.config.word_wrap;
    }
}

#[test]
fn tab_name_resolution_preserves_collisions_unique_names_and_untitled_tabs() {
    let mut tabs = TabManager::new_for_test();
    for path in ["one/main.rs", "two/main.rs", "one/unique.rs"] {
        let mut editor = Editor::new_buffer();
        editor.file_path = Some(path.into());
        tabs.tabs.push(editor);
    }
    tabs.resolve_display_names();
    let names: Vec<_> = tabs.tabs.iter().map(|e| e.display_name.as_str()).collect();
    assert_eq!(
        names,
        vec!["[untitled]", "one/main.rs", "two/main.rs", "unique.rs"]
    );
    tabs.tabs.remove(2);
    tabs.resolve_display_names();
    assert_eq!(tabs.tabs[1].display_name, "main.rs");
}

#[test]
fn replacement_without_matches_does_not_allocate_an_undo_entry() {
    let mut e = Editor::new_buffer();
    e.rope = Rope::from_str("unchanged");
    assert_eq!(e.perform_replace("missing", "replacement"), 0);
    assert!(e.undo_manager.undo_stack.is_empty());
    assert!(!e.modified);
}
