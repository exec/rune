use regex::Regex;
use std::collections::HashMap;

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
            goto_line_buffer: String::new(),
            cached_regex_pattern: None,
            cached_regex: None,
        }
    }
}

impl SearchState {
    pub fn find_all_matches(&mut self, rope: &ropey::Rope) -> Vec<(usize, usize)> {
        self.search_matches_truncated = false;
        if self.search_buffer.is_empty() {
            return Vec::new();
        }

        if self.use_regex {
            return self.find_all_regex_matches(rope);
        }

        let search_term = self.search_buffer.clone();
        let case_sensitive = self.case_sensitive;
        let search_lower = if case_sensitive {
            String::new()
        } else {
            search_term.to_lowercase()
        };

        let mut line_cache: HashMap<usize, String> = HashMap::new();
        let mut matches = Vec::new();

        'outer: for line_idx in 0..rope.len_lines() {
            let line_string = line_cache
                .entry(line_idx)
                .or_insert_with(|| crate::get_line_str(rope, line_idx));
            let line_content = line_string.trim_end_matches('\n');

            let line_matches = if case_sensitive {
                find_matches_in_line(line_content, &search_term)
            } else {
                find_matches_in_line(&line_content.to_lowercase(), &search_lower)
            };

            for col in line_matches {
                if validate_match_at_position(line_content, col, &search_term, case_sensitive) {
                    matches.push((line_idx, col));
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

    fn find_all_regex_matches(&mut self, rope: &ropey::Rope) -> Vec<(usize, usize)> {
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

            for m in re.find_iter(line_content) {
                let char_pos = line_content[..m.start()].chars().count();
                matches.push((line_idx, char_pos));
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
pub fn find_matches_in_line(line_content: &str, search_term: &str) -> Vec<usize> {
    let mut matches = Vec::new();
    let mut start_pos = 0;

    while let Some(pos) = line_content[start_pos..].find(search_term) {
        let byte_pos = start_pos + pos;
        // Convert byte offset to char position
        let char_pos = line_content[..byte_pos].chars().count();
        matches.push(char_pos);
        start_pos = byte_pos + 1;
        // Ensure we don't start in the middle of a multi-byte char
        while start_pos < line_content.len() && !line_content.is_char_boundary(start_pos) {
            start_pos += 1;
        }
    }

    matches
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
            return search_term.is_empty()
                && char_pos == line_content.chars().count();
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
        let mut state = SearchState::default();
        state.use_regex = true;
        state.search_buffer = r"\d+".to_string();
        let rope = Rope::from_str("hello123 world456\n");
        let matches = state.find_all_matches(&rope);
        assert_eq!(matches.len(), 2);
    }

    #[test]
    fn test_invalid_regex() {
        let mut state = SearchState::default();
        state.use_regex = true;
        state.search_buffer = "[invalid".to_string();
        let rope = Rope::from_str("hello\n");
        let matches = state.find_all_matches(&rope);
        assert_eq!(matches.len(), 0);
    }

    #[test]
    fn test_regex_case_insensitive() {
        let mut state = SearchState::default();
        state.use_regex = true;
        state.case_sensitive = false;
        state.search_buffer = "hello".to_string();
        let rope = Rope::from_str("Hello HELLO hello\n");
        let matches = state.find_all_matches(&rope);
        assert_eq!(matches.len(), 3);
    }
}
