#![cfg(feature = "terminal")]

use ropey::Rope;
use rune::{
    editor::{Editor, InputMode},
    input::handle_paste,
    search::ReplacePhase,
    tabs::TabManager,
};

#[test]
fn pasted_blocks_preserve_literal_bytes_cursor_clipboard_and_one_step_undo() {
    let original = "    head\r\norigin\n";
    for payload in [
        "abc",
        "first\nsecond\n",
        "\t界e\u{301}\r\nlast",
        "\u{11}\u{13}\u{1b}[2J\0",
        "a\u{2028}b\u{85}c",
    ] {
        let mut tabs = TabManager::new_for_test();
        tabs.clipboard = vec!["previous cut".into()];
        tabs.last_cut_line = Some(0);
        let e = tabs.active_editor_mut();
        e.rope = Rope::from_str(original);
        e.viewport.cursor_pos = (0, 4);
        e.viewport.manual_scroll = true;
        e.mark_anchor = Some((0, 0));
        let mut expected = Editor::new_for_test();
        expected.rope = e.rope.clone();
        expected.rope.insert(4, payload);
        let end = 4 + payload.chars().count();
        let line = expected.rope.char_to_line(end);
        let column = expected.char_idx_to_display_col(line, end - expected.rope.line_to_char(line));
        handle_paste(&mut tabs, payload).unwrap();
        assert_eq!(tabs.active_editor().rope, expected.rope);
        assert_eq!(tabs.active_editor().viewport.cursor_pos, (line, column));
        assert!(!tabs.active_editor().viewport.manual_scroll);
        assert!(tabs.active_editor().mark_anchor.is_none());
        assert_eq!(tabs.active_editor().undo_manager.undo_stack.len(), 1);
        assert_eq!(tabs.clipboard, vec!["previous cut"]);
        assert_eq!(tabs.last_cut_line, None);
        tabs.undo();
        assert_eq!(tabs.active_editor().rope.to_string(), original);
        assert_eq!(tabs.active_editor().viewport.cursor_pos, (0, 4));
        tabs.redo();
        assert_eq!(tabs.active_editor().rope, expected.rope);
    }
}

#[test]
fn empty_and_read_only_pastes_do_not_mutate_the_document() {
    for (read_only, name, text) in [
        (false, "notes", ""),
        (true, "notes", "payload"),
        (false, "[Help]", "payload"),
    ] {
        let mut tabs = TabManager::new_for_test();
        tabs.read_only = read_only;
        tabs.needs_redraw = false;
        tabs.active_editor_mut().display_name = name.into();
        handle_paste(&mut tabs, text).unwrap();
        assert_eq!(tabs.active_editor().rope.len_chars(), 0);
        assert!(!tabs.active_editor().modified);
        assert!(tabs.active_editor().undo_manager.undo_stack.is_empty());
        assert!(!tabs.needs_redraw);
    }
}

#[test]
fn paste_never_activates_menus_confirmations_or_hex_commands() {
    for mode in [
        InputMode::ConfirmQuit,
        InputMode::ConfirmOpen,
        InputMode::ConfirmOverwrite,
        InputMode::ConfirmCloseTab,
        InputMode::ConfirmExecute,
        InputMode::ReplaceConfirm,
        InputMode::OptionsMenu,
        InputMode::FindOptionsMenu,
        InputMode::HexView,
    ] {
        let mut tabs = TabManager::new_for_test();
        tabs.input_mode = mode.clone();
        tabs.needs_redraw = false;
        let config = tabs.config.clone();
        handle_paste(&mut tabs, "ynYNA\r\n\u{11}\u{13}\u{1b}").unwrap();
        assert_eq!(tabs.input_mode, mode);
        assert_eq!(tabs.config, config);
        assert!(tabs.pending_command.is_none());
        assert_eq!(tabs.active_editor().rope.len_chars(), 0);
        assert!(tabs.active_editor().undo_manager.undo_stack.is_empty());
        assert!(!tabs.needs_redraw);
    }
}

#[test]
fn single_line_prompts_append_text_without_submitting_or_running_commands() {
    for mode in [
        InputMode::EnteringFilename,
        InputMode::EnteringSaveAs,
        InputMode::OpenFileCurrentTab,
        InputMode::OpenFileNewTab,
        InputMode::ExecuteCommand,
        InputMode::FuzzyFinder,
        InputMode::GoToLine,
    ] {
        let mut tabs = TabManager::new_for_test();
        tabs.input_mode = mode.clone();
        let (text, expected) = if mode == InputMode::GoToLine {
            ("12a٣4\n", "124")
        } else {
            ("Hé界\r\n\tX\u{1b}", "Hé界X")
        };
        handle_paste(&mut tabs, text).unwrap();
        let actual = match mode {
            InputMode::FuzzyFinder => &tabs.fuzzy_query,
            InputMode::GoToLine => &tabs.active_editor().search.goto_line_buffer,
            _ => &tabs.input_buffer,
        };
        assert_eq!(actual, expected);
        assert_eq!(tabs.input_mode, mode);
        assert!(tabs.pending_command.is_none());
        assert_eq!(tabs.active_editor().rope.len_chars(), 0);
        assert!(tabs.needs_redraw);
    }
}

#[test]
fn find_and_replace_receive_the_complete_pasted_query() {
    let mut tabs = TabManager::new_for_test();
    tabs.active_editor_mut().rope = Rope::from_str("alpha alphabet\nalpha\n");
    tabs.start_find();
    tabs.active_editor_mut().search.search_buffer = "al".into();
    handle_paste(&mut tabs, "pha\n").unwrap();
    assert_eq!(tabs.input_mode, InputMode::Find);
    assert_eq!(tabs.active_editor().search.search_buffer, "alpha");
    assert_eq!(
        tabs.active_editor().search.search_matches,
        vec![(0, 0), (0, 6), (1, 0)]
    );
    for phase in [ReplacePhase::FindPattern, ReplacePhase::ReplaceWith] {
        tabs.start_replace();
        tabs.active_editor_mut().search.replace_phase = phase.clone();
        handle_paste(&mut tabs, "界value\r\n").unwrap();
        let search = &tabs.active_editor().search;
        let field = if phase == ReplacePhase::FindPattern {
            &search.search_buffer
        } else {
            &search.replace_buffer
        };
        assert_eq!(field, "界value");
        assert_eq!(tabs.input_mode, InputMode::Replace);
        assert_eq!(search.replace_phase, phase);
    }
    assert!(tabs.active_editor().undo_manager.undo_stack.is_empty());
}

#[test]
fn large_and_verbatim_pastes_are_single_literal_edits() {
    let mut tabs = TabManager::new_for_test();
    tabs.input_mode = InputMode::VerbatimInput;
    let text = "    界e\u{301}\tdata\r\n".repeat(30_000);
    handle_paste(&mut tabs, &text).unwrap();
    assert_eq!(tabs.input_mode, InputMode::Normal);
    assert_eq!(tabs.active_editor().rope.to_string(), text);
    assert_eq!(tabs.active_editor().undo_manager.undo_stack.len(), 1);
    tabs.undo();
    assert_eq!(tabs.active_editor().rope.len_chars(), 0);
}
