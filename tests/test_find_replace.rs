mod helpers;
use crossterm::event::KeyCode;
use helpers::*;
use rune::editor::InputMode;
use rune::search::MAX_SEARCH_MATCHES;

fn search_matches(tabs: &TabManager) -> Vec<(usize, usize)> {
    tabs.active_editor().search.search_matches.clone()
}

fn current_match(tabs: &TabManager) -> Option<(usize, usize)> {
    let editor = tabs.active_editor();
    editor
        .search
        .current_match_index
        .and_then(|i| editor.search.search_matches.get(i).copied())
}

#[test]
fn test_find_literal_basic() {
    let mut tabs = TestEditor::with_content("foo bar foo baz foo\n");
    set_cursor(&mut tabs, 0, 0);
    send_ctrl(&mut tabs, 'f');
    assert_eq!(tabs.input_mode, InputMode::Find);
    type_string(&mut tabs, "foo");
    assert_eq!(search_matches(&tabs).len(), 3);
    assert_eq!(current_match(&tabs), Some((0, 0)));
    assert_eq!(cursor(&tabs), (0, 0));
}

#[test]
fn test_find_selects_first_match_after_cursor() {
    let mut tabs = TestEditor::with_content("foo bar foo baz\n");
    set_cursor(&mut tabs, 0, 4);
    send_ctrl(&mut tabs, 'f');
    type_string(&mut tabs, "foo");
    assert_eq!(current_match(&tabs), Some((0, 8)));
    assert_eq!(cursor(&tabs), (0, 8));
}

#[test]
fn test_find_navigate_forward_wraps() {
    let mut tabs = TestEditor::with_content("ab ab ab\n");
    set_cursor(&mut tabs, 0, 0);
    send_ctrl(&mut tabs, 'f');
    type_string(&mut tabs, "ab");
    assert_eq!(current_match(&tabs), Some((0, 0)));
    send_key(&mut tabs, KeyCode::Down);
    assert_eq!(current_match(&tabs), Some((0, 3)));
    send_key(&mut tabs, KeyCode::Down);
    assert_eq!(current_match(&tabs), Some((0, 6)));
    send_key(&mut tabs, KeyCode::Down);
    assert_eq!(current_match(&tabs), Some((0, 0)));
}

#[test]
fn test_find_navigate_backward_wraps() {
    let mut tabs = TestEditor::with_content("ab ab ab\n");
    set_cursor(&mut tabs, 0, 0);
    send_ctrl(&mut tabs, 'f');
    type_string(&mut tabs, "ab");
    send_key(&mut tabs, KeyCode::Up);
    assert_eq!(current_match(&tabs), Some((0, 6)));
    send_key(&mut tabs, KeyCode::Up);
    assert_eq!(current_match(&tabs), Some((0, 3)));
}

#[test]
fn test_find_case_insensitive_by_default() {
    let mut tabs = TestEditor::with_content("Hello HELLO hello\n");
    set_cursor(&mut tabs, 0, 0);
    send_ctrl(&mut tabs, 'f');
    type_string(&mut tabs, "hello");
    assert_eq!(search_matches(&tabs).len(), 3);
}

#[test]
fn test_find_case_sensitive_toggle() {
    let mut tabs = TestEditor::with_content("Hello HELLO hello\n");
    set_cursor(&mut tabs, 0, 0);
    send_ctrl(&mut tabs, 'f');
    type_string(&mut tabs, "hello");
    assert_eq!(search_matches(&tabs).len(), 3);

    // Ctrl+O → FindOptionsMenu, then 'c' toggles case sensitivity, returns to Find.
    send_ctrl(&mut tabs, 'o');
    assert_eq!(tabs.input_mode, InputMode::FindOptionsMenu);
    send_key(&mut tabs, KeyCode::Char('c'));
    assert_eq!(tabs.input_mode, InputMode::Find);
    assert!(tabs.active_editor().search.case_sensitive);

    // Re-run the search to apply the new setting.
    send_key(&mut tabs, KeyCode::Backspace);
    type_string(&mut tabs, "o");
    assert_eq!(search_matches(&tabs).len(), 1);
}

#[test]
fn test_find_regex_toggle() {
    let mut tabs = TestEditor::with_content("abc 123 def 456\n");
    set_cursor(&mut tabs, 0, 0);
    send_ctrl(&mut tabs, 'f');
    // Empty buffer: Ctrl+R toggles regex mode.
    send_ctrl(&mut tabs, 'r');
    assert!(tabs.active_editor().search.use_regex);
    type_string(&mut tabs, r"\d+");
    assert_eq!(search_matches(&tabs).len(), 2);
}

#[test]
fn test_find_no_match() {
    let mut tabs = TestEditor::with_content("hello world\n");
    set_cursor(&mut tabs, 0, 0);
    send_ctrl(&mut tabs, 'f');
    type_string(&mut tabs, "zzz");
    assert!(search_matches(&tabs).is_empty());
    assert!(tabs.active_editor().search.current_match_index.is_none());
}

#[test]
fn test_find_esc_restores_pre_find_cursor() {
    // Esc returns cursor to the position the user was at when they pressed
    // Ctrl+F, NOT to whatever match they last navigated to. The snapshot
    // must be taken once in start_find and not clobbered by perform_find.
    let mut tabs = TestEditor::with_content("foo bar foo baz foo qux\n");
    set_cursor(&mut tabs, 0, 4);
    send_ctrl(&mut tabs, 'f');
    type_string(&mut tabs, "foo");
    // Cursor has drifted to the first match at/after col 4.
    assert_eq!(cursor(&tabs), (0, 8));
    // Navigate forward once more so we're really far from the start.
    send_key(&mut tabs, KeyCode::Down);
    assert_eq!(cursor(&tabs), (0, 16));
    send_key(&mut tabs, KeyCode::Esc);
    assert_eq!(tabs.input_mode, InputMode::Normal);
    // Must land back on the pre-find position (0, 4), not (0, 16).
    assert_eq!(cursor(&tabs), (0, 4));
}

#[test]
fn test_find_enter_keeps_cursor_at_match() {
    let mut tabs = TestEditor::with_content("foo bar foo baz\n");
    set_cursor(&mut tabs, 0, 0);
    send_ctrl(&mut tabs, 'f');
    type_string(&mut tabs, "foo");
    send_key(&mut tabs, KeyCode::Down);
    assert_eq!(cursor(&tabs), (0, 8));
    send_key(&mut tabs, KeyCode::Enter);
    assert_eq!(tabs.input_mode, InputMode::Normal);
    assert_eq!(cursor(&tabs), (0, 8));
    assert!(tabs.active_editor().search.search_matches.is_empty());
}

#[test]
fn test_find_backspace_to_empty_clears_matches() {
    let mut tabs = TestEditor::with_content("foo bar foo\n");
    set_cursor(&mut tabs, 0, 0);
    send_ctrl(&mut tabs, 'f');
    type_string(&mut tabs, "foo");
    assert_eq!(search_matches(&tabs).len(), 2);
    send_key(&mut tabs, KeyCode::Backspace);
    send_key(&mut tabs, KeyCode::Backspace);
    send_key(&mut tabs, KeyCode::Backspace);
    assert!(search_matches(&tabs).is_empty());
    assert!(tabs.active_editor().search.current_match_index.is_none());
}

#[test]
fn test_replace_all_applies_every_match() {
    let mut tabs = TestEditor::with_content("foo bar foo baz foo\n");
    set_cursor(&mut tabs, 0, 0);
    // Ctrl+\ starts replace from Normal mode.
    send_ctrl(&mut tabs, '\\');
    assert_eq!(tabs.input_mode, InputMode::Replace);
    type_string(&mut tabs, "foo");
    send_key(&mut tabs, KeyCode::Enter); // FindPattern → ReplaceWith
    type_string(&mut tabs, "qux");
    send_key(&mut tabs, KeyCode::Enter); // → ReplaceConfirm
    assert_eq!(tabs.input_mode, InputMode::ReplaceConfirm);
    send_key(&mut tabs, KeyCode::Char('a'));
    assert_eq!(tabs.input_mode, InputMode::Normal);
    assert_eq!(content(&tabs), "qux bar qux baz qux\n");
}

#[test]
fn test_replace_single_match() {
    let mut tabs = TestEditor::with_content("foo foo foo\n");
    set_cursor(&mut tabs, 0, 0);
    send_ctrl(&mut tabs, '\\');
    type_string(&mut tabs, "foo");
    send_key(&mut tabs, KeyCode::Enter);
    type_string(&mut tabs, "bar");
    send_key(&mut tabs, KeyCode::Enter);
    send_key(&mut tabs, KeyCode::Char('y'));
    assert_eq!(content(&tabs), "bar foo foo\n");
}

#[test]
fn test_replace_with_empty_deletes_pattern() {
    let mut tabs = TestEditor::with_content("xxhelloxxhelloxx\n");
    set_cursor(&mut tabs, 0, 0);
    send_ctrl(&mut tabs, '\\');
    type_string(&mut tabs, "hello");
    send_key(&mut tabs, KeyCode::Enter);
    // No replacement text — go straight to confirm.
    send_key(&mut tabs, KeyCode::Enter);
    send_key(&mut tabs, KeyCode::Char('a'));
    assert_eq!(content(&tabs), "xxxxxx\n");
}

#[test]
fn test_replace_no_match() {
    let mut tabs = TestEditor::with_content("hello world\n");
    set_cursor(&mut tabs, 0, 0);
    send_ctrl(&mut tabs, '\\');
    type_string(&mut tabs, "zzz");
    send_key(&mut tabs, KeyCode::Enter);
    type_string(&mut tabs, "qqq");
    send_key(&mut tabs, KeyCode::Enter);
    send_key(&mut tabs, KeyCode::Char('a'));
    assert_eq!(tabs.input_mode, InputMode::Normal);
    assert_eq!(content(&tabs), "hello world\n");
}

#[test]
fn test_replace_esc_cancels() {
    let mut tabs = TestEditor::with_content("foo bar\n");
    send_ctrl(&mut tabs, '\\');
    type_string(&mut tabs, "foo");
    send_key(&mut tabs, KeyCode::Esc);
    assert_eq!(tabs.input_mode, InputMode::Normal);
    assert_eq!(content(&tabs), "foo bar\n");
}

#[test]
fn test_find_ctrl_r_switches_to_replace() {
    let mut tabs = TestEditor::with_content("foo bar foo\n");
    set_cursor(&mut tabs, 0, 0);
    send_ctrl(&mut tabs, 'f');
    type_string(&mut tabs, "foo");
    // Non-empty buffer: Ctrl+R transitions directly to Replace/ReplaceWith.
    send_ctrl(&mut tabs, 'r');
    assert_eq!(tabs.input_mode, InputMode::Replace);
    type_string(&mut tabs, "XYZ");
    send_key(&mut tabs, KeyCode::Enter);
    assert_eq!(tabs.input_mode, InputMode::ReplaceConfirm);
    send_key(&mut tabs, KeyCode::Char('a'));
    assert_eq!(content(&tabs), "XYZ bar XYZ\n");
}

#[test]
fn test_search_cap_truncates_at_max_matches() {
    // Build a document with > MAX_SEARCH_MATCHES occurrences of 'a'.
    let mut buf = String::with_capacity(MAX_SEARCH_MATCHES + 100);
    for _ in 0..(MAX_SEARCH_MATCHES + 50) {
        buf.push_str("a\n");
    }
    let mut tabs = TestEditor::with_content(&buf);
    set_cursor(&mut tabs, 0, 0);
    send_ctrl(&mut tabs, 'f');
    type_string(&mut tabs, "a");
    assert_eq!(search_matches(&tabs).len(), MAX_SEARCH_MATCHES);
    assert!(tabs.active_editor().search.search_matches_truncated);
}

#[test]
fn test_find_multiline_matches() {
    let mut tabs = TestEditor::with_content("alpha\nbeta\nalpha\ngamma\nalpha\n");
    set_cursor(&mut tabs, 0, 0);
    send_ctrl(&mut tabs, 'f');
    type_string(&mut tabs, "alpha");
    assert_eq!(search_matches(&tabs).len(), 3);
    send_key(&mut tabs, KeyCode::Down);
    assert_eq!(current_match(&tabs), Some((2, 0)));
    send_key(&mut tabs, KeyCode::Down);
    assert_eq!(current_match(&tabs), Some((4, 0)));
}
