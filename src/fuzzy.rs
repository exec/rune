/// A candidate prepared once for fuzzy matching — avoids re-lowercasing and
/// re-collecting chars on every keystroke.
#[derive(Debug, Clone)]
pub struct FuzzyCandidate {
    pub id: usize,
    pub display: String,
    lowercase_chars: Vec<char>,
    byte_len: usize,
    // Collisions only permit extra scans, never incorrect exclusions.
    character_mask: u64,
    prefix: [u8; 8],
}

impl FuzzyCandidate {
    pub fn new(id: usize, display: String) -> Self {
        let normalized = display.to_lowercase();
        let lowercase_chars: Vec<char> = normalized.chars().collect();
        let character_mask = lowercase_chars
            .iter()
            .fold(0u64, |mask, &ch| mask | (1u64 << (ch as u32 % 64)));
        let mut prefix = [0; 8];
        let len = normalized.len().min(prefix.len());
        prefix[..len].copy_from_slice(&normalized.as_bytes()[..len]);
        let byte_len = display.len();
        Self {
            id,
            display,
            lowercase_chars,
            byte_len,
            character_mask,
            prefix,
        }
    }
}

/// Score a candidate string against a query using fuzzy matching.
/// Returns None if no match, Some(score) if match (higher = better).
pub fn fuzzy_score(query: &str, candidate: &str) -> Option<i32> {
    if query.is_empty() {
        return Some(0);
    }

    let query_lower = query.to_lowercase();
    let query_chars: Vec<char> = query_lower.chars().collect();
    let candidate_lower = candidate.to_lowercase();
    let candidate_chars: Vec<char> = candidate_lower.chars().collect();

    score_prepared(&query_chars, &candidate_chars, candidate.len())
}

/// Score a query (already lowercased char-vec) against a pre-prepared candidate.
fn score_prepared(
    query_chars: &[char],
    candidate_chars: &[char],
    candidate_byte_len: usize,
) -> Option<i32> {
    if query_chars.is_empty() {
        return Some(0);
    }

    let mut query_idx = 0;
    let mut score = 0i32;
    let mut last_match_idx: Option<usize> = None;

    for (i, &ch) in candidate_chars.iter().enumerate() {
        if query_idx < query_chars.len() && ch == query_chars[query_idx] {
            score += 10;

            if let Some(last) = last_match_idx {
                if i == last + 1 {
                    score += 5;
                }
            }

            if i == 0
                || candidate_chars[i - 1] == '/'
                || candidate_chars[i - 1] == '_'
                || candidate_chars[i - 1] == '-'
                || candidate_chars[i - 1] == '.'
            {
                score += 8;
            }

            last_match_idx = Some(i);
            query_idx += 1;
            if query_idx == query_chars.len() {
                break;
            }
        }
    }

    if query_idx == query_chars.len() {
        score -= candidate_byte_len as i32;
        Some(score)
    } else {
        None
    }
}

/// Filter and rank a list of candidates by fuzzy match score.
/// Legacy API — prepares candidate text on each call.
pub fn fuzzy_filter(query: &str, candidates: &[(usize, String)]) -> Vec<(usize, String, i32)> {
    let mut scored: Vec<(usize, String, i32)> = candidates
        .iter()
        .filter_map(|(idx, name)| fuzzy_score(query, name).map(|score| (*idx, name.clone(), score)))
        .collect();

    scored.sort_by_key(|s| std::cmp::Reverse(s.2));
    scored
}

/// Filter and rank a list of prepared candidates. The lowercase/char-vec work
/// is done once when the candidate list is built, not per keystroke.
pub fn fuzzy_filter_prepared<'a>(
    query: &str,
    candidates: &'a [FuzzyCandidate],
) -> Vec<(&'a FuzzyCandidate, i32)> {
    if query.is_empty() {
        return candidates.iter().map(|cand| (cand, 0)).collect();
    }
    let query_lower = query.to_lowercase();
    let query_chars: Vec<char> = query_lower.chars().collect();

    let query_mask = query_chars
        .iter()
        .fold(0u64, |mask, &ch| mask | (1u64 << (ch as u32 % 64)));
    // Short prefix matches need only cached bytes and one positional score.
    let prefix = (query_lower.len() <= 8).then(|| {
        let mut bytes = [0; 8];
        bytes[..query_lower.len()].copy_from_slice(query_lower.as_bytes());
        let mask = u64::MAX >> ((8 - query_lower.len()) * 8);
        let score: i32 = query_chars
            .iter()
            .enumerate()
            .map(|(i, _)| {
                10 + if i > 0 { 5 } else { 0 }
                    + if i == 0 || matches!(query_chars[i - 1], '/' | '_' | '-' | '.') {
                        8
                    } else {
                        0
                    }
            })
            .sum();
        (u64::from_le_bytes(bytes), mask, score)
    });

    let mut scored: Vec<(&FuzzyCandidate, i32)> = candidates
        .iter()
        .filter_map(|cand| {
            if let Some((bytes, mask, score)) = prefix {
                if cand.lowercase_chars.len() >= query_chars.len()
                    && u64::from_le_bytes(cand.prefix) & mask == bytes
                {
                    return Some((cand, score - cand.byte_len as i32));
                }
            }
            if query_chars.len() > cand.lowercase_chars.len()
                || query_mask & !cand.character_mask != 0
            {
                return None;
            }
            score_prepared(&query_chars, &cand.lowercase_chars, cand.byte_len).map(|s| (cand, s))
        })
        .collect();

    scored.sort_by_key(|s| std::cmp::Reverse(s.1));
    scored
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_empty_query_matches_all() {
        assert_eq!(fuzzy_score("", "anything"), Some(0));
    }

    #[test]
    fn test_exact_match() {
        let score = fuzzy_score("main.rs", "main.rs");
        assert!(score.is_some());
        assert!(score.unwrap() > 0);
    }

    #[test]
    fn test_no_match() {
        assert_eq!(fuzzy_score("xyz", "abc"), None);
    }

    #[test]
    fn test_partial_match() {
        let score = fuzzy_score("mr", "main.rs");
        assert!(score.is_some());
    }

    #[test]
    fn test_case_insensitive() {
        let score = fuzzy_score("MAIN", "main.rs");
        assert!(score.is_some());
    }

    #[test]
    fn test_consecutive_bonus() {
        // "abc" in "abcdef" = all consecutive, vs "adf" in "abcdef" = scattered
        let consecutive = fuzzy_score("abc", "abcdef").unwrap();
        let scattered = fuzzy_score("adf", "abcdef").unwrap();
        assert!(consecutive > scattered);
    }

    #[test]
    fn test_word_boundary_bonus() {
        // "e" at start of "editor.rs" should score higher than "e" in middle of "getter.rs"
        let boundary = fuzzy_score("e", "editor.rs").unwrap();
        let middle = fuzzy_score("e", "xxeditor.rs").unwrap();
        assert!(boundary > middle);
    }

    #[test]
    fn test_shorter_candidate_preferred() {
        let short = fuzzy_score("m", "m.rs").unwrap();
        let long = fuzzy_score("m", "very_long_module_name.rs").unwrap();
        assert!(short > long);
    }

    #[test]
    fn test_fuzzy_filter_ordering() {
        let candidates = vec![
            (0, "very_long_name.rs".to_string()),
            (1, "main.rs".to_string()),
            (2, "module.rs".to_string()),
        ];
        let results = fuzzy_filter("m", &candidates);
        assert_eq!(results.len(), 3);
        // "main.rs" and "module.rs" should rank before "very_long_name.rs"
        // because they have word-boundary bonus for 'm'
        let first_idx = results[0].0;
        assert!(first_idx == 1 || first_idx == 2);
    }

    #[test]
    fn test_fuzzy_filter_excludes_non_matches() {
        let candidates = vec![(0, "alpha.rs".to_string()), (1, "beta.rs".to_string())];
        let results = fuzzy_filter("xyz", &candidates);
        assert!(results.is_empty());
    }

    #[test]
    fn test_fuzzy_filter_empty_query_returns_all() {
        let candidates = vec![(0, "a.rs".to_string()), (1, "b.rs".to_string())];
        let results = fuzzy_filter("", &candidates);
        assert_eq!(results.len(), 2);
    }
    #[test]
    fn scores_match_greedy_position_reference() {
        for query in ["", "a", "mod", "oda", "not-found", "界a", "İ", "ab"] {
            for candidate in [
                "",
                "module/path/a.rs",
                "a_long_trailing_suffix",
                "界/a",
                "İnput",
                "prefix-ab-ab",
                "ab",
                "MODule_abc",
            ] {
                let q: Vec<_> = query.to_lowercase().chars().collect();
                let chars: Vec<_> = candidate.to_lowercase().chars().collect();
                let expected = (|| {
                    if q.is_empty() {
                        return Some(0);
                    }
                    let mut positions = Vec::new();
                    let mut start = 0;
                    for ch in q {
                        let index = start + chars[start..].iter().position(|&c| c == ch)?;
                        positions.push(index);
                        start = index + 1;
                    }
                    let consecutive = positions.windows(2).filter(|p| p[1] == p[0] + 1).count();
                    let boundaries = positions
                        .iter()
                        .filter(|&&i| i == 0 || ['/', '_', '-', '.'].contains(&chars[i - 1]))
                        .count();
                    Some(
                        (10 * positions.len() + 5 * consecutive + 8 * boundaries) as i32
                            - candidate.len() as i32,
                    )
                })();
                assert_eq!(
                    fuzzy_score(query, candidate),
                    expected,
                    "{query:?}, {candidate:?}"
                );
            }
        }
    }
}
