use regex::Regex;

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
    pub current_match_index: Option<usize>,
    pub search_start_pos: (usize, usize),
    pub use_regex: bool,
    pub case_sensitive: bool,
    pub search_history: Vec<String>,
    pub search_history_index: Option<usize>,
    pub find_navigation_mode: FindNavigationMode,
    pub replace_phase: ReplacePhase,
    pub goto_line_buffer: String,
}

impl Default for SearchState {
    fn default() -> Self {
        Self {
            search_buffer: String::new(),
            replace_buffer: String::new(),
            search_matches: Vec::new(),
            current_match_index: None,
            search_start_pos: (0, 0),
            use_regex: false,
            case_sensitive: false,
            search_history: Vec::new(),
            search_history_index: None,
            find_navigation_mode: FindNavigationMode::HistoryBrowsing,
            replace_phase: ReplacePhase::FindPattern,
            goto_line_buffer: String::new(),
        }
    }
}

impl SearchState {
    pub fn find_all_matches(&mut self, rope: &ropey::Rope) -> Vec<(usize, usize)> {
        let search_term = &self.search_buffer;
        if search_term.is_empty() {
            return Vec::new();
        }

        if self.use_regex {
            return self.find_all_regex_matches(rope);
        }

        let mut matches = Vec::new();

        for line_idx in 0..rope.len_lines() {
            let rope_line = rope.line(line_idx);
            let owned_line: String;
            let line_str = match rope_line.as_str() {
                Some(s) => s,
                None => {
                    owned_line = rope_line.chars().collect::<String>();
                    &owned_line
                }
            };
            let line_content = line_str.trim_end_matches('\n');

            let line_matches = if self.case_sensitive {
                find_matches_in_line(line_content, search_term)
            } else {
                find_matches_in_line(
                    &line_content.to_lowercase(),
                    &search_term.to_lowercase(),
                )
            };

            for col in line_matches {
                if validate_match(rope, line_idx, col, search_term, self.case_sensitive) {
                    matches.push((line_idx, col));
                }
            }
        }

        matches.sort_by(|a, b| a.0.cmp(&b.0).then(a.1.cmp(&b.1)));
        matches
    }

    fn find_all_regex_matches(&self, rope: &ropey::Rope) -> Vec<(usize, usize)> {
        let pattern = if self.case_sensitive {
            self.search_buffer.clone()
        } else {
            format!("(?i){}", self.search_buffer)
        };

        let re = match Regex::new(&pattern) {
            Ok(re) => re,
            Err(_) => return Vec::new(),
        };

        let mut matches = Vec::new();

        for line_idx in 0..rope.len_lines() {
            let rope_line = rope.line(line_idx);
            let owned_line: String;
            let line_str = match rope_line.as_str() {
                Some(s) => s,
                None => {
                    owned_line = rope_line.chars().collect::<String>();
                    &owned_line
                }
            };
            let line_content = line_str.trim_end_matches('\n');

            for m in re.find_iter(line_content) {
                let char_pos = line_content[..m.start()].chars().count();
                matches.push((line_idx, char_pos));
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
    let rope_line = rope.line(line_idx);
    let owned_line: String;
    let line_str = match rope_line.as_str() {
        Some(s) => s,
        None => {
            owned_line = rope_line.chars().collect::<String>();
            &owned_line
        }
    };
    let line_content = line_str.trim_end_matches('\n');
    validate_match_at_position(line_content, col, search_term, case_sensitive)
}

/// Validate that text at a given character position matches the search term
pub fn validate_match_at_position(
    line_content: &str,
    char_pos: usize,
    search_term: &str,
    case_sensitive: bool,
) -> bool {
    let line_chars: Vec<char> = line_content.chars().collect();
    let search_chars: Vec<char> = search_term.chars().collect();

    if char_pos + search_chars.len() > line_chars.len() {
        return false;
    }

    let text_at_pos: String = line_chars[char_pos..char_pos + search_chars.len()]
        .iter()
        .collect();

    if case_sensitive {
        text_at_pos == search_term
    } else {
        text_at_pos.to_lowercase() == search_term.to_lowercase()
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
