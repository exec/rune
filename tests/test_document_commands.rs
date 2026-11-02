use rune::{Document, Selection};

fn document(text: &str, selection: Selection) -> Document {
    let mut doc = Document::new();
    doc.insert_text(text).unwrap();
    doc.set_selection(selection).unwrap();
    doc.take_changed_lines();
    doc
}

#[test]
fn indentation_matches_individual_edits_across_dense_sparse_and_directed_selections() {
    for count in [1, 31, 32, 33, 100] {
        for ending in ["\n", "\r\n"] {
            for long in [false, true] {
                let text = (0..count)
                    .map(|i| {
                        format!(
                            "{}{}{ending}",
                            ["    界é", "\t🙂", "plain", "  "][i % 4],
                            if long { "x".repeat(200) } else { String::new() }
                        )
                    })
                    .collect::<String>();
                let rope = ropey::Rope::from_str(&text);
                for (anchor, caret) in [
                    (0, rope.len_chars()),
                    (rope.len_chars(), 1),
                    (1, 1),
                    (0, rope.line_to_char(1)),
                    (rope.len_chars(), rope.len_chars()),
                ] {
                    for outdent in [false, true] {
                        let selection = Selection { anchor, caret };
                        let mut expected = rope.clone();
                        let mut expected_selection = selection;
                        let range = selection.range();
                        let first = rope.char_to_line(range.start);
                        let last = rope.char_to_line(if range.is_empty() {
                            range.end
                        } else {
                            range.end - 1
                        });
                        for line in (first..=last).rev() {
                            let start = expected.line_to_char(line);
                            let removed = if outdent {
                                if expected.line(line).chars().next() == Some('\t') {
                                    1
                                } else {
                                    expected
                                        .line(line)
                                        .chars()
                                        .take(4)
                                        .take_while(|c| *c == ' ')
                                        .count()
                                }
                            } else {
                                0
                            };
                            let added = if outdent { 0 } else { 4 };
                            expected.remove(start..start + removed);
                            if !outdent {
                                expected.insert(start, "    ");
                            }
                            for position in [
                                &mut expected_selection.anchor,
                                &mut expected_selection.caret,
                            ] {
                                if *position >= start {
                                    *position = *position - removed.min(*position - start) + added;
                                }
                            }
                        }
                        let mut doc = document(&text, selection);
                        let changed = doc.indent_lines(4, outdent);
                        assert_eq!(*doc.text(), expected);
                        assert_eq!(doc.selection(), expected_selection);
                        if changed {
                            assert_eq!(doc.take_changed_lines(), Some(first..last + 1));
                            assert!(doc.undo());
                            assert_eq!(*doc.text(), rope);
                            assert_eq!(doc.selection(), selection);
                            assert!(doc.redo());
                            assert_eq!(*doc.text(), expected);
                            assert_eq!(doc.selection(), expected_selection);
                        }
                    }
                }
            }
        }
    }
}

#[test]
fn newline_keeps_crlf_and_copies_only_indentation_before_the_replacement() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("text.rs");
    std::fs::write(&path, "\t  café\r\nnext\r\n").unwrap();
    let mut doc = Document::new();
    doc.load_file(path).unwrap();
    let original = doc.text().clone();
    doc.set_selection(Selection {
        anchor: 7,
        caret: 2,
    })
    .unwrap();
    doc.insert_newline(true);
    assert_eq!(doc.text().to_string(), "\t \r\n\t \r\nnext\r\n");
    assert_eq!(
        doc.selection(),
        Selection {
            anchor: 6,
            caret: 6
        }
    );
    assert!(doc.undo());
    assert_eq!(*doc.text(), original);
    assert_eq!(
        doc.selection(),
        Selection {
            anchor: 7,
            caret: 2
        }
    );
    doc.insert_newline(false);
    assert_eq!(doc.text().to_string(), "\t \r\n\r\nnext\r\n");
}

#[test]
fn comments_preserve_selection_indentation_and_one_step_undo() {
    let text = "    // café\r\n\t// 界🙂\r\nlast";
    let mut doc = document(
        text,
        Selection {
            anchor: ropey::Rope::from_str(text).line_to_char(2),
            caret: 0,
        },
    );
    let original = doc.text().clone();
    let selection = doc.selection();
    assert!(doc.toggle_line_comment());
    assert_eq!(doc.text().to_string(), "    café\r\n\t界🙂\r\nlast");
    assert!(doc.undo());
    assert_eq!(*doc.text(), original);
    assert_eq!(doc.selection(), selection);
    assert!(doc.redo());
    assert!(doc.toggle_line_comment());
    assert_eq!(doc.text().to_string(), "//     café\r\n// \t界🙂\r\nlast");
}

#[test]
fn unsupported_comments_and_unindent_noops_leave_history_and_revision_alone() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("page.html");
    std::fs::write(&path, "<p>hi</p>\n").unwrap();
    let mut doc = Document::new();
    doc.load_file(path).unwrap();
    let revision = doc.revision();
    assert!(!doc.toggle_line_comment());
    assert!(!doc.indent_lines(4, true));
    assert!(!doc.indent_lines(0, false));
    assert_eq!(doc.revision(), revision);
    assert!(!doc.is_modified());
    assert!(!doc.undo());
}

#[test]
fn dense_and_sparse_uncomment_keep_unicode_indentation_and_line_endings() {
    for long in [false, true] {
        let body = if long {
            "x".repeat(200)
        } else {
            "界🙂".into()
        };
        let text = format!("\t\u{3000}  // {body}\r\n").repeat(80);
        let expected = format!("\t\u{3000}  {body}\r\n").repeat(80);
        let selection = Selection {
            anchor: text.chars().count(),
            caret: 0,
        };
        let mut doc = document(&text, selection);
        assert!(doc.toggle_line_comment());
        assert_eq!(doc.text().to_string(), expected);
        assert_eq!(
            doc.selection(),
            Selection {
                anchor: expected.chars().count(),
                caret: 0
            }
        );
        assert!(doc.undo());
        assert_eq!(doc.text().to_string(), text);
        assert_eq!(doc.selection(), selection);
        assert!(doc.redo());
        assert_eq!(doc.text().to_string(), expected);
    }
}
