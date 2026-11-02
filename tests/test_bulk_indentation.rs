#![cfg(feature = "terminal")]

use ropey::Rope;
use rune::tabs::TabManager;

fn original_adjust(text: &str, start: usize, end: usize, width: usize, remove: bool) -> Rope {
    let mut rope = Rope::from_str(text);
    for line in (start..=end).rev() {
        if line >= rope.len_lines() {
            continue;
        }
        let first = rope.line_to_char(line);
        if remove {
            let mut count = 0;
            for ch in rope.line(line).chars() {
                if ch == ' ' && count < width {
                    count += 1;
                } else if ch == '\t' && count == 0 {
                    count = 1;
                    break;
                } else {
                    break;
                }
            }
            if count > 0 {
                rope.remove(first..first + count);
            }
        } else {
            rope.insert(first, &" ".repeat(width));
        }
    }
    rope
}

#[test]
fn dense_and_sparse_indentation_match_original_edits_and_undo() {
    let lines = [
        "",
        "    abc",
        "\t界e\u{301}",
        " a",
        "\t\tz",
        "\u{3000}wide",
        " \tx",
        "plain",
    ];
    for count in [1, 31, 32, 33, 100] {
        for ending in ["\n", "\r\n"] {
            for long in [false, true] {
                let text: String = (0..count)
                    .map(|i| {
                        format!(
                            "{}{}{ending}",
                            lines[i % lines.len()],
                            if long { "x".repeat(200) } else { String::new() }
                        )
                    })
                    .collect();
                let total = Rope::from_str(&text).len_lines();
                for start in [0, total / 3, total - 1] {
                    for end in [start, (start + 1).min(total - 1), total - 1, total + 2] {
                        for width in [0, 1, 4, 8] {
                            for remove in [false, true] {
                                let expected =
                                    original_adjust(&text, start, end, width, remove).to_string();
                                let mut tabs = TabManager::new_for_test();
                                tabs.config.tab_width = width;
                                let e = tabs.active_editor_mut();
                                e.rope = Rope::from_str(&text);
                                e.mark_anchor = Some((start, 0));
                                e.viewport.cursor_pos = (end, 0);
                                if remove {
                                    tabs.unindent_lines();
                                } else {
                                    tabs.indent_lines();
                                }
                                assert_eq!(tabs.active_editor().rope.to_string(),expected,"count={count} start={start} end={end} width={width} remove={remove} long={long}");
                                assert!(tabs.active_editor().mark_anchor.is_none());
                                assert_eq!(tabs.active_editor().undo_manager.undo_stack.len(), 1);
                                tabs.undo();
                                assert_eq!(tabs.active_editor().rope.to_string(), text);
                                tabs.redo();
                                assert_eq!(tabs.active_editor().rope.to_string(), expected);
                            }
                        }
                    }
                }
            }
        }
    }
}
