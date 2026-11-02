#![cfg(feature = "terminal")]

use ratatui::{
    backend::TestBackend,
    style::{Color, Style},
    Terminal,
};
use rune::{syntax::SyntaxHighlighter, tabs::TabManager, ui::draw_ui};
use std::path::Path;

const TEXT: &str = "fn sample() { let value = true; } // 42 \"quoted\"\n";

#[test]
fn unrecognized_and_untitled_documents_skip_all_syntax_coloring() {
    for path in [
        None,
        Some("notes.txt"),
        Some("notes.TXT"),
        Some("file.unknown"),
        Some("README"),
    ] {
        let mut h = SyntaxHighlighter::new();
        let syntax = h.detect_syntax(path.map(Path::new), Some("#!/usr/bin/env python"));
        assert!(syntax.is_none());
        h.set_syntax(syntax.as_deref());
        assert_eq!(
            *h.highlight_line(0, TEXT),
            vec![(Style::default(), TEXT.to_owned())]
        );
    }
    let mut h = SyntaxHighlighter::new();
    h.set_syntax(Some("unsupported language"));
    assert_eq!(
        *h.highlight_line(0, TEXT),
        vec![(Style::default(), TEXT.to_owned())]
    );
}

#[test]
fn save_as_switches_cached_rendering_between_recognized_and_plain_text() {
    let directory = tempfile::tempdir().unwrap();
    let source = directory.path().join("source.rs");
    std::fs::write(&source, TEXT).unwrap();
    let mut tabs = TabManager::new_for_test();
    tabs.config.backup_on_save = false;
    tabs.config.show_line_numbers = false;
    tabs.active_editor_mut().load_file(source).unwrap();
    let mut terminal = Terminal::new(TestBackend::new(80, 6)).unwrap();
    terminal.draw(|f| draw_ui(f, &mut tabs)).unwrap();
    assert_eq!(terminal.backend().buffer()[(0, 1)].fg, Color::Magenta);

    for (i, extension) in ["txt", "unknown", "rs", "TXT"].into_iter().enumerate() {
        let destination = directory.path().join(format!("saved-{i}.{extension}"));
        tabs.save_as();
        tabs.input_buffer = destination.to_string_lossy().into_owned();
        assert!(!tabs.finish_filename_input().unwrap());
        assert_eq!(std::fs::read_to_string(destination).unwrap(), TEXT);
        terminal.draw(|f| draw_ui(f, &mut tabs)).unwrap();
        if extension == "rs" {
            assert_eq!(tabs.active_editor().syntax_name.as_deref(), Some("Rust"));
            assert_eq!(terminal.backend().buffer()[(0, 1)].fg, Color::Magenta);
        } else {
            assert!(tabs.active_editor().syntax_name.is_none());
            for x in 0..TEXT.trim_end().len() as u16 {
                assert_eq!(terminal.backend().buffer()[(x, 1)].fg, Color::Reset);
            }
        }
    }
}

#[test]
fn plain_text_retains_search_selection_and_control_stripping() {
    let directory = tempfile::tempdir().unwrap();
    let source = directory.path().join("notes.txt");
    std::fs::write(&source, "if true 42 \"quote\" \u{1b}[31m // comment\n").unwrap();
    let mut tabs = TabManager::new_for_test();
    tabs.config.show_line_numbers = false;
    tabs.active_editor_mut().load_file(source).unwrap();
    let editor = tabs.active_editor_mut();
    editor.perform_find("true");
    editor.mark_anchor = Some((0, 8));
    editor.viewport.cursor_pos = (0, 10);
    let mut terminal = Terminal::new(TestBackend::new(80, 6)).unwrap();
    terminal.draw(|f| draw_ui(f, &mut tabs)).unwrap();
    let buffer = terminal.backend().buffer();
    assert_eq!(buffer[(0, 1)].fg, Color::Reset);
    assert_eq!(buffer[(3, 1)].bg, Color::Red);
    assert_eq!(buffer[(8, 1)].bg, Color::White);
    assert_eq!(buffer[(8, 1)].fg, Color::Black);
    assert_eq!(buffer[(11, 1)].fg, Color::Reset);
    assert!(buffer
        .content
        .iter()
        .all(|cell| !cell.symbol().contains(char::is_control)));
}
