/// A candidate prepared once for fuzzy matching — avoids re-lowercasing and
/// re-collecting chars on every keystroke.
#[derive(Debug, Clone)]
pub struct FuzzyCandidate {
    pub id: usize,
    pub display: String,
    lowercase_chars: Vec<char>,
    byte_len: usize,
}

impl FuzzyCandidate {
    pub fn new(id: usize, display: String) -> Self {
        let lowercase_chars: Vec<char> = display.to_lowercase().chars().collect();
        let byte_len = display.len();
        Self {
            id,
            display,
            lowercase_chars,
            byte_len,
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
/// Legacy API kept for ui.rs and benches — allocates per-keystroke.
pub fn fuzzy_filter(query: &str, candidates: &[(usize, String)]) -> Vec<(usize, String, i32)> {
    let mut scored: Vec<(usize, String, i32)> = candidates
        .iter()
        .filter_map(|(idx, name)| fuzzy_score(query, name).map(|score| (*idx, name.clone(), score)))
        .collect();

    scored.sort_by(|a, b| b.2.cmp(&a.2));
    scored
}

/// Filter and rank a list of prepared candidates. The lowercase/char-vec work
/// is done once when the candidate list is built, not per keystroke.
pub fn fuzzy_filter_prepared<'a>(
    query: &str,
    candidates: &'a [FuzzyCandidate],
) -> Vec<(&'a FuzzyCandidate, i32)> {
    let query_lower = query.to_lowercase();
    let query_chars: Vec<char> = query_lower.chars().collect();

    let mut scored: Vec<(&FuzzyCandidate, i32)> = candidates
        .iter()
        .filter_map(|cand| {
            score_prepared(&query_chars, &cand.lowercase_chars, cand.byte_len).map(|s| (cand, s))
        })
        .collect();

    scored.sort_by(|a, b| b.1.cmp(&a.1));
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
}
