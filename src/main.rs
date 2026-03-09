use anyhow::Result;
use clap::Parser;
use crossterm::{
    event::{
        self, Event, KeyCode, KeyEvent, KeyEventKind, KeyModifiers, MouseEvent, MouseEventKind,
    },
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use ratatui::{
    prelude::*,
    widgets::{Block, Borders, Paragraph},
};
use ropey::Rope;
use serde::{Deserialize, Serialize};
use std::{
    fs,
    io::{self, stdout},
    path::PathBuf,
    time::{Duration, Instant},
};
use unicode_width::{UnicodeWidthChar, UnicodeWidthStr};

mod constants {
    use std::time::Duration;

    pub const DEFAULT_TAB_WIDTH: usize = 4;
    pub const STATUS_MESSAGE_TIMEOUT: Duration = Duration::from_secs(3);
    pub const FALLBACK_TERMINAL_HEIGHT: usize = 24;
    pub const MAX_UNDO_STACK_SIZE: usize = 100;
    pub const EVENT_POLL_INTERVAL: Duration = Duration::from_millis(100);
}

/// Mathematical coordinate system for robust rendering

mod syntax;
use syntax::SyntaxHighlighter;
// Removed unused coordinate validation system

#[derive(Clone, Debug)]
struct UndoState {
    rope: Rope,
    cursor_pos: (usize, usize),
}

/// Configuration settings for the editor
#[derive(Serialize, Deserialize, Clone, Debug)]
struct Config {
    mouse_enabled: bool,
    show_line_numbers: bool,
    tab_width: usize,
    word_wrap: bool,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            mouse_enabled: true,
            show_line_numbers: false,
            tab_width: constants::DEFAULT_TAB_WIDTH,
            word_wrap: false,
        }
    }
}

#[derive(Parser)]
#[command(author, version, about, long_about = None)]
struct Cli {
    /// File to edit
    file: Option<PathBuf>,
}

/// Main editor state containing document content, cursor position, and UI state
struct Editor {
    rope: Rope,
    cursor_pos: (usize, usize),      // (line, column) - legacy field for compatibility
    viewport_offset: (usize, usize), // (vertical, horizontal) scroll offset
    file_path: Option<PathBuf>,
    modified: bool,
    status_message: String,
    status_message_time: Option<Instant>,
    status_message_timeout: Duration,
    highlighter: SyntaxHighlighter,
    syntax_name: Option<String>,
    input_mode: InputMode,
    filename_buffer: String,
    quit_after_save: bool,
    mouse_enabled: bool,
    show_line_numbers: bool,
    tab_width: usize,
    word_wrap: bool,
    search_buffer: String,
    replace_buffer: String,
    search_matches: Vec<(usize, usize)>,
    current_match_index: Option<usize>,
    search_start_pos: (usize, usize),
    undo_stack: Vec<UndoState>,
    redo_stack: Vec<UndoState>,
    // Performance optimizations
    needs_redraw: bool,
    cached_text: Option<String>, // Cache for search operations
    cache_valid: bool,
    // Enhanced search features
    use_regex: bool,
    case_sensitive: bool,
    search_history: Vec<String>,
    search_history_index: Option<usize>,
    find_navigation_mode: FindNavigationMode,
}

/// Navigation mode within find functionality
#[derive(Debug, Clone, PartialEq)]
enum FindNavigationMode {
    HistoryBrowsing,   // arrows navigate search history
    ResultNavigation,  // arrows navigate search results
}

/// Different input modes the editor can be in
#[derive(Debug, Clone, PartialEq)]
enum InputMode {
    Normal,
    EnteringFilename,
    EnteringSaveAs,
    ConfirmQuit,
    OptionsMenu,
    Find,
    FindOptionsMenu,
    Replace,
    ReplaceConfirm,
    GoToLine,
    Help,
}

impl Editor {
    fn new() -> Self {
        let mut editor = Self {
            rope: Rope::new(),
            cursor_pos: (0, 0),
            viewport_offset: (0, 0),
            file_path: None,
            modified: false,
            status_message: String::new(),
            status_message_time: None,
            status_message_timeout: constants::STATUS_MESSAGE_TIMEOUT,
            highlighter: SyntaxHighlighter::new(),
            syntax_name: None,
            input_mode: InputMode::Normal,
            filename_buffer: String::new(),
            quit_after_save: false,
            mouse_enabled: true,
            show_line_numbers: false,
            tab_width: 4,
            word_wrap: false,
            search_buffer: String::new(),
            replace_buffer: String::new(),
            search_matches: Vec::new(),
            current_match_index: None,
            search_start_pos: (0, 0),
            undo_stack: Vec::new(),
            redo_stack: Vec::new(),
            // Performance optimizations
            needs_redraw: true,
            cached_text: None,
            cache_valid: false,
            // Enhanced search features
            use_regex: false,
            case_sensitive: false,
            search_history: Vec::new(),
            search_history_index: None,
            find_navigation_mode: FindNavigationMode::HistoryBrowsing,
        };
        editor.load_config();
        editor
    }

    fn load_file(&mut self, path: PathBuf) -> Result<()> {
        let content = fs::read_to_string(&path)?;
        self.rope = Rope::from_str(&content);

        // Detect syntax for highlighting
        let first_line = self.rope.line(0).as_str().map(|s| s.trim_end_matches('\n'));
        self.syntax_name = self.highlighter.detect_syntax(Some(&path), first_line);

        // Set the syntax in the highlighter
        self.highlighter.set_syntax(self.syntax_name.as_deref());

        self.file_path = Some(path);
        self.modified = false;
        Ok(())
    }

    fn save_file(&mut self) -> Result<()> {
        if let Some(path) = &self.file_path {
            self.perform_save(path.clone())?;
        } else {
            // No filename - prompt for one
            self.start_filename_input();
        }
        Ok(())
    }

    fn save_as(&mut self) {
        self.start_save_as_input();
    }

    fn start_filename_input(&mut self) {
        self.input_mode = InputMode::EnteringFilename;
        self.filename_buffer.clear();
        self.status_message = "File Name to Write: ".to_string();
        self.needs_redraw = true;
    }

    fn start_save_as_input(&mut self) {
        self.input_mode = InputMode::EnteringSaveAs;
        self.filename_buffer = self
            .file_path
            .as_ref()
            .map(|p| p.display().to_string())
            .unwrap_or_default();
        self.status_message = format!("File Name to Write: {}", self.filename_buffer);
        self.needs_redraw = true;
    }

    fn perform_save(&mut self, path: PathBuf) -> Result<()> {
        // Create parent directories if they don't exist
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
        }

        // Try to write the file
        match fs::write(&path, self.rope.to_string()) {
            Ok(()) => {
                self.file_path = Some(path.clone());
                self.modified = false;

                // Detect syntax for the new file
                let first_line = self.rope.line(0).as_str().map(|s| s.trim_end_matches('\n'));
                self.syntax_name = self.highlighter.detect_syntax(Some(&path), first_line);

                // Set the syntax in the highlighter
                self.highlighter.set_syntax(self.syntax_name.as_deref());

                self.set_temporary_status_message(format!("Saved: {}", path.display()));
            }
            Err(e) => {
                self.set_temporary_status_message(format!("Error saving file: {e}"));
            }
        }
        Ok(())
    }

    fn finish_filename_input(&mut self) -> Result<bool> {
        if self.filename_buffer.is_empty() {
            self.status_message = "Cancelled".to_string();
            self.input_mode = InputMode::Normal;
            self.quit_after_save = false;
            return Ok(false);
        }

        let path = PathBuf::from(&self.filename_buffer);

        // Check if file exists and warn user
        if path.exists() && self.input_mode == InputMode::EnteringFilename {
            // For now, just overwrite. Could add confirmation later.
        }

        self.perform_save(path)?;
        self.input_mode = InputMode::Normal;
        self.filename_buffer.clear();

        // Check if we should quit after saving
        let should_quit = self.quit_after_save && !self.modified; // Only quit if save succeeded
        self.quit_after_save = false;
        Ok(should_quit)
    }

    fn cancel_filename_input(&mut self) {
        self.input_mode = InputMode::Normal;
        self.filename_buffer.clear();
        self.quit_after_save = false;
        self.status_message = "Cancelled".to_string();
    }

    fn try_quit(&mut self) -> bool {
        if self.modified {
            self.start_quit_confirmation();
            false // Don't quit yet
        } else {
            true // Safe to quit immediately
        }
    }

    fn start_quit_confirmation(&mut self) {
        self.input_mode = InputMode::ConfirmQuit;
        self.status_message = "Save modified buffer? (Y/N/Ctrl+C)".to_string();
        self.needs_redraw = true;
    }

    fn handle_quit_confirmation(&mut self, save: bool) -> Result<bool> {
        self.input_mode = InputMode::Normal;

        if save {
            // Try to save before quitting
            if self.file_path.is_some() {
                // Has filename, save directly
                self.save_file()?;
                if !self.modified {
                    Ok(true) // Save succeeded, quit
                } else {
                    // Save failed, don't quit
                    Ok(false)
                }
            } else {
                // No filename, need to prompt for one
                self.quit_after_save = true;
                self.start_filename_input();
                // We'll quit after the save completes
                Ok(false)
            }
        } else {
            // User chose not to save, quit anyway
            Ok(true)
        }
    }

    fn cancel_quit_confirmation(&mut self) {
        self.input_mode = InputMode::Normal;
        self.status_message = "Cancelled".to_string();
        self.needs_redraw = true;
    }

    fn insert_char(&mut self, c: char) {
        self.save_undo_state();
        let pos = self.line_col_to_char_idx(self.cursor_pos.0, self.cursor_pos.1);
        self.rope.insert_char(pos, c);

        // Invalidate highlighting cache from current line
        self.highlighter
            .invalidate_cache_from_line(self.cursor_pos.0);
        
        // Performance optimizations
        self.invalidate_cache();
        self.needs_redraw = true;

        self.move_cursor_right();
        self.modified = true;
    }

    fn delete_char(&mut self) {
        if self.cursor_pos.1 > 0 {
            let pos = self.line_col_to_char_idx(self.cursor_pos.0, self.cursor_pos.1);
            if pos > 0 {
                self.save_undo_state();
                self.rope.remove(pos - 1..pos);

                // Invalidate highlighting cache from current line
                self.highlighter
                    .invalidate_cache_from_line(self.cursor_pos.0);
                
                // Performance optimizations
                self.invalidate_cache();
                self.needs_redraw = true;

                self.move_cursor_left();
                self.modified = true;
            }
        } else if self.cursor_pos.0 > 0 {
            // Join with previous line - cursor should stay at junction point
            let pos = self.line_col_to_char_idx(self.cursor_pos.0, 0);
            if pos > 0 {
                self.save_undo_state();

                // Get the length of the previous line before joining
                let prev_line = self.rope.line(self.cursor_pos.0 - 1);
                let junction_col = if let Some(line_str) = prev_line.as_str() {
                    line_str.trim_end_matches('\n').len()
                } else {
                    0
                };

                self.rope.remove(pos - 1..pos);

                // Invalidate highlighting cache from previous line (since we're joining)
                self.highlighter
                    .invalidate_cache_from_line(self.cursor_pos.0 - 1);
                
                // Performance optimizations
                self.invalidate_cache();
                self.needs_redraw = true;

                // Move cursor to junction point, not end of combined line
                self.cursor_pos.0 -= 1;
                self.cursor_pos.1 = junction_col;
                self.modified = true;
            }
        }
    }

    fn insert_newline(&mut self) {
        self.save_undo_state();
        let pos = self.line_col_to_char_idx(self.cursor_pos.0, self.cursor_pos.1);
        self.rope.insert_char(pos, '\n');

        // Invalidate highlighting cache from current line
        self.highlighter
            .invalidate_cache_from_line(self.cursor_pos.0);
        
        // Performance optimizations
        self.invalidate_cache();
        self.needs_redraw = true;

        self.cursor_pos.0 += 1;
        self.cursor_pos.1 = 0;
        self.modified = true;
    }

    fn move_cursor_up(&mut self) {
        if self.cursor_pos.0 > 0 {
            self.cursor_pos.0 -= 1;
            self.clamp_cursor_to_line();
            self.needs_redraw = true;
        }
    }

    fn move_cursor_down(&mut self) {
        if self.cursor_pos.0 < self.rope.len_lines().saturating_sub(1) {
            self.cursor_pos.0 += 1;
            self.clamp_cursor_to_line();
            self.needs_redraw = true;
        }
    }

    fn move_cursor_left(&mut self) {
        if self.cursor_pos.1 > 0 {
            self.cursor_pos.1 -= 1;
            self.needs_redraw = true;
        } else if self.cursor_pos.0 > 0 {
            self.cursor_pos.0 -= 1;
            if let Some(line) = self.rope.line(self.cursor_pos.0).as_str() {
                self.cursor_pos.1 = line.trim_end_matches('\n').width();
            }
            self.needs_redraw = true;
        }
    }

    fn move_cursor_right(&mut self) {
        if let Some(line) = self.rope.line(self.cursor_pos.0).as_str() {
            let line_len = line.trim_end_matches('\n').width();
            if self.cursor_pos.1 < line_len {
                self.cursor_pos.1 += 1;
                self.needs_redraw = true;
            } else if self.cursor_pos.0 < self.rope.len_lines().saturating_sub(1) {
                self.cursor_pos.0 += 1;
                self.cursor_pos.1 = 0;
                self.needs_redraw = true;
            }
        }
    }

    fn page_up(&mut self) {
        let terminal_height: usize = constants::FALLBACK_TERMINAL_HEIGHT; // Approximate terminal height
        let page_size = terminal_height.saturating_sub(4); // Leave room for status/help bars
        self.cursor_pos.0 = self.cursor_pos.0.saturating_sub(page_size);
        self.clamp_cursor_to_line();
        self.needs_redraw = true;
    }

    fn page_down(&mut self) {
        let terminal_height: usize = constants::FALLBACK_TERMINAL_HEIGHT; // Approximate terminal height
        let page_size = terminal_height.saturating_sub(4); // Leave room for status/help bars
        let max_line = self.rope.len_lines().saturating_sub(1);
        self.cursor_pos.0 = (self.cursor_pos.0 + page_size).min(max_line);
        self.clamp_cursor_to_line();
        self.needs_redraw = true;
    }

    fn clamp_cursor_to_line(&mut self) {
        if let Some(line) = self.rope.line(self.cursor_pos.0).as_str() {
            let line_len = line.trim_end_matches('\n').width();
            self.cursor_pos.1 = self.cursor_pos.1.min(line_len);
        }
    }

    fn line_col_to_char_idx(&self, line: usize, col: usize) -> usize {
        let line_start = self.rope.line_to_char(line);
        if let Some(line_str) = self.rope.line(line).as_str() {
            let mut char_idx = 0;
            let mut display_col = 0;
            for (i, ch) in line_str.chars().enumerate() {
                if display_col >= col || ch == '\n' {
                    break;
                }
                char_idx = i + 1;
                display_col += ch.width().unwrap_or(0);
            }
            line_start + char_idx
        } else {
            line_start
        }
    }

    fn update_viewport(&mut self, _terminal_width: u16, terminal_height: u16) {
        // Calculate available editor height (subtract 2 for status and help bars)
        let editor_height = terminal_height.saturating_sub(2) as usize;
        
        // Simple, direct viewport calculation
        if self.cursor_pos.0 < self.viewport_offset.0 {
            // Cursor above viewport - scroll up
            self.viewport_offset.0 = self.cursor_pos.0;
        } else if self.cursor_pos.0 >= self.viewport_offset.0 + editor_height {
            // Cursor below viewport - scroll down
            self.viewport_offset.0 = self.cursor_pos.0.saturating_sub(editor_height.saturating_sub(1));
        }
    }

    fn handle_mouse_event(&mut self, event: MouseEvent, terminal_height: usize) {
        match event.kind {
            MouseEventKind::Down(_) => {
                let clicked_line = self.viewport_offset.0 + event.row as usize;
                let clicked_col = event.column as usize;

                if clicked_line < self.rope.len_lines() {
                    self.cursor_pos.0 = clicked_line;
                    self.cursor_pos.1 = clicked_col;
                    self.clamp_cursor_to_line();
                    self.needs_redraw = true;
                }
            }
            MouseEventKind::Drag(_) => {
                // Mouse drag handling can be added here if needed
                self.needs_redraw = true;
            }
            MouseEventKind::ScrollDown => {
                if self.viewport_offset.0 < self.rope.len_lines().saturating_sub(terminal_height) {
                    self.viewport_offset.0 += 3;
                    self.needs_redraw = true;
                }
            }
            MouseEventKind::ScrollUp => {
                self.viewport_offset.0 = self.viewport_offset.0.saturating_sub(3);
                self.needs_redraw = true;
            }
            _ => {}
        }
    }

    fn toggle_mouse_mode(&mut self) {
        self.mouse_enabled = !self.mouse_enabled;

        // Actually enable/disable mouse capture at terminal level
        if self.mouse_enabled {
            let _ = crossterm::execute!(stdout(), crossterm::event::EnableMouseCapture);
            self.set_temporary_status_message("Mouse mode enabled".to_string());
        } else {
            let _ = crossterm::execute!(stdout(), crossterm::event::DisableMouseCapture);
            self.set_temporary_status_message("Mouse mode disabled".to_string());
        }
    }

    fn open_options_menu(&mut self) {
        self.input_mode = InputMode::OptionsMenu;
        self.status_message = "Options Menu".to_string();
        self.needs_redraw = true;
    }

    fn set_temporary_status_message(&mut self, message: String) {
        self.status_message = message;
        self.status_message_time = Some(Instant::now());
        self.needs_redraw = true;
    }

    fn check_status_message_timeout(&mut self) -> bool {
        if let Some(time) = self.status_message_time {
            if time.elapsed() >= self.status_message_timeout {
                self.status_message.clear();
                self.status_message_time = None;
                return true; // Status changed, need redraw
            }
        }
        false
    }

    fn save_config(&self) {
        let config = Config {
            mouse_enabled: self.mouse_enabled,
            show_line_numbers: self.show_line_numbers,
            tab_width: self.tab_width,
            word_wrap: self.word_wrap,
        };

        if let Some(config_dir) = dirs::config_dir() {
            let rune_config_dir = config_dir.join("rune");
            if fs::create_dir_all(&rune_config_dir).is_err() {
                return; // Silently fail if we can't create the directory
            }

            let config_path = rune_config_dir.join("config.toml");
            if let Ok(config_str) = toml::to_string(&config) {
                let _ = fs::write(config_path, config_str); // Silently fail if we can't write
            }
        }
    }

    fn load_config(&mut self) {
        if let Some(config_dir) = dirs::config_dir() {
            let config_path = config_dir.join("rune").join("config.toml");
            if let Ok(config_str) = fs::read_to_string(config_path) {
                if let Ok(config) = toml::from_str::<Config>(&config_str) {
                    self.mouse_enabled = config.mouse_enabled;
                    self.show_line_numbers = config.show_line_numbers;
                    self.tab_width = config.tab_width.max(1); // Ensure tab_width is at least 1
                    self.word_wrap = config.word_wrap;
                }
            }
        }
    }

    fn save_undo_state(&mut self) {
        let state = UndoState {
            rope: self.rope.clone(),
            cursor_pos: self.cursor_pos,
        };
        self.undo_stack.push(state);
        self.redo_stack.clear(); // Clear redo stack when new action happens

        // Limit undo stack size to prevent memory issues
        if self.undo_stack.len() > constants::MAX_UNDO_STACK_SIZE {
            self.undo_stack.remove(0);
        }
    }

    fn undo(&mut self) {
        if let Some(state) = self.undo_stack.pop() {
            let current_state = UndoState {
                rope: self.rope.clone(),
                cursor_pos: self.cursor_pos,
            };
            self.redo_stack.push(current_state);

            self.rope = state.rope;
            self.cursor_pos = state.cursor_pos;
            self.modified = true;
            
            // Performance optimizations
            self.invalidate_cache();
            self.needs_redraw = true;
            
            // Invalidate highlighting cache for entire document
            self.highlighter.invalidate_cache_from_line(0);
            
            self.set_temporary_status_message("Undo".to_string());
        }
    }

    fn redo(&mut self) {
        if let Some(state) = self.redo_stack.pop() {
            let current_state = UndoState {
                rope: self.rope.clone(),
                cursor_pos: self.cursor_pos,
            };
            self.undo_stack.push(current_state);

            self.rope = state.rope;
            self.cursor_pos = state.cursor_pos;
            self.modified = true;
            
            // Performance optimizations
            self.invalidate_cache();
            self.needs_redraw = true;
            
            // Invalidate highlighting cache for entire document
            self.highlighter.invalidate_cache_from_line(0);
            
            self.set_temporary_status_message("Redo".to_string());
        }
    }

    fn start_find(&mut self) {
        self.input_mode = InputMode::Find;
        self.search_buffer.clear();
        // Clear any previous search matches to prevent ghost highlights
        self.search_matches.clear();
        self.current_match_index = None;
        self.status_message = "Find: ".to_string();
        self.find_navigation_mode = FindNavigationMode::HistoryBrowsing;
        self.needs_redraw = true;
    }

    fn start_replace(&mut self) {
        self.input_mode = InputMode::Replace;
        self.search_buffer.clear();
        self.replace_buffer.clear();
        self.status_message = "Find: ".to_string();
        self.needs_redraw = true;
    }

    fn start_goto_line(&mut self) {
        self.input_mode = InputMode::GoToLine;
        self.search_buffer.clear();
        self.status_message = "Go to line: ".to_string();
        self.needs_redraw = true;
    }

    fn perform_find(&mut self, search_term: &str) -> bool {
        if search_term.is_empty() {
            self.search_matches.clear();
            self.current_match_index = None;
            return false;
        }

        // Store starting position to return to on cancel
        self.search_start_pos = self.cursor_pos;

        // Find all matches in the document
        self.search_matches = self.find_all_matches(search_term);

        if !self.search_matches.is_empty() {
            // Find the match closest to cursor position (at or after cursor)
            let cursor_char_idx = self.line_col_to_char_idx(self.cursor_pos.0, self.cursor_pos.1);

            self.current_match_index = self
                .search_matches
                .iter()
                .position(|(line, col)| {
                    let match_char_idx = self.line_col_to_char_idx(*line, *col);
                    match_char_idx >= cursor_char_idx
                })
                .or(Some(0)); // If no match at/after cursor, wrap to first match

            if let Some(index) = self.current_match_index {
                if let Some(&(line, col)) = self.search_matches.get(index) {
                    // Set cursor position
                    self.cursor_pos = (line, col);
                    self.clamp_cursor_to_line();
                    
                    // Handle viewport positioning for search results
                    
                    // Always position search results consistently at top of viewport
                    // This prevents cursor offset issues when searching repeatedly
                    self.viewport_offset.0 = line;
                    
                    self.needs_redraw = true;
                } else {
                    // Index is invalid, reset search state
                    self.current_match_index = None;
                }
            }

            true
        } else {
            self.current_match_index = None;
            false
        }
    }

    fn find_all_matches(&mut self, search_term: &str) -> Vec<(usize, usize)> {
        let mut matches = Vec::new();
        
        if search_term.is_empty() {
            return matches;
        }
        
        // BULLETPROOF: Search directly in rope line-by-line to avoid any text conversion issues
        for line_idx in 0..self.rope.len_lines() {
            if let Some(line_slice) = self.rope.line(line_idx).as_str() {
                // Get the actual line content
                let line_content = line_slice.trim_end_matches('\n');
                
                // Find all matches in this line with direct string validation
                let line_matches = if self.case_sensitive {
                    self.find_matches_in_line(line_content, search_term)
                } else {
                    self.find_matches_in_line(&line_content.to_lowercase(), &search_term.to_lowercase())
                };
                
                // Add validated matches for this line
                for col in line_matches {
                    // CRITICAL: Double-validate each match against original content
                    if self.validate_match_in_rope(line_idx, col, search_term) {
                        matches.push((line_idx, col));
                    }
                }
            }
        }
        
        // Sort matches by position for consistent navigation
        matches.sort_by(|a, b| a.0.cmp(&b.0).then(a.1.cmp(&b.1)));
        matches
    }
    
    // Find all occurrences of search_term in a single line
    fn find_matches_in_line(&self, line_content: &str, search_term: &str) -> Vec<usize> {
        let mut matches = Vec::new();
        let mut start_pos = 0;
        
        while let Some(pos) = line_content[start_pos..].find(search_term) {
            let match_pos = start_pos + pos;
            matches.push(match_pos);
            start_pos = match_pos + 1; // Move past this match to find overlapping ones
        }
        
        matches
    }
    
    // Validate that a match actually exists at the specified position in the rope
    fn validate_match_in_rope(&self, line_idx: usize, col: usize, search_term: &str) -> bool {
        // Get the actual line from rope
        if let Some(line_slice) = self.rope.line(line_idx).as_str() {
            let line_content = line_slice.trim_end_matches('\n');
            let line_chars: Vec<char> = line_content.chars().collect();
            let search_chars: Vec<char> = search_term.chars().collect();
            
            // Check bounds
            if col + search_chars.len() > line_chars.len() {
                return false;
            }
            
            // Extract text at position and validate
            let text_at_pos: String = line_chars[col..col + search_chars.len()].iter().collect();
            
            // Exact match validation
            if self.case_sensitive {
                text_at_pos == search_term
            } else {
                text_at_pos.to_lowercase() == search_term.to_lowercase()
            }
        } else {
            false
        }
    }

    fn invalidate_cache(&mut self) {
        self.cache_valid = false;
        self.cached_text = None;
    }

    fn find_next_match(&mut self) -> bool {
        if self.search_matches.is_empty() {
            return false;
        }

        if let Some(current_index) = self.current_match_index {
            if self.search_matches.is_empty() {
                self.current_match_index = None;
                return false;
            }
            let next_index = (current_index + 1) % self.search_matches.len();
            self.current_match_index = Some(next_index);
            if let Some(&(line, col)) = self.search_matches.get(next_index) {
                // Set cursor position
                self.cursor_pos = (line, col);
                self.clamp_cursor_to_line();
                
                // Always position search results consistently at top of viewport
                self.viewport_offset.0 = line;
                
                self.needs_redraw = true;
                true
            } else {
                // Index is invalid, reset search state
                self.current_match_index = None;
                false
            }
        } else {
            false
        }
    }

    fn find_previous_match(&mut self) -> bool {
        if self.search_matches.is_empty() {
            return false;
        }

        if let Some(current_index) = self.current_match_index {
            if self.search_matches.is_empty() {
                self.current_match_index = None;
                return false;
            }
            let prev_index = if current_index == 0 {
                self.search_matches.len() - 1
            } else {
                current_index - 1
            };
            self.current_match_index = Some(prev_index);
            if let Some(&(line, col)) = self.search_matches.get(prev_index) {
                // Set cursor position
                self.cursor_pos = (line, col);
                self.clamp_cursor_to_line();
                
                // Always position search results consistently at top of viewport
                self.viewport_offset.0 = line;
                
                self.needs_redraw = true;
                true
            } else {
                // Index is invalid, reset search state
                self.current_match_index = None;
                false
            }
        } else {
            false
        }
    }

    fn cancel_search(&mut self) {
        self.cursor_pos = self.search_start_pos;
        self.search_matches.clear();
        self.current_match_index = None;
    }

    fn perform_replace(&mut self, search_term: &str, replace_term: &str) -> usize {
        if search_term.is_empty() {
            return 0;
        }

        self.save_undo_state();
        let text = self.rope.to_string();
        let new_text = text.replace(search_term, replace_term);
        let replacements = text.matches(search_term).count();

        if replacements > 0 {
            self.rope = Rope::from_str(&new_text);
            self.modified = true;
            // Try to keep cursor in a reasonable position
            self.clamp_cursor_to_line();
            
            // Performance optimizations
            self.invalidate_cache();
            self.needs_redraw = true;
            
            // Invalidate highlighting cache for entire document
            self.highlighter.invalidate_cache_from_line(0);
        }

        replacements
    }

    fn perform_replace_interactive(&mut self, search_term: &str, replace_term: &str) -> usize {
        if search_term.is_empty() {
            return 0;
        }

        self.save_undo_state();
        let text = self.rope.to_string();
        
        // For interactive replace, only replace the first occurrence
        if let Some(pos) = text.find(search_term) {
            let mut new_text = text.clone();
            new_text.replace_range(pos..pos + search_term.len(), replace_term);
            
            self.rope = Rope::from_str(&new_text);
            self.modified = true;
            
            // Move cursor to the replaced text
            let line = self.rope.char_to_line(pos);
            let line_start = self.rope.line_to_char(line);
            let col = pos - line_start;
            self.cursor_pos = (line, col);
            self.clamp_cursor_to_line();
            
            // Performance optimizations
            self.invalidate_cache();
            self.needs_redraw = true;
            
            // Invalidate highlighting cache for entire document
            self.highlighter.invalidate_cache_from_line(0);
            
            return 1;
        }
        
        0
    }

    fn goto_line(&mut self, line_num: usize) {
        if line_num > 0 && line_num <= self.rope.len_lines() {
            self.cursor_pos.0 = line_num - 1; // Convert to 0-based
            self.cursor_pos.1 = 0;
            self.clamp_cursor_to_line();
            self.set_temporary_status_message(format!("Jumped to line {line_num}"));
        } else {
            self.set_temporary_status_message(format!("Invalid line number: {line_num}"));
        }
    }
    
    fn toggle_regex_mode(&mut self) {
        self.use_regex = !self.use_regex;
        let mode = if self.use_regex { "Regex" } else { "Literal" };
        self.set_temporary_status_message(format!("Search mode: {} ({})", 
            mode,
            if self.use_regex { "Pattern matching" } else { "Exact text" }
        ));
        self.needs_redraw = true;
        
        // Re-search if we have an active search
        if !self.search_buffer.is_empty() && self.input_mode == InputMode::Find {
            let search_term = self.search_buffer.clone();
            self.perform_find(&search_term);
        }
    }
    
    fn toggle_case_sensitive(&mut self) {
        self.case_sensitive = !self.case_sensitive;
        let mode = if self.case_sensitive { "Case sensitive" } else { "Case insensitive" };
        self.set_temporary_status_message(format!("Search: {}", mode));
        self.needs_redraw = true;
        
        // Re-search if we have an active search
        if !self.search_buffer.is_empty() && self.input_mode == InputMode::Find {
            let search_term = self.search_buffer.clone();
            self.perform_find(&search_term);
        }
    }
    
    fn add_to_search_history(&mut self, search_term: &str) {
        if !search_term.is_empty() {
            // Remove duplicates
            self.search_history.retain(|s| s != search_term);
            // Add to history
            self.search_history.push(search_term.to_string());
            // Limit history size
            if self.search_history.len() > 50 {
                self.search_history.remove(0);
            }
            // Reset history index
            self.search_history_index = None;
        }
    }

    fn navigate_search_history_up(&mut self) -> bool {
        if self.search_history.is_empty() {
            return false;
        }
        
        if let Some(current_index) = self.search_history_index {
            if current_index > 0 {
                self.search_history_index = Some(current_index - 1);
            } else {
                return false; // Already at oldest
            }
        } else {
            // Start from most recent
            self.search_history_index = Some(self.search_history.len() - 1);
        }
        
        if let Some(index) = self.search_history_index {
            if let Some(term) = self.search_history.get(index) {
                self.search_buffer = term.clone();
                self.status_message = format!("Find: {}", self.search_buffer);
                return true;
            }
        }
        false
    }

    fn navigate_search_history_down(&mut self) -> bool {
        if let Some(current_index) = self.search_history_index {
            if current_index < self.search_history.len() - 1 {
                self.search_history_index = Some(current_index + 1);
                if let Some(index) = self.search_history_index {
                    if let Some(term) = self.search_history.get(index) {
                        self.search_buffer = term.clone();
                    }
                }
                self.status_message = format!("Find: {}", self.search_buffer);
                return true;
            } else {
                // Go to empty search
                self.search_history_index = None;
                self.search_buffer.clear();
                self.status_message = "Find: ".to_string();
                return true;
            }
        }
        false
    }

    fn handle_tab_insertion(&mut self) {
        self.save_undo_state();
        
        // Calculate how many spaces to insert to reach the next tab stop
        let current_col = self.cursor_pos.1;
        let tab_width = self.tab_width.max(1); // Ensure tab_width is at least 1
        let spaces_to_next_tab = tab_width - (current_col % tab_width);
        
        // Insert the appropriate number of spaces
        for _ in 0..spaces_to_next_tab {
            self.insert_char(' ');
        }
    }
}


fn draw_help_modal(f: &mut Frame, area: Rect) {
    use ratatui::widgets::*;
    
    // Calculate modal size (fit content width)
    let modal_width = 48u16; // Fixed width to match content
    let modal_height = (area.height as f32 * 0.8) as u16;
    let modal_x = (area.width - modal_width) / 2;
    let modal_y = (area.height - modal_height) / 2;
    
    let modal_area = Rect {
        x: modal_x,
        y: modal_y,
        width: modal_width,
        height: modal_height,
    };

    // ASCII art logo and help content
    let help_content = r#"
 __________ ____ _____________________________
 \______   \    |   \      \   \_   _____/
  |       _/    |   /   |   \   |    __)_ 
  |    |   \    |  /    |    \  |        \
  |____|_  /______/\____|__  / /_______  /
         \/                \/          \/ 
            
         A nano-inspired text editor

─────────────────────────────────────────
               FILE OPERATIONS
─────────────────────────────────────────
^Q / ^X  Quit editor
^S       Save file  
^W       Save as (write file)
^O       Options menu

─────────────────────────────────────────
                 EDITING
─────────────────────────────────────────
^Z       Undo
^R       Redo
^K       Cut line
^U       Paste

─────────────────────────────────────────
               NAVIGATION  
─────────────────────────────────────────
^F       Find text
^\       Replace text
^G       Go to line
^V       Page down
^Y       Page up
Arrows   Move cursor

─────────────────────────────────────────
                OPTIONS
─────────────────────────────────────────
^O       Open options menu
  M      Toggle mouse mode
  L      Toggle line numbers  
  W      Toggle word wrap
  T      Set tab width

─────────────────────────────────────────
          Press ^H or Esc to close
─────────────────────────────────────────"#;

    // Clear the modal background
    let clear = Clear;
    f.render_widget(clear, modal_area);
    
    // Draw the modal
    let help_block = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(Color::Cyan))
        .style(Style::default().bg(Color::Black).fg(Color::White));
    
    let help_paragraph = Paragraph::new(help_content)
        .block(help_block)
        .alignment(ratatui::layout::Alignment::Center);
    
    f.render_widget(help_paragraph, modal_area);
}

fn main() -> Result<()> {
    let cli = Cli::parse();

    enable_raw_mode()?;
    execute!(stdout(), EnterAlternateScreen)?;

    let mut terminal = Terminal::new(CrosstermBackend::new(stdout()))?;
    terminal.clear()?;

    let mut editor = Editor::new();

    // Enable mouse capture only if configured to do so
    if editor.mouse_enabled {
        crossterm::execute!(stdout(), crossterm::event::EnableMouseCapture)?;
    }

    if let Some(file) = cli.file {
        editor.load_file(file)?;
    }

    let result = run_editor(&mut terminal, &mut editor);

    disable_raw_mode()?;
    execute!(stdout(), LeaveAlternateScreen)?;
    crossterm::execute!(stdout(), crossterm::event::DisableMouseCapture)?;

    result
}

fn run_editor(
    terminal: &mut Terminal<CrosstermBackend<io::Stdout>>,
    editor: &mut Editor,
) -> Result<()> {
    loop {
        // Update viewport before drawing to ensure correct cursor positioning
        editor.update_viewport(terminal.size()?.width, terminal.size()?.height);
        
        // Only redraw if something has changed
        if editor.needs_redraw {
            // Use synchronized output on macOS to reduce visual artifacts
            #[cfg(target_os = "macos")]
            {
                use crossterm::{execute, terminal::BeginSynchronizedUpdate};
                let _ = execute!(stdout(), BeginSynchronizedUpdate);
            }

            terminal.draw(|f| draw_ui(f, editor))?;

            #[cfg(target_os = "macos")]
            {
                use crossterm::{execute, terminal::EndSynchronizedUpdate};
                use std::io::Write;
                let _ = execute!(stdout(), EndSynchronizedUpdate);
                let _ = stdout().flush();
            }
            
            editor.needs_redraw = false;
        }

        // Check if status message should timeout
        let status_timeout = editor.check_status_message_timeout();
        if status_timeout {
            editor.needs_redraw = true;
        }

        if event::poll(constants::EVENT_POLL_INTERVAL)? {
            match event::read()? {
                Event::Key(key) => {
                    // Only handle key press events to avoid double registration on Windows
                    // and ensure consistent behavior across all platforms
                    if key.kind == KeyEventKind::Press && handle_key_event(editor, key)? {
                        break;
                    }
                }
                Event::Mouse(mouse) => {
                    if editor.mouse_enabled {
                        editor.handle_mouse_event(mouse, terminal.size()?.height as usize);
                    }
                }
                Event::Resize(_, _) => {
                    // Terminal was resized, trigger a redraw
                    editor.needs_redraw = true;
                }
                _ => {}
            }
        }
    }

    Ok(())
}

fn handle_key_event(editor: &mut Editor, key: KeyEvent) -> Result<bool> {
    // Handle quit confirmation first
    if editor.input_mode == InputMode::ConfirmQuit {
        match key.code {
            KeyCode::Char('y') | KeyCode::Char('Y') => {
                return editor.handle_quit_confirmation(true);
            }
            KeyCode::Char('n') | KeyCode::Char('N') => {
                return editor.handle_quit_confirmation(false);
            }
            KeyCode::Esc => {
                editor.cancel_quit_confirmation();
                return Ok(false);
            }
            _ if key.modifiers == KeyModifiers::CONTROL && key.code == KeyCode::Char('c') => {
                editor.cancel_quit_confirmation();
                return Ok(false);
            }
            _ => {}
        }
        return Ok(false);
    }

    // Handle options menu
    if editor.input_mode == InputMode::OptionsMenu {
        match key.code {
            KeyCode::Char('m') | KeyCode::Char('M') => {
                editor.toggle_mouse_mode();
                editor.input_mode = InputMode::Normal;
                editor.needs_redraw = true;
                return Ok(false);
            }
            KeyCode::Char('l') | KeyCode::Char('L') => {
                editor.show_line_numbers = !editor.show_line_numbers;
                editor.set_temporary_status_message(format!(
                    "Line numbers: {}",
                    if editor.show_line_numbers {
                        "ON"
                    } else {
                        "OFF"
                    }
                ));
                editor.input_mode = InputMode::Normal;
                editor.needs_redraw = true;
                return Ok(false);
            }
            KeyCode::Char('w') | KeyCode::Char('W') => {
                editor.word_wrap = !editor.word_wrap;
                editor.set_temporary_status_message(format!(
                    "Word wrap: {}",
                    if editor.word_wrap { "ON" } else { "OFF" }
                ));
                editor.input_mode = InputMode::Normal;
                editor.needs_redraw = true;
                return Ok(false);
            }
            KeyCode::Char('t') | KeyCode::Char('T') => {
                editor.tab_width = match editor.tab_width {
                    2 => 4,
                    4 => 8,
                    _ => 2,
                };
                editor.set_temporary_status_message(format!("Tab width: {}", editor.tab_width));
                editor.input_mode = InputMode::Normal;
                editor.needs_redraw = true;
                return Ok(false);
            }
            KeyCode::Esc => {
                editor.save_config();
                editor.input_mode = InputMode::Normal;
                editor.status_message.clear();
                editor.needs_redraw = true;
                return Ok(false);
            }
            _ if key.modifiers == KeyModifiers::CONTROL && key.code == KeyCode::Char('c') => {
                editor.save_config();
                editor.input_mode = InputMode::Normal;
                editor.status_message.clear();
                editor.needs_redraw = true;
                return Ok(false);
            }
            _ => {}
        }
        return Ok(false);
    }

    // Handle find options menu
    if editor.input_mode == InputMode::FindOptionsMenu {
        match key.code {
            KeyCode::Char('c') | KeyCode::Char('C') => {
                editor.toggle_case_sensitive();
                editor.set_temporary_status_message(format!(
                    "Case sensitivity: {}",
                    if editor.case_sensitive { "ON" } else { "OFF" }
                ));
                editor.input_mode = InputMode::Find;
                return Ok(false);
            }
            KeyCode::Char('r') | KeyCode::Char('R') => {
                editor.toggle_regex_mode();
                editor.input_mode = InputMode::Find;
                return Ok(false);
            }
            KeyCode::Esc => {
                editor.input_mode = InputMode::Find;
                return Ok(false);
            }
            _ if key.modifiers == KeyModifiers::CONTROL && key.code == KeyCode::Char('c') => {
                editor.input_mode = InputMode::Find;
                return Ok(false);
            }
            _ => {}
        }
        return Ok(false);
    }

    // Handle filename input modes
    if editor.input_mode == InputMode::EnteringFilename
        || editor.input_mode == InputMode::EnteringSaveAs
    {
        match key.code {
            KeyCode::Enter => {
                return editor.finish_filename_input();
            }
            KeyCode::Esc => {
                editor.cancel_filename_input();
            }
            KeyCode::Backspace => {
                editor.filename_buffer.pop();
                editor.status_message = format!("File Name to Write: {}", editor.filename_buffer);
                editor.needs_redraw = true;
            }
            KeyCode::Char(c) => {
                editor.filename_buffer.push(c);
                editor.status_message = format!("File Name to Write: {}", editor.filename_buffer);
                editor.needs_redraw = true;
            }
            _ => {}
        }
        return Ok(false);
    }

    // Handle find mode
    if editor.input_mode == InputMode::Find {
        match key.code {
            KeyCode::Char('r') if key.modifiers == KeyModifiers::CONTROL => {
                // Switch to replace mode from within find
                if !editor.search_buffer.is_empty() {
                    editor.input_mode = InputMode::Replace;
                    editor.replace_buffer.clear();
                    editor.status_message = format!("Replace '{}' with: ", editor.search_buffer);
                    editor.needs_redraw = true;
                } else {
                    editor.toggle_regex_mode();
                }
                return Ok(false);
            }
            KeyCode::Char('o') if key.modifiers == KeyModifiers::CONTROL => {
                editor.input_mode = InputMode::FindOptionsMenu;
                editor.needs_redraw = true;
                return Ok(false);
            }
            KeyCode::Enter => {
                // Add to search history
                let search_term = editor.search_buffer.clone();
                editor.add_to_search_history(&search_term);
                
                if editor.find_navigation_mode == FindNavigationMode::ResultNavigation && !editor.search_matches.is_empty() {
                    // If we already have matches and are in result mode, exit find mode
                    editor.input_mode = InputMode::Normal;
                    editor.search_matches.clear();
                    editor.current_match_index = None;
                    editor.search_buffer.clear();
                    editor.set_temporary_status_message("Search completed".to_string());
                } else {
                    // Perform search and switch to result navigation mode
                    let search_term = editor.search_buffer.clone();
                    if editor.perform_find(&search_term) {
                        editor.find_navigation_mode = FindNavigationMode::ResultNavigation;
                        let matches_count = editor.search_matches.len();
                        let current = editor.current_match_index.map(|i| i + 1).unwrap_or(1);
                        editor.status_message = format!(
                            "Find: {search_term} ({current}/{matches_count} matches) - Use ↑↓ to navigate, Enter/Esc to exit"
                        );
                    } else {
                        editor.set_temporary_status_message("Not found".to_string());
                        editor.input_mode = InputMode::Normal;
                    }
                }
            }
            KeyCode::Esc => {
                editor.cancel_search();
                editor.input_mode = InputMode::Normal;
                editor.set_temporary_status_message("Search cancelled".to_string());
            }
            KeyCode::Char('c') if key.modifiers == KeyModifiers::CONTROL => {
                editor.cancel_search();
                editor.input_mode = InputMode::Normal;
                editor.set_temporary_status_message("Search cancelled".to_string());
            }
            KeyCode::Up | KeyCode::Left => {
                if key.code == KeyCode::Up && editor.find_navigation_mode == FindNavigationMode::HistoryBrowsing {
                    // Navigate search history
                    if editor.navigate_search_history_up() {
                        editor.needs_redraw = true;
                        // Perform search with historical term but stay in history mode
                        if !editor.search_buffer.is_empty() {
                            let search_term = editor.search_buffer.clone();
                            editor.perform_find(&search_term);
                        }
                    } else {
                        editor.move_cursor_up();
                    }
                } else if editor.find_navigation_mode == FindNavigationMode::ResultNavigation && !editor.search_matches.is_empty() {
                    // Navigate to previous match
                    editor.find_previous_match();
                    let matches_count = editor.search_matches.len();
                    let current = editor.current_match_index.map(|i| i + 1).unwrap_or(1);
                    editor.status_message = format!(
                        "Find: {} ({current}/{matches_count} matches) - Use arrows to navigate, Enter/Esc to exit",
                        editor.search_buffer
                    );
                    editor.needs_redraw = true;
                } else {
                    // Default cursor movement
                    if key.code == KeyCode::Up {
                        editor.move_cursor_up();
                    } else {
                        editor.move_cursor_left();
                    }
                }
            }
            KeyCode::Down | KeyCode::Right => {
                if key.code == KeyCode::Down && editor.find_navigation_mode == FindNavigationMode::HistoryBrowsing {
                    // Navigate search history
                    if editor.navigate_search_history_down() {
                        editor.needs_redraw = true;
                        // Perform search with historical term (or clear if now empty)
                        if !editor.search_buffer.is_empty() {
                            let search_term = editor.search_buffer.clone();
                            editor.perform_find(&search_term);
                        } else {
                            editor.search_matches.clear();
                            editor.current_match_index = None;
                        }
                    } else {
                        editor.move_cursor_down();
                    }
                } else if editor.find_navigation_mode == FindNavigationMode::ResultNavigation && !editor.search_matches.is_empty() {
                    // Navigate to next match
                    editor.find_next_match();
                    let matches_count = editor.search_matches.len();
                    let current = editor.current_match_index.map(|i| i + 1).unwrap_or(1);
                    editor.status_message = format!(
                        "Find: {} ({current}/{matches_count} matches) - Use arrows to navigate, Enter/Esc to exit",
                        editor.search_buffer
                    );
                    editor.needs_redraw = true;
                } else {
                    // Default cursor movement  
                    if key.code == KeyCode::Down {
                        editor.move_cursor_down();
                    } else {
                        editor.move_cursor_right();
                    }
                }
            }
            KeyCode::Backspace => {
                editor.search_buffer.pop();
                if !editor.search_buffer.is_empty() {
                    // Re-search with updated term
                    let search_term = editor.search_buffer.clone();
                    if editor.perform_find(&search_term) {
                        let matches_count = editor.search_matches.len();
                        let current = editor.current_match_index.map(|i| i + 1).unwrap_or(1);
                        editor.status_message = format!(
                            "Find: {search_term} ({current}/{matches_count} matches) - Use ↑↓ to navigate, Enter/Esc to exit"
                        );
                    } else {
                        editor.status_message =
                            format!("Find: {} (no matches)", editor.search_buffer);
                    }
                } else {
                    // Switch back to history browsing when buffer is empty
                    editor.find_navigation_mode = FindNavigationMode::HistoryBrowsing;
                    editor.status_message = "Find: ".to_string();
                    editor.search_matches.clear();
                    editor.current_match_index = None;
                    editor.needs_redraw = true;
                }
            }
            KeyCode::Char(c) => {
                editor.search_buffer.push(c);
                // Switch to result navigation mode when user types
                editor.find_navigation_mode = FindNavigationMode::ResultNavigation;
                // Re-search with updated term
                let search_term = editor.search_buffer.clone();
                if editor.perform_find(&search_term) {
                    let matches_count = editor.search_matches.len();
                    let current = editor.current_match_index.map(|i| i + 1).unwrap_or(1);
                    editor.status_message = format!(
                        "Find: {search_term} ({current}/{matches_count} matches) - Use ↑↓ to navigate, Enter/Esc to exit"
                    );
                    editor.needs_redraw = true;
                } else {
                    editor.status_message = format!("Find: {} (no matches)", editor.search_buffer);
                    editor.needs_redraw = true;
                }
            }
            _ => {}
        }
        return Ok(false);
    }

    // Handle replace mode
    if editor.input_mode == InputMode::Replace {
        match key.code {
            KeyCode::Char('o') if key.modifiers == KeyModifiers::CONTROL => {
                editor.input_mode = InputMode::FindOptionsMenu;
                editor.needs_redraw = true;
                return Ok(false);
            }
            KeyCode::Enter => {
                if editor.status_message.starts_with("Find:") {
                    // Switch to replace input
                    editor.status_message = format!("Replace '{}' with: ", editor.search_buffer);
                    editor.needs_redraw = true;
                } else {
                    // Move to replace confirmation mode
                    editor.input_mode = InputMode::ReplaceConfirm;
                    editor.status_message = format!(
                        "Replace '{}' with '{}'? Y: Replace This | N: Skip | A: Replace All | ^C: Cancel",
                        editor.search_buffer, editor.replace_buffer
                    );
                    editor.needs_redraw = true;
                }
            }
            KeyCode::Esc => {
                editor.input_mode = InputMode::Normal;
                editor.status_message = "Replace cancelled".to_string();
                editor.needs_redraw = true;
            }
            KeyCode::Char('c') if key.modifiers == KeyModifiers::CONTROL => {
                editor.input_mode = InputMode::Normal;
                editor.status_message = "Replace cancelled".to_string();
                editor.needs_redraw = true;
            }
            KeyCode::Backspace => {
                if editor.status_message.starts_with("Find:") {
                    editor.search_buffer.pop();
                    editor.status_message = format!("Find: {}", editor.search_buffer);
                    editor.needs_redraw = true;
                } else {
                    editor.replace_buffer.pop();
                    editor.status_message = format!(
                        "Replace '{}' with: {}",
                        editor.search_buffer, editor.replace_buffer
                    );
                    editor.needs_redraw = true;
                }
            }
            KeyCode::Char(c) => {
                if editor.status_message.starts_with("Find:") {
                    editor.search_buffer.push(c);
                    editor.status_message = format!("Find: {}", editor.search_buffer);
                    editor.needs_redraw = true;
                } else {
                    editor.replace_buffer.push(c);
                    editor.status_message = format!(
                        "Replace '{}' with: {}",
                        editor.search_buffer, editor.replace_buffer
                    );
                    editor.needs_redraw = true;
                }
            }
            _ => {}
        }
        return Ok(false);
    }

    // Handle replace confirmation mode
    if editor.input_mode == InputMode::ReplaceConfirm {
        match key.code {
            KeyCode::Char('y') | KeyCode::Char('Y') => {
                // Replace this one and continue to next
                let replacements = editor.perform_replace_interactive(
                    &editor.search_buffer.clone(),
                    &editor.replace_buffer.clone(),
                );
                if replacements > 0 {
                    // Continue to next match - stay in ReplaceConfirm mode
                    editor.status_message = format!(
                        "Replaced 1. Continue? Y: Replace This | N: Skip | A: Replace All | ^C: Cancel"
                    );
                } else {
                    editor.set_temporary_status_message("No more matches found".to_string());
                    editor.input_mode = InputMode::Normal;
                }
                return Ok(false);
            }
            KeyCode::Char('n') | KeyCode::Char('N') => {
                // Skip this one and continue to next
                // For now, just end the replacement (we can enhance this later)
                editor.input_mode = InputMode::Normal;
                editor.set_temporary_status_message("Replace skipped".to_string());
                return Ok(false);
            }
            KeyCode::Char('a') | KeyCode::Char('A') => {
                // Replace all remaining
                let replacements = editor.perform_replace(
                    &editor.search_buffer.clone(),
                    &editor.replace_buffer.clone(),
                );
                if replacements > 0 {
                    editor.set_temporary_status_message(format!(
                        "Replaced all {replacements} occurrence(s)"
                    ));
                } else {
                    editor.set_temporary_status_message("No matches found".to_string());
                }
                editor.input_mode = InputMode::Normal;
                return Ok(false);
            }
            KeyCode::Esc => {
                editor.input_mode = InputMode::Normal;
                editor.set_temporary_status_message("Replace cancelled".to_string());
                return Ok(false);
            }
            _ if key.modifiers == KeyModifiers::CONTROL && key.code == KeyCode::Char('c') => {
                editor.input_mode = InputMode::Normal;
                editor.set_temporary_status_message("Replace cancelled".to_string());
                return Ok(false);
            }
            _ => {}
        }
        return Ok(false);
    }

    // Handle go to line mode
    if editor.input_mode == InputMode::GoToLine {
        match key.code {
            KeyCode::Enter => {
                if let Ok(line_num) = editor.search_buffer.parse::<usize>() {
                    editor.goto_line(line_num);
                } else {
                    editor.set_temporary_status_message("Invalid line number".to_string());
                }
                editor.input_mode = InputMode::Normal;
            }
            KeyCode::Esc => {
                editor.input_mode = InputMode::Normal;
                editor.status_message = "Go to line cancelled".to_string();
                editor.needs_redraw = true;
            }
            KeyCode::Char('c') if key.modifiers == KeyModifiers::CONTROL => {
                editor.input_mode = InputMode::Normal;
                editor.status_message = "Go to line cancelled".to_string();
                editor.needs_redraw = true;
            }
            KeyCode::Backspace => {
                editor.search_buffer.pop();
                editor.status_message = format!("Go to line: {}", editor.search_buffer);
                editor.needs_redraw = true;
            }
            KeyCode::Char(c) if c.is_ascii_digit() => {
                editor.search_buffer.push(c);
                editor.status_message = format!("Go to line: {}", editor.search_buffer);
                editor.needs_redraw = true;
            }
            _ => {}
        }
        return Ok(false);
    }

    // Handle help mode
    if editor.input_mode == InputMode::Help {
        match key.code {
            KeyCode::Esc => {
                editor.input_mode = InputMode::Normal;
                editor.needs_redraw = true;
            }
            KeyCode::Char('c') if key.modifiers == KeyModifiers::CONTROL => {
                editor.input_mode = InputMode::Normal;
                editor.needs_redraw = true;
            }
            KeyCode::Char('h') if key.modifiers == KeyModifiers::CONTROL => {
                editor.input_mode = InputMode::Normal;
                editor.needs_redraw = true;
            }
            _ => {}
        }
        return Ok(false);
    }

    // Handle normal mode
    match (key.modifiers, key.code) {
        // Standard keybindings
        (KeyModifiers::CONTROL, KeyCode::Char('q')) => return Ok(editor.try_quit()),
        (KeyModifiers::CONTROL, KeyCode::Char('x')) => return Ok(editor.try_quit()), // nano compatibility
        (KeyModifiers::CONTROL, KeyCode::Char('s')) => {
            editor.save_file()?;
        }
        (KeyModifiers::CONTROL, KeyCode::Char('w')) => {
            editor.save_as();
        }
        (KeyModifiers::CONTROL, KeyCode::Char('v')) => {
            editor.page_down();
        }
        (KeyModifiers::CONTROL, KeyCode::Char('o')) => {
            editor.open_options_menu();
        }
        (KeyModifiers::CONTROL, KeyCode::Char('h')) => {
            editor.input_mode = InputMode::Help;
            editor.needs_redraw = true;
        }
        (KeyModifiers::CONTROL, KeyCode::Char('z')) => {
            editor.undo();
        }
        (KeyModifiers::CONTROL, KeyCode::Char('y')) => {
            editor.page_up();
        }
        (KeyModifiers::CONTROL, KeyCode::Char('f')) => {
            editor.start_find();
        }
        (KeyModifiers::CONTROL, KeyCode::Char('\\')) => {
            editor.start_replace();
        }
        (KeyModifiers::CONTROL, KeyCode::Char('g')) => {
            editor.start_goto_line();
        }

        (KeyModifiers::CONTROL, KeyCode::Char('r')) => {
            editor.redo();
        }

        // Navigation
        (_, KeyCode::Up) => editor.move_cursor_up(),
        (_, KeyCode::Down) => editor.move_cursor_down(),
        (_, KeyCode::Left) => editor.move_cursor_left(),
        (_, KeyCode::Right) => editor.move_cursor_right(),
        (_, KeyCode::PageUp) => editor.page_up(),
        (_, KeyCode::PageDown) => editor.page_down(),
        (_, KeyCode::Home) => editor.cursor_pos.1 = 0,
        (_, KeyCode::End) => {
            if let Some(line) = editor.rope.line(editor.cursor_pos.0).as_str() {
                editor.cursor_pos.1 = line.trim_end_matches('\n').width();
            }
        }

        // Editing
        (_, KeyCode::Char(c)) => editor.insert_char(c),
        (_, KeyCode::Tab) => {
            editor.handle_tab_insertion();
        }
        (_, KeyCode::Enter) => editor.insert_newline(),
        (_, KeyCode::Backspace) => editor.delete_char(),
        (_, KeyCode::Esc) => {} // Esc key - reserved for future use

        _ => {}
    }

    Ok(false)
}

fn apply_search_highlighting(
    syntax_spans: &[(Style, String)],
    line_content: &str,
    line_idx: usize,
    search_term: &str,
    search_matches: &[(usize, usize)],
    current_match_index: Option<usize>,
) -> Vec<Span<'static>> {
    if search_term.is_empty() || search_matches.is_empty() {
        // No search active - just apply syntax highlighting
        return syntax_spans
            .iter()
            .map(|(style, text)| {
                let clean_text = text.trim_end_matches('\n').to_string();
                Span::styled(clean_text, *style)
            })
            .collect();
    }

    // Find and validate all matches on this line with bulletproof checking
    let mut validated_matches: Vec<usize> = Vec::new();
    for (match_line, match_col) in search_matches {
        if *match_line == line_idx {
            // CRITICAL: Validate that there's actually a match at this position
            if validate_match_at_position(line_content, *match_col, search_term) {
                validated_matches.push(*match_col);
            }
        }
    }

    if validated_matches.is_empty() {
        // No valid matches on this line - just apply syntax highlighting
        return syntax_spans
            .iter()
            .map(|(style, text)| {
                let clean_text = text.trim_end_matches('\n').to_string();
                Span::styled(clean_text, *style)
            })
            .collect();
    }

    // Sort matches by position
    validated_matches.sort_unstable();

    let mut result_spans = Vec::new();
    let line_chars: Vec<char> = line_content.chars().collect();
    let search_chars: Vec<char> = search_term.chars().collect();
    let mut current_char_pos = 0;

    // Find which match is currently selected on this line
    let current_match_col = current_match_index
        .and_then(|idx| search_matches.get(idx))
        .filter(|(match_line, _)| *match_line == line_idx)
        .map(|(_, match_col)| *match_col);

    // Process each validated match with character-based indexing
    for &match_char_pos in &validated_matches {
        // Add text before the match
        if match_char_pos > current_char_pos {
            let before_chars: String = line_chars[current_char_pos..match_char_pos].iter().collect();
            if !before_chars.is_empty() {
                result_spans.push(Span::styled(
                    before_chars,
                    get_syntax_style_at_position(syntax_spans, current_char_pos),
                ));
            }
        }

        // Add the highlighted match with character-based slicing
        let match_end_char = (match_char_pos + search_chars.len()).min(line_chars.len());
        let match_chars: String = line_chars[match_char_pos..match_end_char].iter().collect();

        let highlight_style = if Some(match_char_pos) == current_match_col {
            // Current match - bright red background
            Style::default().bg(Color::Red).fg(Color::White)
        } else {
            // Other matches - yellow background
            Style::default().bg(Color::Yellow).fg(Color::Black)
        };

        result_spans.push(Span::styled(match_chars, highlight_style));
        current_char_pos = match_end_char;
    }

    // Add remaining text after the last match
    if current_char_pos < line_chars.len() {
        let remaining_chars: String = line_chars[current_char_pos..].iter().collect();
        if !remaining_chars.is_empty() {
            result_spans.push(Span::styled(
                remaining_chars,
                get_syntax_style_at_position(syntax_spans, current_char_pos),
            ));
        }
    }

    result_spans
}

// Bulletproof match validation - ensures highlight only appears where text actually matches
fn validate_match_at_position(line_content: &str, char_pos: usize, search_term: &str) -> bool {
    let line_chars: Vec<char> = line_content.chars().collect();
    let search_chars: Vec<char> = search_term.chars().collect();

    // Check bounds
    if char_pos + search_chars.len() > line_chars.len() {
        return false;
    }

    // Extract the text at this position
    let text_at_pos: String = line_chars[char_pos..char_pos + search_chars.len()].iter().collect();

    // Validate it exactly matches the search term
    text_at_pos == search_term || text_at_pos.to_lowercase() == search_term.to_lowercase()
}

fn get_syntax_style_at_position(syntax_spans: &[(Style, String)], position: usize) -> Style {
    let mut current_pos = 0;
    for (style, text) in syntax_spans {
        let text_len = text.trim_end_matches('\n').len();
        if position >= current_pos && position < current_pos + text_len {
            return *style;
        }
        current_pos += text_len;
    }
    Style::default()
}


fn draw_ui(f: &mut Frame, editor: &mut Editor) {
    let area = f.area();

    // Bottom bar content based on mode
    let (help_left, help_right) = match editor.input_mode {
        InputMode::ConfirmQuit => ("Y: Save and quit  N: Quit without saving  ^C/Esc: Cancel".to_string(), String::new()),
        InputMode::EnteringFilename | InputMode::EnteringSaveAs => {
            ("Enter: Confirm  Esc: Cancel  Type filename".to_string(), String::new())
        }
        InputMode::OptionsMenu => ("M: Mouse  L: Line Numbers  W: Word Wrap  T: Tab Width  Esc: Back".to_string(), String::new()),
        InputMode::Find => ("Enter: Search/Exit  Esc/^C: Cancel  Arrows: Navigate  ^R: Replace  ^O: Options".to_string(), String::new()),
        InputMode::FindOptionsMenu => ("C: Case sensitivity  R: Regex mode  Esc: Back to find".to_string(), String::new()),
        InputMode::Replace => ("Enter: Next step  Esc/^C: Cancel  ^O: Options".to_string(), String::new()),
        InputMode::ReplaceConfirm => ("Y: Replace This  N: Skip  A: Replace All  ^C: Cancel".to_string(), String::new()),
        InputMode::GoToLine => ("Enter: Go  Esc/^C: Cancel  Type line number".to_string(), String::new()),
        InputMode::Help => ("^H Help".to_string(), format!("Rune v{}", env!("CARGO_PKG_VERSION"))),
        _ => ("^H Help".to_string(), format!("Rune v{}", env!("CARGO_PKG_VERSION"))),
    };
    let help_height = 1u16;

    // Main editor area (adjusted for dynamic help bar height)
    let editor_area = Rect {
        x: area.x,
        y: area.y,
        width: area.width,
        height: area.height.saturating_sub(1 + help_height),
    };

    // Status bar area
    let status_area = Rect {
        x: area.x,
        y: area.y + area.height.saturating_sub(1 + help_height),
        width: area.width,
        height: 1,
    };

    // Help bar area (dynamic height)
    let help_area = Rect {
        x: area.x,
        y: area.y + area.height.saturating_sub(help_height),
        width: area.width,
        height: help_height,
    };

    // Calculate line number width if line numbers are enabled
    let line_num_width = if editor.show_line_numbers {
        editor.rope.len_lines().to_string().len() + 1 // +1 for space
    } else {
        0
    };

    // Note: Line numbers are rendered inline with content, so no area adjustment needed

    // Draw editor content with lazy syntax highlighting
    let mut lines = vec![];
    let visible_lines = editor_area.height as usize;

    for i in 0..visible_lines {
        let line_idx = editor.viewport_offset.0 + i;
        if line_idx < editor.rope.len_lines() {
            if let Some(line_text) = editor.rope.line(line_idx).as_str() {
                // Use lazy highlighting - only highlight visible lines
                let highlighted_spans = editor.highlighter.highlight_line(line_idx, line_text);

                let mut styled_spans: Vec<Span> = vec![];

                // Add line number if enabled
                if editor.show_line_numbers {
                    let line_num = format!("{:width$} ", line_idx + 1, width = line_num_width - 1);
                    styled_spans.push(Span::styled(line_num, Style::default().fg(Color::DarkGray)));
                }

                // Add highlighted content with search highlighting
                let line_content = line_text.trim_end_matches('\n');

                styled_spans.extend(apply_search_highlighting(
                    &highlighted_spans,
                    line_content,
                    line_idx,
                    &editor.search_buffer,
                    &editor.search_matches,
                    editor.current_match_index,
                ));

                lines.push(Line::from(styled_spans));
            }
        } else {
            let mut styled_spans: Vec<Span> = vec![];

            // Add line number space if enabled
            if editor.show_line_numbers {
                let empty_line_num = format!("{:width$} ", "", width = line_num_width - 1);
                styled_spans.push(Span::styled(
                    empty_line_num,
                    Style::default().fg(Color::DarkGray),
                ));
            }

            styled_spans.push(Span::styled("~", Style::default().fg(Color::DarkGray)));

            lines.push(Line::from(styled_spans));
        }
    }

    let editor_widget = Paragraph::new(lines).block(Block::default().borders(Borders::NONE));

    f.render_widget(editor_widget, editor_area);

    // Draw cursor
    let cursor_screen_y = editor.cursor_pos.0.saturating_sub(editor.viewport_offset.0);
    if cursor_screen_y < visible_lines {
        let cursor_x = if editor.show_line_numbers {
            editor.cursor_pos.1 as u16 + line_num_width as u16
        } else {
            editor.cursor_pos.1 as u16
        };
        f.set_cursor_position(Position::new(cursor_x, cursor_screen_y as u16));
    }

    // Draw status bar
    let status_text = if !editor.status_message.is_empty() {
        editor.status_message.clone()
    } else {
        let modified_indicator = if editor.modified { "[+]" } else { "" };
        let filename = editor
            .file_path
            .as_ref()
            .map(|p| p.display().to_string())
            .unwrap_or_else(|| "[No Name]".to_string());
        let search_modes = if editor.input_mode == InputMode::Find {
            format!(" | Search: {} {}",
                if editor.use_regex { "Regex" } else { "Literal" },
                if editor.case_sensitive { "(Case)" } else { "(NoCase)" }
            )
        } else {
            String::new()
        };
        
        format!(
            "{} {} | Ln {}, Col {} | Mouse: {}{}",
            filename,
            modified_indicator,
            editor.cursor_pos.0 + 1,
            editor.cursor_pos.1 + 1,
            if editor.mouse_enabled { "ON" } else { "OFF" },
            search_modes
        )
    };

    let status_widget =
        Paragraph::new(status_text).style(Style::default().bg(Color::DarkGray).fg(Color::White));

    f.render_widget(status_widget, status_area);

    // Draw help bar (split left/right or full width)
    use ratatui::text::{Line, Span};
    let help_line = if help_right.is_empty() {
        // Full width text for modes like options menu
        Line::from(Span::raw(&help_left))
    } else {
        // Split layout for normal mode
        let remaining_space = help_area.width as usize - help_left.len() - help_right.len();
        let spaces = " ".repeat(remaining_space.max(1));
        Line::from(vec![
            Span::raw(&help_left),
            Span::raw(spaces),
            Span::raw(&help_right),
        ])
    };
    
    let help_widget = Paragraph::new(help_line)
        .style(Style::default().bg(Color::Cyan).fg(Color::Black));

    f.render_widget(help_widget, help_area);

    // Draw help modal if in help mode
    if editor.input_mode == InputMode::Help {
        draw_help_modal(f, area);
    }
}
