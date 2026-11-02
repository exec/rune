#![cfg(feature = "terminal")]

use crossterm::event::{
    Event, KeyCode, KeyEvent, KeyEventKind, KeyModifiers, MouseButton, MouseEvent, MouseEventKind,
};
use ratatui::{backend::TestBackend, Terminal};
use ropey::Rope;
use rune::{editor::InputMode, input, tabs::TabManager, ui::draw_ui};

fn key(code: KeyCode) -> Event {
    chord(code, KeyModifiers::NONE)
}

fn chord(code: KeyCode, modifiers: KeyModifiers) -> Event {
    Event::Key(KeyEvent::new(code, modifiers))
}

fn mouse(kind: MouseEventKind, column: u16, row: u16) -> Event {
    Event::Mouse(MouseEvent {
        kind,
        column,
        row,
        modifiers: KeyModifiers::NONE,
    })
}

fn draw(terminal: &mut Terminal<TestBackend>, tabs: &mut TabManager) {
    terminal.draw(|f| draw_ui(f, tabs)).unwrap();
    tabs.needs_redraw = false;
}

fn simulate(
    events: &[Event],
    batch: bool,
    wrap: bool,
    read_only: bool,
    width: u16,
    height: u16,
) -> (TabManager, Terminal<TestBackend>, usize) {
    let mut tabs = TabManager::new_for_test();
    tabs.config.word_wrap = wrap;
    tabs.config.show_line_numbers = true;
    tabs.config.show_whitespace = true;
    tabs.read_only = read_only;
    let e = tabs.active_editor_mut();
    e.rope = Rope::from_str(&("fn alpha() { let 界 = \"e\u{301}\"; }\t // comment\r\n".repeat(98)));
    e.highlighter.set_syntax(Some("Rust"));
    e.mark_anchor = Some((0, 0));
    let mut terminal = Terminal::new(TestBackend::new(width, height)).unwrap();
    draw(&mut terminal, &mut tabs);
    let mut events = events.iter().cloned();
    let mut pending = None;
    let mut frames = 0;
    while let Some(event) = pending.take().or_else(|| events.next()) {
        match event {
            Event::Key(key) if key.kind == KeyEventKind::Press => {
                let quit = if batch {
                    let area = terminal.get_frame().area();
                    let (quit, next) =
                        input::handle_key_batch(&mut tabs, key, (area.width, area.height), || {
                            Ok(events.next())
                        })
                        .unwrap();
                    pending = next;
                    quit
                } else {
                    input::handle_key_event(&mut tabs, key).unwrap()
                };
                if quit {
                    break;
                }
            }
            Event::Paste(text) => input::handle_paste(&mut tabs, &text).unwrap(),
            Event::Mouse(mouse) if tabs.config.mouse_enabled => {
                let area = terminal.get_frame().area();
                tabs.handle_mouse_event(mouse, area.height as usize);
            }
            Event::Resize(width, height) => {
                terminal.backend_mut().resize(width, height);
                tabs.needs_redraw = true;
            }
            _ => {}
        }
        if tabs.needs_redraw {
            draw(&mut terminal, &mut tabs);
            frames += 1;
        }
    }
    (tabs, terminal, frames)
}

fn equivalent(events: &[Event], wrap: bool, read_only: bool, width: u16, height: u16) {
    let (mut serial, mut old_terminal, _) = simulate(events, false, wrap, read_only, width, height);
    let (mut batch, mut new_terminal, _) = simulate(events, true, wrap, read_only, width, height);
    assert_eq!(serial.input_mode, batch.input_mode);
    assert_eq!(serial.status_message, batch.status_message);
    let old = old_terminal.backend().buffer();
    let new = new_terminal.backend().buffer();
    assert_eq!(old.area, new.area);
    // Padding beneath wide glyphs is invisible and can retain different draw histories.
    assert!(
        old.diff(new).is_empty(),
        "visible cell differences: {:?}",
        old.diff(new)
    );
    assert_eq!(
        old_terminal.get_cursor_position().unwrap(),
        new_terminal.get_cursor_position().unwrap()
    );
    let a = serial.active_editor();
    let b = batch.active_editor();
    assert_eq!(a.viewport.viewport_offset, b.viewport.viewport_offset);
    assert_eq!(a.viewport.wrapped_offset, b.viewport.wrapped_offset);
    assert_eq!(a.viewport.manual_scroll, b.viewport.manual_scroll);
    assert_eq!(a.mark_anchor, b.mark_anchor);
    // Every keystroke keeps its original undo boundary and cursor snapshot.
    for _ in 0..105 {
        assert_eq!(serial.active_editor().rope, batch.active_editor().rope);
        assert_eq!(
            serial.active_editor().viewport.cursor_pos,
            batch.active_editor().viewport.cursor_pos
        );
        serial.undo();
        batch.undo();
    }
    for _ in 0..105 {
        serial.redo();
        batch.redo();
        assert_eq!(serial.active_editor().rope, batch.active_editor().rope);
        assert_eq!(
            serial.active_editor().viewport.cursor_pos,
            batch.active_editor().viewport.cursor_pos
        );
    }
}

#[test]
fn burst_edits_and_reversing_navigation_match_serial_frames_and_undo() {
    let mut events = Vec::new();
    for code in [KeyCode::Down, KeyCode::Up, KeyCode::Right, KeyCode::Left] {
        events.extend(std::iter::repeat_n(key(code), 130));
    }
    for _ in 0..12 {
        events.extend([
            key(KeyCode::End),
            key(KeyCode::Char('界')),
            key(KeyCode::Char('\u{301}')),
            key(KeyCode::Tab),
            key(KeyCode::Backspace),
            key(KeyCode::Delete),
            key(KeyCode::Enter),
            key(KeyCode::Char('x')),
            key(KeyCode::Home),
            key(KeyCode::PageDown),
            key(KeyCode::PageUp),
        ]);
    }
    for wrap in [false, true] {
        for read_only in [false, true] {
            for (width, height) in [(20, 4), (80, 12), (0, 0)] {
                equivalent(&events, wrap, read_only, width, height);
            }
        }
    }
}

#[test]
fn queued_mode_changes_mouse_and_resizes_match_serial_rendering() {
    let mut events = vec![key(KeyCode::Right); 140];
    events.extend([
        Event::Paste("literal \t界\r\ntext".into()),
        mouse(MouseEventKind::Down(MouseButton::Left), 12, 3),
        key(KeyCode::Char('A')),
        Event::Resize(36, 9),
        mouse(MouseEventKind::ScrollDown, 20, 4),
        mouse(MouseEventKind::Drag(MouseButton::Left), 16, 3),
        key(KeyCode::Char('B')),
        chord(KeyCode::Char('f'), KeyModifiers::CONTROL),
        Event::Paste("al".into()),
        key(KeyCode::Char('a')),
        key(KeyCode::Char('l')),
        key(KeyCode::Esc),
        chord(KeyCode::Char('p'), KeyModifiers::CONTROL),
        key(KeyCode::Char('u')),
        key(KeyCode::Esc),
        chord(KeyCode::Char('q'), KeyModifiers::CONTROL),
        key(KeyCode::Esc),
        key(KeyCode::Char('C')),
    ]);
    for wrap in [false, true] {
        equivalent(&events, wrap, false, 80, 12);
    }
}

#[test]
fn barriers_are_returned_untouched_and_never_consume_the_following_event() {
    let mut release = KeyEvent::new(KeyCode::Char('r'), KeyModifiers::NONE);
    release.kind = KeyEventKind::Release;
    let mut repeat = release;
    repeat.kind = KeyEventKind::Repeat;
    for barrier in [
        key(KeyCode::Esc),
        chord(KeyCode::Char('q'), KeyModifiers::CONTROL),
        chord(KeyCode::Char('t'), KeyModifiers::CONTROL),
        Event::Key(release),
        Event::Key(repeat),
        Event::Resize(30, 10),
        Event::FocusLost,
        Event::Paste("preserve paste event".into()),
        mouse(MouseEventKind::Moved, 4, 5),
    ] {
        let mut tabs = TabManager::new_for_test();
        let mut events = [
            key(KeyCode::Char('a')),
            barrier.clone(),
            key(KeyCode::Char('z')),
        ]
        .into_iter();
        let (quit, pending) = input::handle_key_batch(
            &mut tabs,
            KeyEvent::new(KeyCode::Char('x'), KeyModifiers::NONE),
            (80, 12),
            || Ok(events.next()),
        )
        .unwrap();
        assert!(!quit);
        assert_eq!(pending, Some(barrier));
        assert_eq!(tabs.active_editor().rope.to_string(), "xa");
        assert_eq!(events.next(), Some(key(KeyCode::Char('z'))));
    }
}

#[test]
fn isolated_keys_modes_and_quit_never_wait_for_additional_input() {
    let mut tabs = TabManager::new_for_test();
    let key = KeyEvent::new(KeyCode::Char('x'), KeyModifiers::NONE);
    let mut polls = 0;
    input::handle_key_batch(&mut tabs, key, (80, 12), || {
        polls += 1;
        Ok(None)
    })
    .unwrap();
    assert_eq!(polls, 1);
    assert_eq!(tabs.active_editor().rope.to_string(), "x");
    tabs.input_mode = InputMode::Find;
    input::handle_key_batch(&mut tabs, key, (80, 12), || {
        panic!("mode input must be drawn first")
    })
    .unwrap();
    let mut empty = TabManager::new_for_test();
    let (quit, pending) = input::handle_key_batch(
        &mut empty,
        KeyEvent::new(KeyCode::Char('q'), KeyModifiers::CONTROL),
        (80, 12),
        || panic!("quit must stop consuming events"),
    )
    .unwrap();
    assert!(quit);
    assert!(pending.is_none());
}

#[test]
fn batches_are_bounded_and_poll_errors_propagate_without_discarding_edits() {
    let mut tabs = TabManager::new_for_test();
    let pressed = KeyEvent::new(KeyCode::Char('x'), KeyModifiers::NONE);
    input::handle_key_batch(&mut tabs, pressed, (80, 12), || {
        Ok(Some(Event::Key(pressed)))
    })
    .unwrap();
    assert!(tabs.active_editor().rope.len_chars() <= 256);
    assert!(tabs.active_editor().rope.len_chars() >= 1);
    let before = tabs.active_editor().rope.len_chars();
    let error = input::handle_key_batch(&mut tabs, pressed, (80, 12), || {
        Err(std::io::Error::other("poll failed"))
    })
    .unwrap_err();
    assert!(error.to_string().contains("poll failed"));
    assert_eq!(tabs.active_editor().rope.len_chars(), before + 1);
}

#[test]
fn slow_streams_yield_before_the_event_count_limit() {
    let mut tabs = TabManager::new_for_test();
    let pressed = KeyEvent::new(KeyCode::Char('x'), KeyModifiers::NONE);
    let mut polls = 0;
    input::handle_key_batch(&mut tabs, pressed, (80, 12), || {
        polls += 1;
        std::thread::sleep(std::time::Duration::from_millis(10));
        Ok(Some(Event::Key(pressed)))
    })
    .unwrap();
    assert!(
        polls <= 1,
        "the elapsed-time budget must stop a slow stream"
    );
    assert!(tabs.active_editor().rope.len_chars() <= 2);
}
