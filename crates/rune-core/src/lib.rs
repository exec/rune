//! Shared document editing, safe file I/O, search, and syntax roles.
//! Frontends should use [`Document`]; positions count Unicode scalar values.

pub mod constants {
    pub const DEFAULT_TAB_WIDTH: usize = 4;
    pub const UNDO_STACK_LIMIT: usize = 100;
    pub const SEARCH_HISTORY_LIMIT: usize = 50;
    // Legacy column-based navigation retains its existing fallback during extraction.
    pub const FALLBACK_TERMINAL_HEIGHT: usize = 24;
}

pub mod document;
pub use document::{Document, EditError, Selection};
pub mod file_io;
pub mod search;
pub mod syntax;

/// Legacy column-based buffer internals retained for Rune's terminal frontend.
/// New frontends use [`Document`] instead of depending on viewport/column assumptions.
#[doc(hidden)]
pub mod editor;
#[doc(hidden)]
pub mod hex;

/// Get a line's text content as a String, handling chunk boundaries in the rope.
pub fn get_line_str(rope: &ropey::Rope, line: usize) -> String {
    let rope_line = rope.line(line);
    match rope_line.as_str() {
        Some(s) => s.to_string(),
        None => rope_line.chars().collect(),
    }
}
