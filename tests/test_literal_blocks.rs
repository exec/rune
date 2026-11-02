use regex::{Regex, RegexBuilder};
use ropey::Rope;
use rune::search::SearchState;

// Independent line-at-a-time reference for the existing literal-search contract.
fn reference(
    rope: &Rope,
    regex: &Regex,
    resume: usize,
    limit: usize,
) -> (Vec<(usize, usize, usize)>, bool) {
    if resume > rope.len_chars() {
        return (Vec::new(), false);
    }
    let first = rope.char_to_line(resume);
    let mut start = rope.line_to_char(first);
    let mut result = Vec::new();
    for (offset, line) in rope.lines_at(first).enumerate() {
        let text = line.to_string();
        let text = text.trim_end_matches('\n');
        let mut byte = line
            .char_to_byte(resume.saturating_sub(start))
            .min(text.len());
        start += line.len_chars();
        while let Some(found) = regex.find_at(text, byte) {
            if result.len() == limit {
                return (result, true);
            }
            result.push((
                first + offset,
                line.byte_to_char(found.start()),
                text[found.start()..found.end()].chars().count(),
            ));
            byte = found.start() + text[found.start()..].chars().next().unwrap().len_utf8();
        }
    }
    (result, false)
}

#[test]
fn block_literals_match_the_line_reference_for_unicode_resumes_and_limits() {
    for text in [
        "",
        "aaAAaaa\nlast",
        "kKK kelvin KELVIN\r\nΣσςſsS\nİiıI",
        "界🙂e\u{301}\t\0.a*\\b\nend",
        "a\rb\r\na\u{b}b\u{c}a\u{85}b\u{2028}a\u{2029}b",
    ] {
        let rope = Rope::from_str(text);
        for sensitive in [false, true] {
            for pattern in [
                "a", "aa", "k", "kelvin", "σ", "s", "İ", "界🙂", "e\u{301}", ".a*\\", "\0", "\r",
                "a\rb", "a\nb", "\u{2028}",
            ] {
                let regex = RegexBuilder::new(&regex::escape(pattern))
                    .case_insensitive(!sensitive)
                    .build()
                    .unwrap();
                let mut search = SearchState::default();
                search.search_buffer = pattern.into();
                search.case_sensitive = sensitive;
                for resume in 0..=rope.len_chars() + 1 {
                    for limit in [0, 1, 2, usize::MAX] {
                        let expected = reference(&rope, &regex, resume, limit);
                        assert_eq!(
                            search.match_spans(&rope, resume, limit),
                            expected.0,
                            "{text:?} {pattern:?} {sensitive} {resume} {limit}"
                        );
                        assert_eq!(search.search_matches_truncated, expected.1);
                    }
                }
            }
        }
    }
}

#[test]
fn matches_crossing_block_boundaries_are_neither_lost_nor_duplicated() {
    for (pattern, content) in [
        ("needle", "needle"),
        ("kelvin", "KELVIN"),
        ("σs界", "ςſ界"),
        ("🙂e\u{301}", "🙂e\u{301}"),
    ] {
        let mut search = SearchState::default();
        search.search_buffer = pattern.into();
        let regex = RegexBuilder::new(&regex::escape(pattern))
            .case_insensitive(true)
            .build()
            .unwrap();
        for padding in [
            64_000, 65_525, 65_530, 65_534, 65_535, 65_536, 65_540, 65_550, 66_000, 66_500, 67_000,
            130_990,
        ] {
            let text = "x".repeat(padding) + content + &"\nline\r\n".repeat(12_000) + content;
            let rope = Rope::from_str(&text);
            for resume in [0, padding, padding + 1] {
                for limit in [0, 1, 2, usize::MAX] {
                    let expected = reference(&rope, &regex, resume, limit);
                    assert_eq!(
                        search.match_spans(&rope, resume, limit),
                        expected.0,
                        "{pattern:?} padding={padding}, resume={resume}, limit={limit}"
                    );
                    assert_eq!(search.search_matches_truncated, expected.1);
                }
            }
        }
    }
}

#[test]
fn dense_overlapping_matches_keep_the_limit_and_truncation_contract() {
    let rope = Rope::from_str(&"a".repeat(160_000));
    let regex = Regex::new("aaaa").unwrap();
    let mut search = SearchState::default();
    search.search_buffer = "aaaa".into();
    for resume in [0, 65_530, 65_536, 159_990, 160_000] {
        for limit in [0, 1, 10_000, usize::MAX] {
            let expected = reference(&rope, &regex, resume, limit);
            assert_eq!(search.match_spans(&rope, resume, limit), expected.0);
            assert_eq!(search.search_matches_truncated, expected.1);
        }
    }
}

#[test]
fn edits_do_not_reuse_stale_block_contents() {
    let pattern = "needle".to_owned();
    let mut rope = Rope::from_str(&("x".repeat(400_000) + &pattern + "\n" + &pattern));
    let mut search = SearchState::default();
    search.search_buffer = pattern.clone();
    let regex = Regex::new(&regex::escape(&pattern)).unwrap();
    for _ in 0..2 {
        let expected = reference(&rope, &regex, 0, usize::MAX);
        assert_eq!(search.match_spans(&rope, 0, usize::MAX), expected.0);
        rope.insert(0, "界\r\n");
    }
}
