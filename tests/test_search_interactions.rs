use ropey::Rope;
use rune::{
    editor::{line_display_width, Editor},
    search::SearchState,
};

#[test]
fn first_lookup_matches_full_search_without_changing_ui_truncation() {
    let rope = Rope::from_str("aa AA\n界a\u{301}\r\n\tbeta\u{2028}a");
    for regex in [false, true] {
        for case_sensitive in [false, true] {
            for pattern in ["", "a", "aa", "界", "^", "$", "a*", ".*", "\\b", "["] {
                let mut search = SearchState::default();
                search.search_buffer = pattern.into();
                search.use_regex = regex;
                search.case_sensitive = case_sensitive;
                for resume in 0..=rope.len_chars() + 1 {
                    let all = search.match_spans(&rope, resume, usize::MAX);
                    for truncated in [false, true] {
                        search.search_matches_truncated = truncated;
                        assert_eq!(
                            search.first_match_span(&rope, resume),
                            all.first().copied(),
                            "{pattern:?} regex={regex} case={case_sensitive} resume={resume}"
                        );
                        assert_eq!(search.search_matches_truncated, truncated);
                    }
                    assert_eq!(
                        search.match_spans(&rope, resume, 1),
                        all.iter().take(1).copied().collect::<Vec<_>>()
                    );
                    assert_eq!(search.search_matches_truncated, all.len() > 1);
                }
            }
        }
    }
}

#[test]
fn binary_result_selection_matches_original_linear_selection() {
    for text in [
        "aa aa\nbb\n",
        "a\u{2028}a\n",
        "界 e\u{301}a\r\n\nx",
        "single",
    ] {
        let rope = Rope::from_str(text);
        for regex in [false, true] {
            for pattern in ["a", "^", "$", "a*", ".*", "\\b", "["] {
                let mut search = SearchState::default();
                search.search_buffer = pattern.into();
                search.use_regex = regex;
                let matches = search.find_all_matches(&rope);
                for line in 0..rope.len_lines() {
                    for col in 0..=line_display_width(&rope, line) + 2 {
                        let mut editor = Editor::new_buffer();
                        editor.rope = rope.clone();
                        editor.viewport.cursor_pos = (line, col);
                        editor.search.use_regex = regex;
                        let position = editor.line_col_to_char_idx(line, col);
                        let expected = if matches.is_empty() {
                            None
                        } else {
                            matches
                                .iter()
                                .position(|&(l, c)| rope.line_to_char(l) + c >= position)
                                .or(Some(0))
                        };
                        assert_eq!(editor.perform_find(pattern), expected.is_some());
                        assert_eq!(
                            editor.search.current_match_index, expected,
                            "{text:?} {pattern:?} regex={regex}, cursor={line}:{col}"
                        );
                    }
                }
            }
        }
    }
}
