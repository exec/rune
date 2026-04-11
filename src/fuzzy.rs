/// Score a candidate string against a query using fuzzy matching.
/// Returns None if no match, Some(score) if match (higher = better).
pub fn fuzzy_score(query: &str, candidate: &str) -> Option<i32> {
    if query.is_empty() {
        return Some(0);
    }

    let query_lower = query.to_lowercase();
    let candidate_lower = candidate.to_lowercase();
    let query_chars: Vec<char> = query_lower.chars().collect();
    let candidate_chars: Vec<char> = candidate_lower.chars().collect();

    let mut query_idx = 0;
    let mut score = 0i32;
    let mut last_match_idx: Option<usize> = None;

    for (i, &ch) in candidate_chars.iter().enumerate() {
        if query_idx < query_chars.len() && ch == query_chars[query_idx] {
            score += 10; // base match score

            // Bonus for consecutive matches
            if let Some(last) = last_match_idx {
                if i == last + 1 {
                    score += 5;
                }
            }

            // Bonus for matching at word boundaries
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
        // Penalize longer candidates
        score -= candidate.len() as i32;
        Some(score)
    } else {
        None // not all query chars matched
    }
}

/// Filter and rank a list of candidates by fuzzy match score.
pub fn fuzzy_filter(query: &str, candidates: &[(usize, String)]) -> Vec<(usize, String, i32)> {
    let mut scored: Vec<(usize, String, i32)> = candidates
        .iter()
        .filter_map(|(idx, name)| {
            fuzzy_score(query, name).map(|score| (*idx, name.clone(), score))
        })
        .collect();

    scored.sort_by(|a, b| b.2.cmp(&a.2)); // highest score first
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
        let candidates = vec![
            (0, "alpha.rs".to_string()),
            (1, "beta.rs".to_string()),
        ];
        let results = fuzzy_filter("xyz", &candidates);
        assert!(results.is_empty());
    }

    #[test]
    fn test_fuzzy_filter_empty_query_returns_all() {
        let candidates = vec![
            (0, "a.rs".to_string()),
            (1, "b.rs".to_string()),
        ];
        let results = fuzzy_filter("", &candidates);
        assert_eq!(results.len(), 2);
    }
}
