use crossterm::event::{KeyCode, KeyEvent, KeyModifiers, MouseEvent, MouseEventKind};
use ratatui::{backend::TestBackend, Terminal};
use ropey::Rope;
use rune::{
    editor::{Editor, InputMode},
    input::handle_key_event,
    tabs::TabManager,
};
fn tabs(s: &str) -> TabManager {
    let mut t = TabManager::new_for_test();
    t.active_editor_mut().rope = Rope::from_str(s);
    t
}
fn key(t: &mut TabManager, c: KeyCode, m: KeyModifiers) {
    handle_key_event(t, KeyEvent::new(c, m)).unwrap();
}
fn draw(t: &mut TabManager, w: u16, h: u16) -> String {
    let mut term = Terminal::new(TestBackend::new(w, h)).unwrap();
    term.draw(|f| rune::ui::draw_ui(f, t)).unwrap();
    term.backend()
        .buffer()
        .content
        .iter()
        .map(|c| c.symbol())
        .collect()
}
#[test]
fn open_current_protects_unsaved_edits() {
    let d = tempfile::tempdir().unwrap();
    let p = d.path().join("b");
    std::fs::write(&p, "disk B").unwrap();
    let mut t = tabs("UNSAVED A");
    t.active_editor_mut().modified = true;
    key(&mut t, KeyCode::Char('o'), KeyModifiers::CONTROL);
    key(&mut t, KeyCode::Char('o'), KeyModifiers::NONE);
    t.input_buffer = p.display().to_string();
    key(&mut t, KeyCode::Enter, KeyModifiers::NONE);
    assert_eq!(t.active_editor().rope.to_string(), "UNSAVED A");
    assert_eq!(t.input_mode, InputMode::ConfirmOpen);
    key(&mut t, KeyCode::Char('n'), KeyModifiers::NONE);
    assert_eq!(t.active_editor().rope.to_string(), "UNSAVED A");
}
#[test]
fn load_resets_previous_file_undo() {
    let d = tempfile::tempdir().unwrap();
    let p = d.path().join("b");
    std::fs::write(&p, "disk B").unwrap();
    let mut t = tabs("file A");
    t.active_editor_mut().insert_char('x');
    t.active_editor_mut().modified = false;
    t.open_in_current_tab(p).unwrap();
    t.undo();
    assert_eq!(t.active_editor().rope.to_string(), "disk B");
}
#[cfg(unix)]
#[test]
fn save_preserves_symlink() {
    let d = tempfile::tempdir().unwrap();
    let target = d.path().join("target");
    let link = d.path().join("link");
    std::fs::write(&target, "old").unwrap();
    std::os::unix::fs::symlink(&target, &link).unwrap();
    let mut t = tabs("");
    t.open_in_current_tab(link.clone()).unwrap();
    t.active_editor_mut().rope = Rope::from_str("new");
    t.perform_save(link.clone()).unwrap();
    assert!(std::fs::symlink_metadata(&link)
        .unwrap()
        .file_type()
        .is_symlink());
    assert_eq!(std::fs::read_to_string(target).unwrap(), "new");
}
#[test]
fn save_as_confirms_overwrite() {
    let d = tempfile::tempdir().unwrap();
    let p = d.path().join("existing");
    std::fs::write(&p, "other work").unwrap();
    let mut t = tabs("new");
    t.input_mode = InputMode::EnteringSaveAs;
    t.input_buffer = p.display().to_string();
    key(&mut t, KeyCode::Enter, KeyModifiers::NONE);
    assert_eq!(std::fs::read_to_string(&p).unwrap(), "other work");
    assert_eq!(t.input_mode, InputMode::ConfirmOverwrite);
    key(&mut t, KeyCode::Char('y'), KeyModifiers::NONE);
    assert_eq!(std::fs::read_to_string(p).unwrap(), "new");
}
#[test]
fn combining_mark_typing_order() {
    let mut e = Editor::new_buffer();
    for c in "e\u{301}x".chars() {
        e.insert_char(c)
    }
    assert_eq!(e.rope.to_string(), "e\u{301}x");
}
#[test]
fn crlf_backspace_joins_lines() {
    let mut t = tabs("a\r\nb");
    t.active_editor_mut().viewport.cursor_pos = (1, 0);
    t.active_editor_mut().delete_char();
    assert_eq!(t.active_editor().rope.to_string(), "ab");
}
#[test]
fn unicode_lowercase_keeps_positions() {
    let mut t = tabs("İa");
    assert!(t.active_editor_mut().perform_find("a"));
    assert_eq!(t.active_editor().viewport.cursor_pos, (0, 1));
}
#[test]
fn replace_all_goes_past_find_cap() {
    let mut t = tabs(&"a ".repeat(10001));
    assert_eq!(t.active_editor_mut().perform_replace("a", "b"), 10001);
}
#[test]
fn interactive_replace_skips_past_cap() {
    let mut t = tabs(&"a ".repeat(10001));
    t.active_editor_mut().search.replace_resume_char = 20000;
    assert_eq!(
        t.active_editor_mut().perform_replace_interactive("a", "b"),
        1
    );
}
#[test]
fn paste_trailing_newline_cursor() {
    let mut t = tabs("");
    t.clipboard = vec!["x\n".into()];
    t.paste_inline();
    assert_eq!(t.active_editor().viewport.cursor_pos, (1, 0));
}
#[test]
fn long_wrapped_line_scrolls_to_cursor() {
    let mut t = tabs(&("x".repeat(70) + "END"));
    t.config.word_wrap = true;
    t.active_editor_mut().viewport.cursor_pos = (0, 73);
    let s = draw(&mut t, 10, 6);
    assert!(s.contains("END"), "{s}");
}
#[test]
fn mouse_scroll_survives_redraw() {
    let mut t = tabs(&"abc\n".repeat(100));
    draw(&mut t, 40, 10);
    t.handle_mouse_event(
        MouseEvent {
            kind: MouseEventKind::ScrollDown,
            column: 0,
            row: 2,
            modifiers: KeyModifiers::NONE,
        },
        10,
    );
    assert!(t.active_editor().viewport.viewport_offset.0 > 0);
    draw(&mut t, 40, 10);
    assert!(t.active_editor().viewport.viewport_offset.0 > 0);
}
#[test]
fn mouse_drag_selects() {
    let mut t = tabs("abcdef");
    for (kind, col) in [
        (MouseEventKind::Down(crossterm::event::MouseButton::Left), 0),
        (MouseEventKind::Drag(crossterm::event::MouseButton::Left), 4),
    ] {
        t.handle_mouse_event(
            MouseEvent {
                kind,
                column: col,
                row: 1,
                modifiers: KeyModifiers::NONE,
            },
            10,
        );
    }
    assert_eq!(t.active_editor().get_selection_range(), Some((0, 4)));
}
#[cfg(unix)]
#[test]
fn shell_timeout_terminates_descendants_and_bounds_pipe_cleanup() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("child.pid");
    let quoted_path = path.display().to_string().replace('\'', "'\\''");
    let mut t = tabs("keep selection");
    t.active_editor_mut().mark_anchor = Some((0, 0));
    t.active_editor_mut().viewport.cursor_pos = (0, 14);
    t.input_mode = InputMode::ConfirmExecute;
    t.pending_command = Some(format!("sleep 30 & echo $! > '{quoted_path}'"));
    let start = std::time::Instant::now();
    key(&mut t, KeyCode::Char('y'), KeyModifiers::NONE);
    assert!(
        start.elapsed() < std::time::Duration::from_secs(12),
        "elapsed {:?}",
        start.elapsed()
    );
    assert!(t.status_message.contains("timed out"));
    assert_eq!(t.active_editor().rope.to_string(), "keep selection");
    let pid = std::fs::read_to_string(path).unwrap();
    let state = std::process::Command::new("ps")
        .args(["-p", pid.trim(), "-o", "stat="])
        .output()
        .unwrap();
    let state = String::from_utf8_lossy(&state.stdout);
    assert!(
        state.trim().is_empty() || state.trim().starts_with('Z'),
        "child survived timeout: {state}"
    );
}
#[test]
fn regex_highlight_uses_match_length() {
    let mut t = tabs("abcd Z");
    t.active_editor_mut().search.use_regex = true;
    t.active_editor_mut().perform_find("a.+");
    let mut term = Terminal::new(TestBackend::new(40, 10)).unwrap();
    term.draw(|f| rune::ui::draw_ui(f, &mut t)).unwrap();
    assert_eq!(
        term.backend().buffer()[(3, 1)].bg,
        ratatui::style::Color::Red
    );
    assert_eq!(
        term.backend().buffer()[(5, 1)].bg,
        ratatui::style::Color::Red
    );
    assert_ne!(
        term.backend().buffer()[(6, 1)].bg,
        ratatui::style::Color::Red
    );
}
#[test]
fn same_line_block_comment_preserves_suffix() {
    let mut h = rune::syntax::SyntaxHighlighter::new();
    h.set_syntax(Some("Rust"));
    let s = h.highlight_line(0, "/* hi */ let x = 1;");
    let output: String = s.iter().map(|(_, s)| s.as_str()).collect();
    assert_eq!(output, "/* hi */ let x = 1;");
}
#[test]
fn zero_height_hex_does_not_panic() {
    let mut t = tabs("abc");
    t.toggle_hex_view();
    draw(&mut t, 10, 2);
}
#[test]
fn replace_line_break_clamps_row() {
    let mut t = tabs("a\rb");
    t.active_editor_mut().viewport.cursor_pos = (1, 0);
    t.active_editor_mut().search.use_regex = true;
    t.active_editor_mut().perform_replace(r"\r", "");
    assert_eq!(t.active_editor().viewport.cursor_pos.0, 0);
}

#[test]
fn mouse_click_wrapped_row() {
    let mut t = tabs("abcdefghijklmno\nsecond");
    t.config.word_wrap = true;
    draw(&mut t, 10, 8);
    t.handle_mouse_event(
        MouseEvent {
            kind: MouseEventKind::Down(crossterm::event::MouseButton::Left),
            column: 2,
            row: 2,
            modifiers: KeyModifiers::NONE,
        },
        8,
    );
    assert_eq!(t.active_editor().viewport.cursor_pos, (0, 12));
}
#[test]
fn paste_uses_fresh_width_cache() {
    let mut t = tabs("abc");
    let e = t.active_editor_mut();
    e.char_idx_to_display_col(0, 3);
    e.viewport.cursor_pos = (0, 3);
    t.clipboard = vec!["界".into()];
    t.paste_inline();
    assert_eq!(t.active_editor().viewport.cursor_pos, (0, 5));
}
#[test]
fn save_checks_external_modification() {
    let d = tempfile::tempdir().unwrap();
    let p = d.path().join("file");
    std::fs::write(&p, "old").unwrap();
    let mut t = TabManager::new_for_test();
    t.open_in_current_tab(p.clone()).unwrap();
    t.active_editor_mut().insert_char('x');
    std::fs::write(&p, "EXTERNAL WORK").unwrap();
    t.save_file().unwrap();
    assert_eq!(std::fs::read_to_string(p).unwrap(), "EXTERNAL WORK");
}

#[test]
fn open_confirmation_discards_only_after_successful_load() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("next");
    let mut t = tabs("keep me");
    t.active_editor_mut().modified = true;
    t.input_mode = InputMode::ConfirmOpen;
    t.input_buffer = path.display().to_string();
    key(&mut t, KeyCode::Char('y'), KeyModifiers::NONE);
    assert_eq!(t.active_editor().rope.to_string(), "keep me");
    assert!(t.active_editor().modified);
    std::fs::write(&path, "next file").unwrap();
    t.input_mode = InputMode::ConfirmOpen;
    t.input_buffer = path.display().to_string();
    key(&mut t, KeyCode::Char('y'), KeyModifiers::NONE);
    assert_eq!(t.active_editor().rope.to_string(), "next file");
    assert!(!t.active_editor().modified);
}

#[test]
fn regex_empty_matches_follow_regex_iterator_semantics() {
    let mut t = tabs("ab");
    t.active_editor_mut().search.use_regex = true;
    let actual = t.active_editor_mut().perform_replace("a*", "X");
    assert_eq!(
        actual,
        regex::Regex::new("a*").unwrap().find_iter("ab").count()
    );
    assert_eq!(
        t.active_editor().rope.to_string(),
        regex::Regex::new("a*").unwrap().replace_all("ab", "X")
    );
}

#[test]
fn crlf_file_keeps_line_endings_on_enter_and_delete() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("crlf");
    std::fs::write(&path, "a\r\nb").unwrap();
    let mut t = tabs("");
    t.open_in_current_tab(path).unwrap();
    t.active_editor_mut().viewport.cursor_pos = (0, 1);
    t.insert_newline();
    assert_eq!(t.active_editor().rope.to_string(), "a\r\n\r\nb");
    t.active_editor_mut().viewport.cursor_pos = (0, 1);
    t.active_editor_mut().delete_char_forward();
    assert_eq!(t.active_editor().rope.to_string(), "a\r\nb");
}

#[test]
fn wrapped_wheel_scrolls_within_one_document_line() {
    let mut t = tabs(&"x".repeat(200));
    t.config.word_wrap = true;
    draw(&mut t, 10, 8);
    t.handle_mouse_event(
        MouseEvent {
            kind: MouseEventKind::ScrollDown,
            column: 0,
            row: 2,
            modifiers: KeyModifiers::NONE,
        },
        8,
    );
    draw(&mut t, 10, 8);
    assert_eq!(t.active_editor().viewport.wrapped_offset, 3);
    t.handle_mouse_event(
        MouseEvent {
            kind: MouseEventKind::ScrollUp,
            column: 0,
            row: 2,
            modifiers: KeyModifiers::NONE,
        },
        8,
    );
    assert_eq!(t.active_editor().viewport.wrapped_offset, 0);
}

#[test]
fn long_line_search_applies_budget_during_scanning() {
    let rope = Rope::from_str(&"a".repeat(1_000_000));
    let mut search = rune::search::SearchState::default();
    search.search_buffer = "a".into();
    assert_eq!(search.match_spans(&rope, 0, 3).len(), 3);
    assert!(search.search_matches_truncated);
    assert_eq!(search.match_spans(&rope, 999_999, 3), vec![(0, 999_999, 1)]);
    assert!(!search.search_matches_truncated);
}

#[test]
fn find_selects_next_match_using_source_character_offsets() {
    let mut t = tabs("界x x");
    t.active_editor_mut().viewport.cursor_pos = (0, 4);
    assert!(t.active_editor_mut().perform_find("x"));
    assert_eq!(t.active_editor().viewport.cursor_pos, (0, 4));
}

#[test]
fn overwrite_cancellation_preserves_both_files_and_closing_buffer() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("existing");
    std::fs::write(&path, "disk").unwrap();
    let mut t = tabs("buffer");
    t.active_editor_mut().modified = true;
    t.start_close_tab_filename_input();
    t.input_buffer = path.display().to_string();
    key(&mut t, KeyCode::Enter, KeyModifiers::NONE);
    assert_eq!(t.input_mode, InputMode::ConfirmOverwrite);
    key(&mut t, KeyCode::Esc, KeyModifiers::NONE);
    assert_eq!(t.tabs.len(), 1);
    assert_eq!(t.active_editor().rope.to_string(), "buffer");
    assert!(t.active_editor().modified);
    assert_eq!(std::fs::read_to_string(path).unwrap(), "disk");
}

#[test]
fn switching_tabs_cancels_destination_confirmation() {
    let mut t = tabs("original");
    t.new_tab();
    t.active_tab = 0;
    t.input_mode = InputMode::ConfirmOverwrite;
    t.input_buffer = "unrelated destination".into();
    t.next_tab();
    assert_eq!(t.input_mode, InputMode::Normal);
    assert!(t.input_buffer.is_empty());
}
