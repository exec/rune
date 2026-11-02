use rune::fuzzy::{fuzzy_filter, fuzzy_filter_prepared, FuzzyCandidate};

fn reference_score(query: &str, candidate: &str) -> Option<i32> {
    let query: Vec<_> = query.to_lowercase().chars().collect();
    if query.is_empty() {
        return Some(0);
    }
    let chars: Vec<_> = candidate.to_lowercase().chars().collect();
    let mut matched = 0;
    let mut score = 0;
    let mut previous = None;
    for (i, &ch) in chars.iter().enumerate() {
        if matched < query.len() && ch == query[matched] {
            score += 10;
            if previous.is_some_and(|last| i == last + 1) {
                score += 5;
            }
            if i == 0 || ['/', '_', '-', '.'].contains(&chars[i - 1]) {
                score += 8;
            }
            previous = Some(i);
            matched += 1;
        }
    }
    (matched == query.len()).then_some(score - candidate.len() as i32)
}

#[test]
fn prepared_pruning_preserves_scores_order_and_unicode_collisions() {
    let alphabet: Vec<_> = "abzZ09_./@`\0\u{80}界😀İſßΣς\u{301}".chars().collect();
    let mut state = 0x12345678u64;
    let mut candidates: Vec<_> = [
        "",
        "@",
        "\u{80}",
        "界",
        "İnput",
        "Straße",
        "ΣΣ",
        "aaaa",
        "ab",
        "ba",
        "module_tail.rs",
    ]
    .iter()
    .enumerate()
    .map(|(i, s)| (i, s.to_string()))
    .collect();
    for i in 0..250 {
        let mut text = String::new();
        for _ in 0..i % 30 {
            state = state.wrapping_mul(6364136223846793005).wrapping_add(1);
            text.push(alphabet[((state >> 32) as usize) % alphabet.len()]);
        }
        candidates.push((i + 1000, text));
    }
    for len in [0, 1, 7, 8, 9, 63, 64, 65, 128] {
        candidates.push((3000 + len, "a".repeat(len) + "/tail界"));
    }
    for text in [
        "abcdefgh",
        "abcdefghi",
        "/a_b.c-d",
        "界界ab",
        "😀😀",
        "ab\0",
    ] {
        candidates.push((4000 + candidates.len(), text.to_owned() + "suffix"));
    }
    let prepared: Vec<_> = candidates
        .iter()
        .map(|(id, s)| FuzzyCandidate::new(*id, s.clone()))
        .collect();
    let mut queries = vec![
        "".to_owned(),
        "tailmod".into(),
        "module".into(),
        "aaa".into(),
        "a".repeat(50),
    ];
    queries.extend(
        [
            "abcdefgh",
            "abcdefghi",
            "/a_b.c-d",
            "界界ab",
            "😀😀",
            "\0\0",
            "ab\0",
            "tail界",
        ]
        .iter()
        .map(|s| s.to_string()),
    );
    queries.extend(alphabet.iter().map(char::to_string));
    for &a in &alphabet {
        for &b in &alphabet {
            queries.push(format!("{a}{b}"));
        }
    }
    for query in queries {
        let mut expected: Vec<_> = candidates
            .iter()
            .filter_map(|(id, name)| reference_score(&query, name).map(|score| (*id, score)))
            .collect();
        expected.sort_by_key(|&(_, score)| std::cmp::Reverse(score));
        let legacy: Vec<_> = fuzzy_filter(&query, &candidates)
            .into_iter()
            .map(|(id, _, score)| (id, score))
            .collect();
        assert_eq!(legacy, expected, "legacy {query:?}");
        let actual: Vec<_> = fuzzy_filter_prepared(&query, &prepared)
            .into_iter()
            .map(|(cand, score)| (cand.id, score))
            .collect();
        assert_eq!(actual, expected, "{query:?}");
    }
}
