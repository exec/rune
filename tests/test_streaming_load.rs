use ropey::Rope;
use rune::editor::Editor;

#[test]
fn streaming_load_preserves_utf8_and_line_endings_at_read_boundaries() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("boundary.txt");
    for padding in [
        0, 1, 2046, 2047, 2048, 2049, 65533, 65534, 65535, 65536, 65537,
    ] {
        for suffix in [
            "\r\nlast",
            "界🙂e\u{301}\r\nlast",
            "界e\u{301}\nlast",
            "\r",
            "\u{85}\u{2028}\u{2029}last",
        ] {
            let text = "x".repeat(padding) + suffix;
            std::fs::write(&path, &text).unwrap();
            let mut editor = Editor::new_for_test();
            editor.load_file(path.clone()).unwrap();
            assert_eq!(editor.rope.to_string(), text);
            let reference = Rope::from_str(&text);
            assert_eq!(editor.rope.len_lines(), reference.len_lines());
            assert_eq!(editor.rope.len_chars(), reference.len_chars());
            for line in 0..reference.len_lines() {
                assert_eq!(editor.rope.line_to_char(line), reference.line_to_char(line));
                assert_eq!(editor.rope.line_to_byte(line), reference.line_to_byte(line));
            }
            assert_eq!(editor.saved_content.as_ref().unwrap(), &editor.rope);
            editor.goto_end();
            editor.insert_newline(false);
            let newline = if text.contains("\r\n") { "\r\n" } else { "\n" };
            assert_eq!(editor.rope.to_string(), text + newline);
        }
    }
    for text in ["", "\u{feff}text\0\n", "a\r\nb\nc"] {
        std::fs::write(&path, text).unwrap();
        let mut editor = Editor::new_for_test();
        editor.load_file(path.clone()).unwrap();
        assert_eq!(editor.rope.to_string(), text);
        assert_eq!(editor.saved_content.as_ref().unwrap(), &editor.rope);
    }
}

#[test]
fn malformed_utf8_after_a_valid_prefix_preserves_the_entire_editor() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("invalid.txt");
    for padding in [0, 2047, 65535, 65536, 131071] {
        for invalid in [
            &[0xff][..],
            &[0xc0, 0x80],
            &[0xe2, 0x28, 0xa1],
            &[0xed, 0xa0, 0x80],
            &[0xf0, 0x9f],
            &[0xf4, 0x90, 0x80, 0x80],
        ] {
            let mut data = vec![b'a'; padding];
            data.extend_from_slice(invalid);
            std::fs::write(&path, data).unwrap();
            let mut editor = Editor::new_for_test();
            editor.rope = Rope::from_str("original unsaved text\nsecond");
            editor.saved_content = Some(Rope::from_str("original disk text"));
            editor.file_path = Some(directory.path().join("original.rs"));
            editor.highlighter.set_syntax(Some("Rust"));
            editor.syntax_name = Some("Rust".into());
            editor.insert_char('x');
            editor.viewport.cursor_pos = (1, 2);
            editor.viewport.viewport_offset = (1, 3);
            editor.viewport.wrapped_offset = 2;
            editor.viewport.manual_scroll = true;
            editor.mark_anchor = Some((0, 1));
            editor.search.search_buffer = "unsaved".into();
            editor.toggle_hex_view();
            let before = editor.rope.to_string();
            let buffer_id = editor.buffer_id;
            let generation = editor.dirty_generation;
            let old_path = editor.file_path.clone();
            let error = editor.load_file(path.clone()).unwrap_err();
            assert_eq!(
                error.to_string(),
                format!("{}: binary or non-UTF-8 file", path.display())
            );
            assert_eq!(editor.rope.to_string(), before);
            assert_eq!(editor.buffer_id, buffer_id);
            assert_eq!(editor.dirty_generation, generation);
            assert_eq!(editor.file_path, old_path);
            assert_eq!(
                editor.saved_content.as_ref().unwrap().to_string(),
                "original disk text"
            );
            assert_eq!(editor.viewport.cursor_pos, (1, 2));
            assert_eq!(editor.viewport.viewport_offset, (1, 3));
            assert_eq!(editor.viewport.wrapped_offset, 2);
            assert!(editor.viewport.manual_scroll);
            assert_eq!(editor.mark_anchor, Some((0, 1)));
            assert_eq!(editor.search.search_buffer, "unsaved");
            assert_eq!(editor.syntax_name.as_deref(), Some("Rust"));
            assert_eq!(editor.undo_manager.undo_stack.len(), 1);
            assert!(editor.hex_state.is_some());
            assert!(editor.modified);
        }
    }
}

#[test]
fn successful_load_resets_old_state_and_detects_filename_syntax() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("script.py");
    let text = "#!/usr/bin/env python\nprint('界')\n";
    std::fs::write(&path, text).unwrap();
    let mut editor = Editor::new_for_test();
    editor.rope = Rope::from_str("previous file");
    editor.insert_char('x');
    editor.viewport.viewport_offset = (4, 5);
    editor.viewport.manual_scroll = true;
    editor.mark_anchor = Some((0, 0));
    editor.search.search_buffer = "previous".into();
    editor.toggle_hex_view();
    let id = editor.buffer_id;
    editor.load_file(path.clone()).unwrap();
    assert_ne!(editor.buffer_id, id);
    assert_eq!(editor.rope.to_string(), text);
    assert_eq!(editor.file_path, Some(path));
    assert_eq!(editor.display_name, "script.py");
    assert_eq!(editor.syntax_name.as_deref(), Some("Python"));
    assert_eq!(editor.viewport.cursor_pos, (0, 0));
    assert_eq!(editor.viewport.viewport_offset, (0, 0));
    assert!(!editor.viewport.manual_scroll);
    assert!(editor.mark_anchor.is_none());
    assert!(editor.search.search_buffer.is_empty());
    assert!(editor.undo_manager.undo_stack.is_empty());
    assert!(editor.hex_state.is_none());
    assert!(!editor.modified);
}

#[test]
fn open_errors_keep_their_io_kind_and_leave_the_buffer_intact() {
    let directory = tempfile::tempdir().unwrap();
    let mut editor = Editor::new_for_test();
    editor.insert_char('x');
    let id = editor.buffer_id;
    let error = editor
        .load_file(directory.path().join("missing"))
        .unwrap_err();
    assert_eq!(
        error.downcast_ref::<std::io::Error>().unwrap().kind(),
        std::io::ErrorKind::NotFound
    );
    assert_eq!(editor.rope.to_string(), "x");
    assert_eq!(editor.buffer_id, id);
    assert!(editor.modified);
}
