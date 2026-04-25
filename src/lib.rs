pub mod constants {
    use std::time::Duration;

    pub const DEFAULT_TAB_WIDTH: usize = 4;
    pub const STATUS_MESSAGE_TIMEOUT: Duration = Duration::from_secs(3);
    pub const FALLBACK_TERMINAL_HEIGHT: usize = 24;
    pub const EVENT_POLL_INTERVAL: Duration = Duration::from_millis(100);
    pub const SCROLL_SPEED: usize = 3;
    pub const SEARCH_HISTORY_LIMIT: usize = 50;
    pub const UNDO_STACK_LIMIT: usize = 100;
}

/// Get a line's text content as a String, handling chunk boundaries in the rope.
pub fn get_line_str(rope: &ropey::Rope, line: usize) -> String {
    let rope_line = rope.line(line);
    match rope_line.as_str() {
        Some(s) => s.to_string(),
        None => rope_line.chars().collect(),
    }
}

pub mod config;
pub mod editor;
pub mod fuzzy;
pub mod hex;
pub mod input;
pub mod search;
pub mod syntax;
pub mod tabs;
pub mod ui;
pub mod updater;
