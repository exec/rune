pub use rune_core::{document, file_io, get_line_str, search, Document, EditError, Selection};

pub mod constants {
    pub use rune_core::constants::{
        DEFAULT_TAB_WIDTH, FALLBACK_TERMINAL_HEIGHT, SEARCH_HISTORY_LIMIT, UNDO_STACK_LIMIT,
    };
    use std::time::Duration;
    pub const STATUS_MESSAGE_TIMEOUT: Duration = Duration::from_secs(3);
    pub const EVENT_POLL_INTERVAL: Duration = Duration::from_millis(100);
    pub const SCROLL_SPEED: usize = 3;
}

#[cfg(feature = "terminal")]
pub mod backend;
#[cfg(feature = "terminal")]
pub mod config;
#[cfg(feature = "terminal")]
pub mod editor;
#[cfg(not(feature = "terminal"))]
pub use rune_core::editor;
pub mod fuzzy;
pub mod hex;
#[cfg(feature = "terminal")]
pub mod input;
#[cfg(feature = "terminal")]
pub mod syntax;
#[cfg(not(feature = "terminal"))]
pub use rune_core::syntax;
#[cfg(feature = "terminal")]
pub mod tabs;
#[cfg(feature = "terminal")]
pub mod ui;
#[cfg(feature = "terminal")]
pub mod updater;
