use regex::Regex;

/// Maximum number of matches returned by a single search. Beyond this,
/// accumulation stops and the caller is expected to notify the user.
pub const MAX_SEARCH_MATCHES: usize = 10_000;

/// Navigation mode within find functionality
#[derive(Debug, Clone, PartialEq)]
pub enum FindNavigationMode {
    HistoryBrowsing,
    ResultNavigation,
}

/// Phase of the replace workflow, replacing string-based state tracking
#[derive(Debug, Clone, PartialEq)]
pub enum ReplacePhase {
    FindPattern,
    ReplaceWith,
}

/// All search-related state grouped together
pub struct SearchState {
    pub search_buffer: String,
    pub replace_buffer: String,
    pub search_matches: Vec<(usize, usize)>,
    /// True when the most recent `find_all_matches` call hit
    /// `MAX_SEARCH_MATCHES` and stopped accumulating.
    pub search_matches_truncated: bool,
    pub current_match_index: Option<usize>,
    pub search_start_pos: (usize, usize),
    pub use_regex: bool,
    pub case_sensitive: bool,
    pub search_history: Vec<String>,
    pub search_history_index: Option<usize>,
    pub find_navigation_mode: FindNavigationMode,
    pub replace_phase: ReplacePhase,
    /// Absolute char index the interactive replace session resumes from.
    /// Reset to 0 when a session enters the confirm phase; advanced past
    /// each handled (replaced or skipped) match so the session always
    /// makes forward progress.
    pub replace_resume_char: usize,
    pub goto_line_buffer: String,
    cached_regex_pattern: Option<String>,
    cached_regex: Option<Regex>,
}

impl Default for SearchState {
    fn default() -> Self {
        Self {
            search_buffer: String::new(),
            replace_buffer: String::new(),
            search_matches: Vec::new(),
            search_matches_truncated: false,
            current_match_index: None,
            search_start_pos: (0, 0),
            use_regex: false,
            case_sensitive: false,
            search_history: Vec::new(),
            search_history_index: None,
            find_navigation_mode: FindNavigationMode::HistoryBrowsing,
            replace_phase: ReplacePhase::FindPattern,
            replace_resume_char: 0,
            goto_line_buffer: String::new(),
            cached_regex_pattern: None,
            cached_regex: None,
        }
    }
}

impl SearchState {
    pub fn find_all_matches(&mut self, rope: &ropey::Rope) -> Vec<(usize, usize)> {
        self.find_all_match_spans(rope)
            .into_iter()
            .map(|(line, col, _)| (line, col))
            .collect()
    }

    /// Like `find_all_matches`, but each entry also carries the match length
    /// in chars: `(line, char_col, char_len)`. Regex match lengths vary per
    /// match, so replace needs this richer form; the UI keeps consuming the
    /// plain `(line, char_col)` pairs in `search_matches`.
    pub fn find_all_match_spans(&mut self, rope: &ropey::Rope) -> Vec<(usize, usize, usize)> {
        self.search_matches_truncated = false;
        if self.search_buffer.is_empty() {
            return Vec::new();
        }

        if self.use_regex {
            return self.find_all_regex_match_spans(rope);
        }

        let search_term = self.search_buffer.clone();
        let search_char_len = search_term.chars().count();
        let case_sensitive = self.case_sensitive;
        let search_lower = if case_sensitive {
            String::new()
        } else {
            search_term.to_lowercase()
        };

        let mut matches = Vec::new();

        'outer: for line_idx in 0..rope.len_lines() {
            let line_string = crate::get_line_str(rope, line_idx);
            let line_content = line_string.trim_end_matches('\n');

            let line_matches = if case_sensitive {
                find_matches_in_line(line_content, &search_term)
            } else {
                find_matches_in_line(&line_content.to_lowercase(), &search_lower)
            };

            if line_matches.is_empty() {
                continue;
            }

            // Matches arrive in ascending char order, so a single cursor walking
            // the line converts every one of them to a byte offset. Previously
            // each match re-walked the line from column 0 inside
            // `validate_match_at_position`, which is the second half of the
            // O(matches x line_length) cost.
            let mut chars = line_content.char_indices();
            let mut cursor_char = 0usize;
            let mut cursor_byte = 0usize;
            let mut exhausted = false;

            for col in line_matches {
                while cursor_char < col {
                    match chars.next() {
                        Some((b, ch)) => {
                            cursor_byte = b + ch.len_utf8();
                            cursor_char += 1;
                        }
                        None => {
                            exhausted = true;
                            break;
                        }
                    }
                }
                if exhausted || cursor_byte > line_content.len() {
                    break;
                }

                if validate_match_at_byte(
                    line_content,
                    cursor_byte,
                    &search_term,
                    &search_lower,
                    case_sensitive,
                ) {
                    matches.push((line_idx, col, search_char_len));
                    if matches.len() >= MAX_SEARCH_MATCHES {
                        self.search_matches_truncated = true;
                        break 'outer;
                    }
                }
            }
        }

        matches.sort_by(|a, b| a.0.cmp(&b.0).then(a.1.cmp(&b.1)));
        matches
    }

    fn find_all_regex_match_spans(&mut self, rope: &ropey::Rope) -> Vec<(usize, usize, usize)> {
        let pattern = if self.case_sensitive {
            self.search_buffer.clone()
        } else {
            format!("(?i){}", self.search_buffer)
        };

        let cache_hit = self
            .cached_regex_pattern
            .as_ref()
            .is_some_and(|p| p == &pattern);
        if !cache_hit {
            match Regex::new(&pattern) {
                Ok(re) => {
                    self.cached_regex = Some(re);
                    self.cached_regex_pattern = Some(pattern);
                }
                Err(_) => {
                    self.cached_regex = None;
                    self.cached_regex_pattern = None;
                    return Vec::new();
                }
            }
        }

        let re = match self.cached_regex.as_ref() {
            Some(re) => re,
            None => return Vec::new(),
        };

        let mut matches = Vec::new();

        'outer: for line_idx in 0..rope.len_lines() {
            let line_string = crate::get_line_str(rope, line_idx);
            let line_content = line_string.trim_end_matches('\n');

            // Same incremental byte->char carry as the literal path: `find_iter`
            // yields matches in ascending order, so each conversion only needs to
            // count the gap since the previous match rather than rescan from the
            // start of the line.
            let mut prev_byte = 0usize;
            let mut prev_char = 0usize;
            for m in re.find_iter(line_content) {
                prev_char += line_content[prev_byte..m.start()].chars().count();
                prev_byte = m.start();
                let char_len = line_content[m.start()..m.end()].chars().count();
                matches.push((line_idx, prev_char, char_len));
                if matches.len() >= MAX_SEARCH_MATCHES {
                    self.search_matches_truncated = true;
                    break 'outer;
                }
            }
        }

        matches.sort_by(|a, b| a.0.cmp(&b.0).then(a.1.cmp(&b.1)));
        matches
    }

    /// Navigate to the next or previous match. `forward=true` for next, `false` for previous.
    pub fn navigate_match(&mut self, forward: bool) -> Option<(usize, usize)> {
        if self.search_matches.is_empty() {
            return None;
        }

        if let Some(current_index) = self.current_match_index {
            let new_index = if forward {
                (current_index + 1) % self.search_matches.len()
            } else if current_index == 0 {
                self.search_matches.len() - 1
            } else {
                current_index - 1
            };
            self.current_match_index = Some(new_index);
            self.search_matches.get(new_index).copied()
        } else {
            None
        }
    }

    pub fn add_to_search_history(&mut self, search_term: &str) {
        if !search_term.is_empty() {
            self.search_history.retain(|s| s != search_term);
            self.search_history.push(search_term.to_string());
            if self.search_history.len() > super::constants::SEARCH_HISTORY_LIMIT {
                self.search_history.remove(0);
            }
            self.search_history_index = None;
        }
    }

    pub fn navigate_search_history_up(&mut self) -> bool {
        if self.search_history.is_empty() {
            return false;
        }

        if let Some(current_index) = self.search_history_index {
            if current_index > 0 {
                self.search_history_index = Some(current_index - 1);
            } else {
                return false;
            }
        } else {
            self.search_history_index = Some(self.search_history.len() - 1);
        }

        if let Some(index) = self.search_history_index {
            if let Some(term) = self.search_history.get(index) {
                self.search_buffer = term.clone();
                return true;
            }
        }
        false
    }

    pub fn navigate_search_history_down(&mut self) -> bool {
        if let Some(current_index) = self.search_history_index {
            if current_index < self.search_history.len() - 1 {
                self.search_history_index = Some(current_index + 1);
                if let Some(index) = self.search_history_index {
                    if let Some(term) = self.search_history.get(index) {
                        self.search_buffer = term.clone();
                    }
                }
                return true;
            } else {
                self.search_history_index = None;
                self.search_buffer.clear();
                return true;
            }
        }
        false
    }

    pub fn cancel_search(&mut self) -> (usize, usize) {
        let start_pos = self.search_start_pos;
        self.search_matches.clear();
        self.current_match_index = None;
        self.cached_regex = None;
        self.cached_regex_pattern = None;
        start_pos
    }
}

/// Find all occurrences of search_term in a single line, returning char positions.
///
/// The byte->char conversion is carried incrementally rather than recomputed from
/// the start of the line for each hit. The old `line_content[..byte_pos].chars()
/// .count()` made this O(matches x line_length): on a 2MB minified-JSON line with
/// 125k hits it cost ~3.5s, per keystroke, while the user was typing in find mode.
/// Counting only the gap since the previous match makes the whole scan O(line).
pub fn find_matches_in_line(line_content: &str, search_term: &str) -> Vec<usize> {
    let mut matches = Vec::new();
    if search_term.is_empty() {
        return matches;
    }

    let mut start_byte = 0usize;
    // Char index corresponding to `start_byte`, carried forward across matches.
    let mut char_pos = 0usize;

    while let Some(rel) = line_content[start_byte..].find(search_term) {
        let byte_pos = start_byte + rel;
        char_pos += line_content[start_byte..byte_pos].chars().count();
        matches.push(char_pos);

        // Advance exactly one char past the match start, so overlapping matches
        // are still found and `start_byte` stays on a char boundary.
        let step = line_content[byte_pos..]
            .chars()
            .next()
            .map(|c| c.len_utf8())
            .unwrap_or(1);
        start_byte = byte_pos + step;
        char_pos += 1;
        if start_byte >= line_content.len() {
            break;
        }
    }

    matches
}

/// Validate a match whose byte offset within the line is already known.
///
/// `validate_match_at_position` has to walk the line to turn a char position into
/// a byte offset; when the caller is iterating matches in ascending order it can
/// track that offset itself and skip the walk entirely.
fn validate_match_at_byte(
    line_content: &str,
    byte_start: usize,
    search_term: &str,
    search_term_lower: &str,
    case_sensitive: bool,
) -> bool {
    let tail = &line_content[byte_start..];

    if case_sensitive {
        return tail.starts_with(search_term);
    }

    // Take exactly as many chars as the search term has, then compare lowercased.
    // `search_term_lower` is hoisted by the caller: lowercasing the needle once per
    // match (as the old code did) is pure loop-invariant work.
    let needed = search_term.chars().count();
    let mut end = 0usize;
    let mut taken = 0usize;
    for (b, ch) in tail.char_indices() {
        if taken == needed {
            end = b;
            break;
        }
        taken += 1;
        end = b + ch.len_utf8();
    }
    if taken < needed {
        return false;
    }
    tail[..end].to_lowercase() == search_term_lower
}

/// Unified match validation — validates that a match actually exists at the specified position
pub fn validate_match(
    rope: &ropey::Rope,
    line_idx: usize,
    col: usize,
    search_term: &str,
    case_sensitive: bool,
) -> bool {
    let line_string = crate::get_line_str(rope, line_idx);
    let line_content = line_string.trim_end_matches('\n');
    validate_match_at_position(line_content, col, search_term, case_sensitive)
}

/// Validate that text at a given character position matches the search term
pub fn validate_match_at_position(
    line_content: &str,
    char_pos: usize,
    search_term: &str,
    case_sensitive: bool,
) -> bool {
    // Walk chars to find the byte offset at char_pos without allocating a Vec<char>.
    let mut byte_start: Option<usize> = None;
    for (char_idx, (b, _)) in line_content.char_indices().enumerate() {
        if char_idx == char_pos {
            byte_start = Some(b);
            break;
        }
    }
    let byte_start = match byte_start {
        Some(b) => b,
        None => {
            // char_pos may equal line length (valid only if search_term is empty)
            return search_term.is_empty() && char_pos == line_content.chars().count();
        }
    };

    let tail = &line_content[byte_start..];

    if case_sensitive {
        tail.starts_with(search_term)
    } else {
        // Count chars needed and take exactly that many from the tail for comparison.
        let needed = search_term.chars().count();
        let mut end_byte = byte_start;
        let mut taken = 0usize;
        for (b, ch) in tail.char_indices() {
            if taken == needed {
                end_byte = byte_start + b;
                break;
            }
            taken += 1;
            end_byte = byte_start + b + ch.len_utf8();
        }
        if taken < needed {
            return false;
        }
        let slice = &line_content[byte_start..end_byte];
        slice.to_lowercase() == search_term.to_lowercase()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ropey::Rope;

    #[test]
    fn test_regex_digits() {
        let mut state = SearchState {
            use_regex: true,
            search_buffer: r"\d+".to_string(),
            ..SearchState::default()
        };
        let rope = Rope::from_str("hello123 world456\n");
        let matches = state.find_all_matches(&rope);
        assert_eq!(matches.len(), 2);
    }

    #[test]
    fn test_invalid_regex() {
        let mut state = SearchState {
            use_regex: true,
            search_buffer: "[invalid".to_string(),
            ..SearchState::default()
        };
        let rope = Rope::from_str("hello\n");
        let matches = state.find_all_matches(&rope);
        assert_eq!(matches.len(), 0);
    }

    #[test]
    fn test_regex_case_insensitive() {
        let mut state = SearchState {
            use_regex: true,
            case_sensitive: false,
            search_buffer: "hello".to_string(),
            ..SearchState::default()
        };
        let rope = Rope::from_str("Hello HELLO hello\n");
        let matches = state.find_all_matches(&rope);
        assert_eq!(matches.len(), 3);
    }
}
