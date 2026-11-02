use regex::Regex;

/// Maximum number of matches returned by a single search. Beyond this,
/// accumulation stops and the caller is expected to notify the user.
pub const MAX_SEARCH_MATCHES: usize = 10_000;
const SEARCH_BLOCK_BYTES: usize = 64 * 1024;

fn literal_match_spans(
    rope: &ropey::Rope,
    resume: usize,
    limit: usize,
    regex: &Regex,
    max_match_bytes: usize,
) -> (Vec<(usize, usize, usize)>, bool) {
    let mut base = rope.char_to_byte(resume);
    let slice = rope.byte_slice(base..);
    let mut chunks = slice.chunks().peekable();
    let target = SEARCH_BLOCK_BYTES + max_match_bytes;
    let mut text = String::with_capacity(target);
    let mut matches = Vec::new();
    loop {
        while text.len() < target {
            let Some(chunk) = chunks.next() else { break };
            text.push_str(chunk);
        }
        let eof = chunks.peek().is_none();
        let mut boundary = if eof {
            text.len()
        } else {
            text.len().saturating_sub(max_match_bytes - 1)
        };
        while !text.is_char_boundary(boundary) {
            boundary -= 1;
        }
        let mut next_match = regex.find_at(&text, 0);
        if let Some(first) = next_match.filter(|m| eof || m.start() < boundary) {
            let mut character = rope.byte_to_char(base + first.start());
            let mut previous_byte = first.start();
            let mut line = rope.char_to_line(character);
            let mut line_start = rope.line_to_char(line);
            let mut lines = rope.lines_at(line);
            let mut current_line = lines.next().unwrap();
            let mut line_end_byte = rope.line_to_byte(line) + current_line.len_bytes();
            while let Some(found) = next_match {
                // Defer starts in the overlap until their full match is available.
                if !eof && found.start() >= boundary {
                    break;
                }
                if matches.len() == limit {
                    return (matches, true);
                }
                // Jump to the first result, then walk only intervening lines in this block.
                while base + found.start() >= line_end_byte {
                    line_start += current_line.len_chars();
                    line += 1;
                    current_line = lines.next().unwrap();
                    line_end_byte += current_line.len_bytes();
                }
                character += text[previous_byte..found.start()].chars().count();
                previous_byte = found.start();
                matches.push((
                    line,
                    character - line_start,
                    text[found.start()..found.end()].chars().count(),
                ));
                // Literal searches include overlapping matches, unlike regex searches.
                let byte = found.start() + text[found.start()..].chars().next().unwrap().len_utf8();
                next_match = regex.find_at(&text, byte);
            }
        }
        if eof {
            return (matches, false);
        }
        text.drain(..boundary);
        base += boundary;
    }
}

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
    pub search_match_lengths: Vec<usize>,
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
    cached_regex_flags: (bool, bool),
}

impl Default for SearchState {
    fn default() -> Self {
        Self {
            search_buffer: String::new(),
            replace_buffer: String::new(),
            search_matches: Vec::new(),
            search_match_lengths: Vec::new(),
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
            cached_regex_flags: (false, false),
        }
    }
}

impl SearchState {
    pub fn find_all_matches(&mut self, rope: &ropey::Rope) -> Vec<(usize, usize)> {
        let spans = self.find_all_match_spans(rope);
        self.search_match_lengths.clear();
        self.search_match_lengths.extend(spans.iter().map(|m| m.2));
        spans
            .into_iter()
            .map(|(line, col, _)| (line, col))
            .collect()
    }

    pub fn find_all_match_spans(&mut self, rope: &ropey::Rope) -> Vec<(usize, usize, usize)> {
        self.match_spans(rope, 0, MAX_SEARCH_MATCHES)
    }

    /// Scan from a source character position; only UI searches limit results.
    pub fn match_spans(
        &mut self,
        rope: &ropey::Rope,
        resume: usize,
        limit: usize,
    ) -> Vec<(usize, usize, usize)> {
        self.search_matches_truncated = false;
        if self.search_buffer.is_empty() || resume > rope.len_chars() {
            return Vec::new();
        }
        self.prepare_regex();
        let Some(re) = self.cached_regex.as_ref() else {
            return Vec::new();
        };
        if !self.use_regex
            && self.search_buffer.len() <= SEARCH_BLOCK_BYTES / 4
            && !self.search_buffer.contains([
                '\n', '\r', '\u{b}', '\u{c}', '\u{85}', '\u{2028}', '\u{2029}',
            ])
        {
            // Unicode simple case folding matches one scalar per query scalar, at most four bytes.
            let max_match_bytes = self.search_buffer.chars().count() * 4;
            let (matches, truncated) =
                literal_match_spans(rope, resume, limit, re, max_match_bytes);
            self.search_matches_truncated = truncated;
            return matches;
        }
        let mut matches = Vec::new();
        let first_line = rope.char_to_line(resume);
        let mut line_start = rope.line_to_char(first_line);
        for (offset, rope_line) in rope.lines_at(first_line).enumerate() {
            let line_idx = first_line + offset;
            let text: std::borrow::Cow<'_, str> = rope_line.into();
            let text = text.trim_end_matches('\n');
            let mut byte = rope_line
                .char_to_byte(resume.saturating_sub(line_start))
                .min(text.len());
            let mut previous_byte = byte;
            let mut previous_char = rope_line.byte_to_char(byte);
            line_start += rope_line.len_chars();
            let mut last_end = None;
            while let Some(m) = re.find_at(text, byte) {
                if self.use_regex && m.is_empty() && last_end == Some(m.start()) {
                    if m.end() == text.len() {
                        break;
                    }
                    byte = m.end() + text[m.end()..].chars().next().unwrap().len_utf8();
                    continue;
                }
                last_end = Some(m.end());
                if matches.len() == limit {
                    self.search_matches_truncated = true;
                    return matches;
                }
                previous_char += text[previous_byte..m.start()].chars().count();
                previous_byte = m.start();
                matches.push((
                    line_idx,
                    previous_char,
                    text[m.start()..m.end()].chars().count(),
                ));
                let advance_from = if self.use_regex && m.end() > m.start() {
                    m.end()
                } else {
                    m.start()
                };
                if advance_from == text.len() {
                    break;
                }
                byte = if self.use_regex && m.end() > m.start() {
                    m.end()
                } else {
                    advance_from + text[advance_from..].chars().next().unwrap().len_utf8()
                };
            }
        }
        matches
    }

    /// Find one match without looking ahead or changing the UI's truncation flag.
    // Preserve regex inlining in full-result scans (see search_literal benchmarks).
    #[inline]
    pub fn first_match_span(
        &mut self,
        rope: &ropey::Rope,
        resume: usize,
    ) -> Option<(usize, usize, usize)> {
        if self.search_buffer.is_empty() || resume > rope.len_chars() {
            return None;
        }
        self.prepare_regex();
        let re = self.cached_regex.as_ref()?;
        let first_line = rope.char_to_line(resume);
        let mut line_start = rope.line_to_char(first_line);
        for (offset, rope_line) in rope.lines_at(first_line).enumerate() {
            let text: std::borrow::Cow<'_, str> = rope_line.into();
            let text = text.trim_end_matches('\n');
            let byte = rope_line
                .char_to_byte(resume.saturating_sub(line_start))
                .min(text.len());
            line_start += rope_line.len_chars();
            if let Some(m) = re.find_at(text, byte) {
                return Some((
                    first_line + offset,
                    rope_line.byte_to_char(m.start()),
                    text[m.start()..m.end()].chars().count(),
                ));
            }
        }
        None
    }

    fn prepare_regex(&mut self) {
        let flags = (self.use_regex, self.case_sensitive);
        if self.cached_regex_pattern.as_deref() != Some(self.search_buffer.as_str())
            || self.cached_regex_flags != flags
        {
            let source = if self.use_regex {
                std::borrow::Cow::Borrowed(self.search_buffer.as_str())
            } else {
                std::borrow::Cow::Owned(regex::escape(&self.search_buffer))
            };
            self.cached_regex = regex::RegexBuilder::new(&source)
                .case_insensitive(!self.case_sensitive)
                .build()
                .ok();
            self.cached_regex_pattern = Some(self.search_buffer.clone());
            self.cached_regex_flags = flags;
        }
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

    #[test]
    fn literal_blocks_progress_with_an_overlap_larger_than_the_block_size() {
        let text = "x".repeat(400_000) + "needle\nneedle";
        let rope = ropey::Rope::from_str(&text);
        let regex = Regex::new("needle").unwrap();
        let (matches, truncated) = literal_match_spans(&rope, 0, usize::MAX, &regex, 80_000);
        assert_eq!(matches, vec![(0, 400_000, 6), (1, 0, 6)]);
        assert!(!truncated);
    }
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
