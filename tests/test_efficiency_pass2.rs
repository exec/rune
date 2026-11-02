#![cfg(feature = "terminal")]

use ropey::Rope;
use rune::editor::{char_display_width, Editor};
use rune::tabs::TabManager;

fn reference_word_move(rope: &Rope, line: usize, col: usize, right: bool) -> (usize, usize) {
    if !right && col == 0 {
        return if line > 0 {
            (line - 1, rune::editor::line_display_width(rope, line - 1))
        } else {
            (line, 0)
        };
    }
    let chars: Vec<_> = rope.line(line).chars().filter(|&c| c != '\n').collect();
    let mut columns = vec![0];
    for &ch in &chars {
        columns.push(columns.last().unwrap() + char_display_width(ch, *columns.last().unwrap()));
    }
    let mut index = columns
        .iter()
        .position(|&width| width >= col)
        .unwrap_or(chars.len());
    if right {
        while index < chars.len() && !chars[index].is_whitespace() {
            index += 1;
        }
        while index < chars.len() && chars[index].is_whitespace() {
            index += 1;
        }
        if index == chars.len() && line + 1 < rope.len_lines() {
            return (line + 1, 0);
        }
    } else {
        while index > 0 && chars[index - 1].is_whitespace() {
            index -= 1;
        }
        while index > 0 && !chars[index - 1].is_whitespace() {
            index -= 1;
        }
    }
    (line, columns[index])
}

#[test]
fn streamed_word_navigation_matches_original_column_semantics() {
    for text in [
        "hello world\nnext",
        "界foo\t βbar\r\nend",
        "e\u{301} word\0\t世界\n",
        "  \t\n\n",
        "word\u{2028}next",
    ] {
        let mut editor = Editor::new_buffer();
        editor.rope = Rope::from_str(text);
        for line in 0..editor.rope.len_lines() {
            for col in 0..=rune::editor::line_display_width(&editor.rope, line) + 3 {
                for right in [true, false] {
                    let expected = reference_word_move(&editor.rope, line, col, right);
                    editor.viewport.cursor_pos = (line, col);
                    if right {
                        editor.move_word_right();
                    } else {
                        editor.move_word_left();
                    }
                    assert_eq!(
                        editor.viewport.cursor_pos, expected,
                        "{text:?}, {line}:{col}, right={right}"
                    );
                }
            }
        }
    }
}

#[test]
fn bracket_streams_preserve_nested_multiline_unicode_matching() {
    for text in [
        "(a[界]{\nβ})",
        "\tα[\n界]β",
        "((unterminated)",
        "missing]",
        "{([x])}",
    ] {
        let chars: Vec<_> = text.chars().collect();
        for (position, &ch) in chars.iter().enumerate() {
            let (other, right) = match ch {
                '(' => (')', true),
                '[' => (']', true),
                '{' => ('}', true),
                ')' => ('(', false),
                ']' => ('[', false),
                '}' => ('{', false),
                _ => continue,
            };
            let mut editor = Editor::new_buffer();
            editor.rope = Rope::from_str(text);
            let line = editor.rope.char_to_line(position);
            editor.viewport.cursor_pos = (
                line,
                editor.char_idx_to_display_col(line, position - editor.rope.line_to_char(line)),
            );
            let original = editor.viewport.cursor_pos;
            let indices: Vec<_> = if right {
                (position + 1..chars.len()).collect()
            } else {
                (0..position).rev().collect()
            };
            let mut depth = 1;
            let target = indices.into_iter().find(|&i| {
                if chars[i] == ch {
                    depth += 1;
                }
                if chars[i] == other {
                    depth -= 1;
                }
                depth == 0
            });
            let expected = target.map_or(original, |i| {
                let line = editor.rope.char_to_line(i);
                (
                    line,
                    editor.char_idx_to_display_col(line, i - editor.rope.line_to_char(line)),
                )
            });
            editor.viewport.cursor_pos = original;
            editor.match_bracket();
            assert_eq!(
                editor.viewport.cursor_pos, expected,
                "{text:?}, bracket {position}"
            );
        }
    }
}

#[test]
fn untitled_name_lookup_reuses_gaps_and_distinguishes_noncanonical_names() {
    let mut tabs = TabManager::new_for_test();
    for name in [
        "[untitled-02]",
        "[untitled-2]",
        "[untitled-4]",
        "[untitled-2]",
    ] {
        let mut editor = Editor::new_buffer();
        editor.display_name = name.into();
        tabs.tabs.push(editor);
    }
    tabs.new_tab();
    assert_eq!(tabs.active_editor().display_name, "[untitled-3]");
    tabs.tabs.retain(|e| e.display_name != "[untitled-2]");
    tabs.new_tab();
    assert_eq!(tabs.active_editor().display_name, "[untitled-2]");
}
