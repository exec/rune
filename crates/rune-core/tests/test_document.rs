use rune_core::{
    file_io::{SaveError, SaveOptions},
    Document, EditError, Selection,
};

#[test]
fn scalar_positions_preserve_combining_marks_emoji_and_directed_selections() {
    let mut doc = Document::new();
    doc.insert_text("e\u{301}界🙂\r\nlast").unwrap();
    doc.set_selection(Selection {
        anchor: 4,
        caret: 2,
    })
    .unwrap();
    let before = doc.text().clone();
    doc.insert_text("é").unwrap();
    assert_eq!(doc.text().to_string(), "e\u{301}é\r\nlast");
    assert_eq!(
        doc.selection(),
        Selection {
            anchor: 3,
            caret: 3
        }
    );
    assert!(doc.undo());
    assert_eq!(*doc.text(), before);
    assert_eq!(
        doc.selection(),
        Selection {
            anchor: 4,
            caret: 2
        }
    );
    assert!(doc.redo());
    assert_eq!(doc.text().to_string(), "e\u{301}é\r\nlast");
    doc.set_selection(Selection {
        anchor: 1,
        caret: 1,
    })
    .unwrap();
    doc.insert_text("X").unwrap();
    assert_eq!(doc.text().to_string(), "eX\u{301}é\r\nlast");
}

#[test]
fn invalid_edits_and_empty_insertions_leave_revision_selection_and_history_unchanged() {
    let mut doc = Document::new();
    assert_eq!(doc.take_changed_lines(), None);
    doc.insert_text("abc").unwrap();
    let revision = doc.revision();
    for range in [4..4, 2..5, std::ops::Range { start: 2, end: 1 }] {
        assert_eq!(doc.replace_range(range, "X"), Err(EditError::InvalidRange));
    }
    assert_eq!(
        doc.set_selection(Selection {
            anchor: 5,
            caret: 1
        }),
        Err(EditError::InvalidSelection)
    );
    doc.insert_text("").unwrap();
    assert_eq!(doc.revision(), revision);
    assert_eq!(
        doc.selection(),
        Selection {
            anchor: 3,
            caret: 3
        }
    );
    assert!(doc.undo());
    assert_eq!(doc.text().to_string(), "");
    assert!(!doc.undo());
}

#[test]
fn invalidation_coalesces_local_edits_and_handles_crlf_joining_previous_line() {
    let mut doc = Document::new();
    doc.insert_text("a\nb\nc").unwrap();
    doc.take_changed_lines();
    doc.replace_range(2..2, "x").unwrap();
    assert_eq!(doc.take_changed_lines(), Some(1..2));
    doc.replace_range(1..1, "x").unwrap();
    doc.replace_range(6..6, "y").unwrap();
    assert_eq!(doc.take_changed_lines(), Some(0..3));
    let mut doc = Document::new();
    doc.insert_text("a\rb").unwrap();
    doc.take_changed_lines();
    doc.replace_range(2..3, "\nc").unwrap();
    assert_eq!(doc.text().to_string(), "a\r\nc");
    assert_eq!(doc.take_changed_lines(), Some(0..usize::MAX));
}

#[test]
fn load_save_failure_and_search_work_without_settings_or_terminal_state() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("a.rs");
    let mut doc = Document::new();
    doc.insert_text("fn café() {}\nquartz\n").unwrap();
    doc.save(&path, SaveOptions::default()).unwrap();
    assert_eq!(doc.path(), Some(path.as_path()));
    assert_eq!(doc.syntax_name(), Some("Rust"));
    assert!(!doc.is_modified());
    doc.insert_text("X").unwrap();
    let selection = doc.selection();
    let revision = doc.revision();
    assert!(doc.load_file(dir.path().join("missing")).is_err());
    assert_eq!(doc.selection(), selection);
    assert_eq!(doc.revision(), revision);
    assert!(doc.undo());
    std::fs::write(&path, "external").unwrap();
    assert!(matches!(
        doc.save(&path, SaveOptions::default()),
        Err(SaveError::ChangedOnDisk)
    ));
    doc.load_file(path).unwrap();
    assert_eq!(doc.selection(), Selection::default());
    assert!(!doc.undo());
    let mut search = rune_core::search::SearchState::default();
    search.search_buffer = "external".into();
    assert_eq!(search.find_all_match_spans(doc.text()), [(0, 0, 8)]);
}

#[test]
fn edits_match_a_scalar_reference_through_undo_and_redo() {
    let mut doc = Document::new();
    let mut reference = Vec::<char>::new();
    let mut states = vec![String::new()];
    let mut seed = 12345u64;
    for index in 0..80 {
        seed = seed.wrapping_mul(6364136223846793005).wrapping_add(1);
        let start = seed as usize % (reference.len() + 1);
        let end = (start + index % 4).min(reference.len());
        let text = ["a", "界", "\r\n", "e\u{301}", "🙂"][index % 5];
        doc.replace_range(start..end, text).unwrap();
        reference.splice(start..end, text.chars());
        states.push(reference.iter().collect::<String>());
        assert_eq!(doc.text().to_string(), *states.last().unwrap());
    }
    for state in states[..states.len() - 1].iter().rev() {
        assert!(doc.undo());
        assert_eq!(doc.text().to_string(), *state);
    }
    for state in &states[1..] {
        assert!(doc.redo());
        assert_eq!(doc.text().to_string(), *state);
    }
}
