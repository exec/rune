#![cfg(feature = "terminal")]

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use ropey::Rope;
use rune::{
    editor::InputMode,
    input::handle_key_event,
    tabs::{AfterSave, TabManager},
};

fn shortcuts() -> [(KeyEvent, bool); 6] {
    [
        (KeyEvent::new(KeyCode::Left, KeyModifiers::ALT), false),
        (KeyEvent::new(KeyCode::Right, KeyModifiers::ALT), true),
        (KeyEvent::new(KeyCode::Char(','), KeyModifiers::ALT), false),
        (KeyEvent::new(KeyCode::Char('.'), KeyModifiers::ALT), true),
        (KeyEvent::new(KeyCode::PageUp, KeyModifiers::CONTROL), false),
        (
            KeyEvent::new(KeyCode::PageDown, KeyModifiers::CONTROL),
            true,
        ),
    ]
}

fn tabs() -> TabManager {
    let mut tabs = TabManager::new_for_test();
    tabs.new_tab();
    tabs.new_tab();
    for (index, editor) in tabs.tabs.iter_mut().enumerate() {
        editor.rope = Rope::from_str(&format!("buffer {index}\n"));
        editor.viewport.cursor_pos = (0, index);
        editor.mark_anchor = Some((0, 0));
        editor.modified = true;
    }
    tabs
}

#[test]
fn shortcuts_switch_and_wrap_without_editing_buffers() {
    for mode in [
        InputMode::Normal,
        InputMode::Find,
        InputMode::FindOptionsMenu,
        InputMode::Replace,
        InputMode::ReplaceConfirm,
        InputMode::GoToLine,
        InputMode::HexView,
        InputMode::ConfirmOpen,
        InputMode::ConfirmOverwrite,
        InputMode::EnteringFilename,
        InputMode::EnteringSaveAs,
    ] {
        for (key, forward) in shortcuts() {
            for active in 0..3 {
                let mut tabs = tabs();
                tabs.active_tab = active;
                tabs.input_mode = mode.clone();
                tabs.needs_redraw = false;
                tabs.input_buffer = "unfinished filename".into();
                tabs.pending_after_save = Some(AfterSave::ContinueQuit);
                tabs.status_message = "prompt".into();
                assert!(!handle_key_event(&mut tabs, key).unwrap());
                assert_eq!(tabs.active_tab, (active + if forward { 1 } else { 2 }) % 3);
                assert_eq!(tabs.input_mode, InputMode::Normal);
                assert!(tabs.needs_redraw);
                if mode != InputMode::Normal {
                    assert!(tabs.input_buffer.is_empty());
                    assert!(tabs.pending_after_save.is_none());
                    assert!(tabs.status_message.is_empty());
                }
                for (index, editor) in tabs.tabs.iter().enumerate() {
                    assert_eq!(editor.rope.to_string(), format!("buffer {index}\n"));
                    assert_eq!(editor.viewport.cursor_pos, (0, index));
                    assert_eq!(editor.mark_anchor, Some((0, 0)));
                    assert!(editor.modified);
                }
            }
        }
    }
}

#[test]
fn single_tab_shortcuts_preserve_the_prompt() {
    for mode in [InputMode::Find, InputMode::EnteringFilename] {
        for (key, _) in shortcuts() {
            let mut tabs = TabManager::new_for_test();
            tabs.input_mode = mode.clone();
            tabs.input_buffer = "unfinished".into();
            tabs.pending_after_save = Some(AfterSave::ContinueQuit);
            handle_key_event(&mut tabs, key).unwrap();
            assert_eq!(tabs.active_tab, 0);
            assert_eq!(tabs.input_mode, mode);
            assert_eq!(tabs.input_buffer, "unfinished");
            assert_eq!(tabs.pending_after_save, Some(AfterSave::ContinueQuit));
        }
    }
}

#[test]
fn shortcuts_do_not_retarget_confirmation_dialogs() {
    for mode in [
        InputMode::ConfirmQuit,
        InputMode::ConfirmCloseTab,
        InputMode::ConfirmExecute,
    ] {
        for (key, _) in shortcuts() {
            let mut tabs = tabs();
            tabs.input_mode = mode.clone();
            tabs.pending_command = Some("do not execute".into());
            assert!(!handle_key_event(&mut tabs, key).unwrap());
            assert_eq!(tabs.active_tab, 2);
            assert_eq!(tabs.input_mode, mode);
            assert_eq!(tabs.pending_command.as_deref(), Some("do not execute"));
            assert_eq!(tabs.tabs.len(), 3);
        }
    }
}

#[test]
fn read_only_and_help_tabs_can_switch_but_verbatim_stays_literal() {
    let mut tabs = tabs();
    tabs.read_only = true;
    tabs.active_editor_mut().display_name = "[Help]".into();
    let (next, _) = shortcuts()[3];
    handle_key_event(&mut tabs, next).unwrap();
    assert_eq!(tabs.active_tab, 0);
    tabs.read_only = false;
    tabs.input_mode = InputMode::VerbatimInput;
    handle_key_event(&mut tabs, next).unwrap();
    assert_eq!(tabs.active_tab, 0);
    assert_eq!(tabs.active_editor().rope.to_string(), ".buffer 0\n");
}
