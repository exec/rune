use anyhow::Result;
use clap::Parser;
use crossterm::{
    event::{self, Event, KeyCode, KeyEvent, KeyModifiers, MouseEvent, MouseEventKind},
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use ratatui::{
    prelude::*,
    widgets::{Block, Borders, Paragraph, Wrap},
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

mod syntax;
use syntax::SyntaxHighlighter;

#[derive(Clone, Debug)]
struct UndoState {
    rope: Rope,
    cursor_pos: (usize, usize),
}

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
            tab_width: 4,
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

struct Editor {
    rope: Rope,
    cursor_pos: (usize, usize),      // (line, column)
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
}

#[derive(Debug, Clone, PartialEq)]
enum InputMode {
    Normal,
    EnteringFilename,
    EnteringSaveAs,
    ConfirmQuit,
    OptionsMenu,
    Find,
    Replace,
    GoToLine,
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
            status_message_timeout: Duration::from_secs(3),
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
    }

    fn start_save_as_input(&mut self) {
        self.input_mode = InputMode::EnteringSaveAs;
        self.filename_buffer = self
            .file_path
            .as_ref()
            .map(|p| p.display().to_string())
            .unwrap_or_default();
        self.status_message = "File Name to Write: ".to_string();
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
    }

    fn insert_char(&mut self, c: char) {
        self.save_undo_state();
        let pos = self.line_col_to_char_idx(self.cursor_pos.0, self.cursor_pos.1);
        self.rope.insert_char(pos, c);

        // Invalidate highlighting cache from current line
        self.highlighter
            .invalidate_cache_from_line(self.cursor_pos.0);

        self.move_cursor_right();
        self.modified = true;
    }

    fn insert_tab(&mut self) {
        self.save_undo_state();
        let pos = self.line_col_to_char_idx(self.cursor_pos.0, self.cursor_pos.1);
        
        // Insert the spaces as a single operation
        let spaces = " ".repeat(self.tab_width);
        self.rope.insert(pos, &spaces);

        // Invalidate highlighting cache from current line
        self.highlighter
            .invalidate_cache_from_line(self.cursor_pos.0);

        // Move cursor forward by tab_width spaces
        self.cursor_pos.1 += self.tab_width;
        
        // Ensure cursor doesn't go beyond line boundaries
        self.clamp_cursor_to_line();
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

                self.move_cursor_left();
                self.modified = true;
            }
        } else if self.cursor_pos.0 > 0 {
            // Join with previous line
            let pos = self.line_col_to_char_idx(self.cursor_pos.0, 0);
            if pos > 0 {
                self.save_undo_state();
                self.rope.remove(pos - 1..pos);

                // Invalidate highlighting cache from previous line (since we're joining)
                self.highlighter
                    .invalidate_cache_from_line(self.cursor_pos.0 - 1);

                self.cursor_pos.0 -= 1;
                if let Some(line) = self.rope.line(self.cursor_pos.0).as_str() {
                    self.cursor_pos.1 = line.trim_end_matches('\n').width();
                }
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

        self.cursor_pos.0 += 1;
        self.cursor_pos.1 = 0;
        self.modified = true;
    }

    fn move_cursor_up(&mut self) {
        if self.cursor_pos.0 > 0 {
            self.cursor_pos.0 -= 1;
            self.clamp_cursor_to_line();
        }
    }

    fn move_cursor_down(&mut self) {
        if self.cursor_pos.0 < self.rope.len_lines().saturating_sub(1) {
            self.cursor_pos.0 += 1;
            self.clamp_cursor_to_line();
        }
    }

    fn move_cursor_left(&mut self) {
        if self.cursor_pos.1 > 0 {
            self.cursor_pos.1 -= 1;
        } else if self.cursor_pos.0 > 0 {
            self.cursor_pos.0 -= 1;
            if let Some(line) = self.rope.line(self.cursor_pos.0).as_str() {
                self.cursor_pos.1 = line.trim_end_matches('\n').width();
            }
        }
    }

    fn move_cursor_right(&mut self) {
        if let Some(line) = self.rope.line(self.cursor_pos.0).as_str() {
            let line_len = line.trim_end_matches('\n').width();
            if self.cursor_pos.1 < line_len {
                self.cursor_pos.1 += 1;
            } else if self.cursor_pos.0 < self.rope.len_lines().saturating_sub(1) {
                self.cursor_pos.0 += 1;
                self.cursor_pos.1 = 0;
            }
        }
    }

    fn page_up(&mut self) {
        let terminal_height: usize = 24; // Approximate terminal height, could be made dynamic
        let page_size = terminal_height.saturating_sub(4); // Leave room for status/help bars
        self.cursor_pos.0 = self.cursor_pos.0.saturating_sub(page_size);
        self.clamp_cursor_to_line();
    }

    fn page_down(&mut self) {
        let terminal_height: usize = 24; // Approximate terminal height, could be made dynamic  
        let page_size = terminal_height.saturating_sub(4); // Leave room for status/help bars
        let max_line = self.rope.len_lines().saturating_sub(1);
        self.cursor_pos.0 = (self.cursor_pos.0 + page_size).min(max_line);
        self.clamp_cursor_to_line();
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

    fn update_viewport(&mut self, terminal_height: usize) {
        // Vertical scrolling
        if self.cursor_pos.0 < self.viewport_offset.0 {
            self.viewport_offset.0 = self.cursor_pos.0;
        } else if self.cursor_pos.0 >= self.viewport_offset.0 + terminal_height - 3 {
            self.viewport_offset.0 = self.cursor_pos.0.saturating_sub(terminal_height - 4);
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

                }
            }
            MouseEventKind::Drag(_) => {
                // Mouse drag handling can be added here if needed
            }
            MouseEventKind::ScrollDown => {
                if self.viewport_offset.0 < self.rope.len_lines().saturating_sub(terminal_height) {
                    self.viewport_offset.0 += 3;
                }
            }
            MouseEventKind::ScrollUp => {
                self.viewport_offset.0 = self.viewport_offset.0.saturating_sub(3);
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
    }

    fn set_temporary_status_message(&mut self, message: String) {
        self.status_message = message;
        self.status_message_time = Some(Instant::now());
    }

    fn set_persistent_status_message(&mut self, message: String) {
        self.status_message = message;
        self.status_message_time = None;
    }

    fn check_status_message_timeout(&mut self) {
        if let Some(time) = self.status_message_time {
            if time.elapsed() >= self.status_message_timeout {
                self.status_message.clear();
                self.status_message_time = None;
            }
        }
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
                    self.tab_width = config.tab_width;
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
        if self.undo_stack.len() > 100 {
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
            self.set_temporary_status_message("Redo".to_string());
        }
    }

    fn start_find(&mut self) {
        self.input_mode = InputMode::Find;
        self.search_buffer.clear();
        self.status_message = "Find: ".to_string();
    }

    fn start_replace(&mut self) {
        self.input_mode = InputMode::Replace;
        self.search_buffer.clear();
        self.replace_buffer.clear();
        self.status_message = "Find: ".to_string();
    }

    fn start_goto_line(&mut self) {
        self.input_mode = InputMode::GoToLine;
        self.search_buffer.clear();
        self.status_message = "Go to line: ".to_string();
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
            
            self.current_match_index = self.search_matches
                .iter()
                .position(|(line, col)| {
                    let match_char_idx = self.line_col_to_char_idx(*line, *col);
                    match_char_idx >= cursor_char_idx
                })
                .or(Some(0)); // If no match at/after cursor, wrap to first match
            
            if let Some(index) = self.current_match_index {
                let (line, col) = self.search_matches[index];
                self.cursor_pos = (line, col);
            }
            
            true
        } else {
            self.current_match_index = None;
            false
        }
    }

    fn find_all_matches(&self, search_term: &str) -> Vec<(usize, usize)> {
        let mut matches = Vec::new();
        let text = self.rope.to_string();
        let mut start = 0;
        
        while let Some(pos) = text[start..].find(search_term) {
            let absolute_pos = start + pos;
            let line = self.rope.char_to_line(absolute_pos);
            let line_start = self.rope.line_to_char(line);
            let col = absolute_pos - line_start;
            matches.push((line, col));
            start = absolute_pos + 1;
        }
        
        matches
    }

    fn find_next_match(&mut self) -> bool {
        if self.search_matches.is_empty() {
            return false;
        }
        
        if let Some(current_index) = self.current_match_index {
            let next_index = (current_index + 1) % self.search_matches.len();
            self.current_match_index = Some(next_index);
            let (line, col) = self.search_matches[next_index];
            self.cursor_pos = (line, col);
            true
        } else {
            false
        }
    }

    fn find_previous_match(&mut self) -> bool {
        if self.search_matches.is_empty() {
            return false;
        }
        
        if let Some(current_index) = self.current_match_index {
            let prev_index = if current_index == 0 {
                self.search_matches.len() - 1
            } else {
                current_index - 1
            };
            self.current_match_index = Some(prev_index);
            let (line, col) = self.search_matches[prev_index];
            self.cursor_pos = (line, col);
            true
        } else {
            false
        }
    }

    fn cancel_search(&mut self) {
        self.cursor_pos = self.search_start_pos;
        self.search_matches.clear();
        self.current_match_index = None;
    }


    fn char_idx_to_line_col(&self, char_idx: usize) -> (usize, usize) {
        let line = self.rope.char_to_line(char_idx);
        let line_start = self.rope.line_to_char(line);
        let col = char_idx - line_start;
        (line, col)
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
        }
        
        replacements
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
        terminal.draw(|f| draw_ui(f, editor))?;
        
        // Check if status message should timeout
        editor.check_status_message_timeout();

        if event::poll(Duration::from_millis(100))? {
            match event::read()? {
                Event::Key(key) => {
                    if handle_key_event(editor, key)? {
                        break;
                    }
                }
                Event::Mouse(mouse) => {
                    if editor.mouse_enabled {
                        editor.handle_mouse_event(mouse, terminal.size()?.height as usize);
                    }
                }
                _ => {}
            }
        }

        editor.update_viewport(terminal.size()?.height as usize);
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
                return Ok(false);
            }
            KeyCode::Char('w') | KeyCode::Char('W') => {
                editor.word_wrap = !editor.word_wrap;
                editor.set_temporary_status_message(
                    format!("Word wrap: {}", if editor.word_wrap { "ON" } else { "OFF" }));
                editor.input_mode = InputMode::Normal;
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
                return Ok(false);
            }
            KeyCode::Esc => {
                editor.save_config();
                editor.input_mode = InputMode::Normal;
                editor.status_message.clear();
                return Ok(false);
            }
            _ if key.modifiers == KeyModifiers::CONTROL && key.code == KeyCode::Char('c') => {
                editor.save_config();
                editor.input_mode = InputMode::Normal;
                editor.status_message.clear();
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
            }
            KeyCode::Char(c) => {
                editor.filename_buffer.push(c);
                editor.status_message = format!("File Name to Write: {}", editor.filename_buffer);
            }
            _ => {}
        }
        return Ok(false);
    }

    // Handle find mode
    if editor.input_mode == InputMode::Find {
        match key.code {
            KeyCode::Enter => {
                if !editor.search_matches.is_empty() {
                    // If we already have matches, exit find mode and stay at current position
                    editor.input_mode = InputMode::Normal;
                    editor.search_matches.clear();
                    editor.current_match_index = None;
                    editor.set_temporary_status_message("Search completed".to_string());
                } else {
                    // First time pressing enter - perform search
                    let search_term = editor.search_buffer.clone();
                    if editor.perform_find(&search_term) {
                        let matches_count = editor.search_matches.len();
                        let current = editor.current_match_index.map(|i| i + 1).unwrap_or(1);
                        editor.status_message = format!("Find: {} ({}/{} matches) - Use ↑↓ to navigate, Enter/Esc to exit", 
                            search_term, current, matches_count);
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
            KeyCode::Up => {
                if editor.search_matches.is_empty() {
                    // If no current search, just move cursor
                    editor.move_cursor_up();
                } else {
                    // Navigate to previous match
                    editor.find_previous_match();
                    let matches_count = editor.search_matches.len();
                    let current = editor.current_match_index.map(|i| i + 1).unwrap_or(1);
                    editor.status_message = format!("Find: {} ({}/{} matches) - Use ↑↓ to navigate, Enter/Esc to exit", 
                        editor.search_buffer, current, matches_count);
                }
            }
            KeyCode::Down => {
                if editor.search_matches.is_empty() {
                    // If no current search, just move cursor
                    editor.move_cursor_down();
                } else {
                    // Navigate to next match
                    editor.find_next_match();
                    let matches_count = editor.search_matches.len();
                    let current = editor.current_match_index.map(|i| i + 1).unwrap_or(1);
                    editor.status_message = format!("Find: {} ({}/{} matches) - Use ↑↓ to navigate, Enter/Esc to exit", 
                        editor.search_buffer, current, matches_count);
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
                        editor.status_message = format!("Find: {} ({}/{} matches) - Use ↑↓ to navigate, Enter/Esc to exit", 
                            search_term, current, matches_count);
                    } else {
                        editor.status_message = format!("Find: {} (no matches)", editor.search_buffer);
                    }
                } else {
                    editor.status_message = "Find: ".to_string();
                    editor.search_matches.clear();
                    editor.current_match_index = None;
                }
            }
            KeyCode::Char(c) => {
                editor.search_buffer.push(c);
                // Re-search with updated term
                let search_term = editor.search_buffer.clone();
                if editor.perform_find(&search_term) {
                    let matches_count = editor.search_matches.len();
                    let current = editor.current_match_index.map(|i| i + 1).unwrap_or(1);
                    editor.status_message = format!("Find: {} ({}/{} matches) - Use ↑↓ to navigate, Enter/Esc to exit", 
                        search_term, current, matches_count);
                } else {
                    editor.status_message = format!("Find: {} (no matches)", editor.search_buffer);
                }
            }
            _ => {}
        }
        return Ok(false);
    }

    // Handle replace mode
    if editor.input_mode == InputMode::Replace {
        match key.code {
            KeyCode::Enter => {
                if editor.status_message.starts_with("Find:") {
                    // Switch to replace input
                    editor.status_message = format!("Replace '{}' with: ", editor.search_buffer);
                } else {
                    // Perform replace
                    let replacements = editor.perform_replace(&editor.search_buffer.clone(), &editor.replace_buffer.clone());
                    if replacements > 0 {
                        editor.set_temporary_status_message(format!("Replaced {} occurrence(s)", replacements));
                    } else {
                        editor.set_temporary_status_message("No matches found".to_string());
                    }
                    editor.input_mode = InputMode::Normal;
                }
            }
            KeyCode::Esc => {
                editor.input_mode = InputMode::Normal;
                editor.status_message = "Replace cancelled".to_string();
            }
            KeyCode::Backspace => {
                if editor.status_message.starts_with("Find:") {
                    editor.search_buffer.pop();
                    editor.status_message = format!("Find: {}", editor.search_buffer);
                } else {
                    editor.replace_buffer.pop();
                    editor.status_message = format!("Replace '{}' with: {}", editor.search_buffer, editor.replace_buffer);
                }
            }
            KeyCode::Char(c) => {
                if editor.status_message.starts_with("Find:") {
                    editor.search_buffer.push(c);
                    editor.status_message = format!("Find: {}", editor.search_buffer);
                } else {
                    editor.replace_buffer.push(c);
                    editor.status_message = format!("Replace '{}' with: {}", editor.search_buffer, editor.replace_buffer);
                }
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
            }
            KeyCode::Backspace => {
                editor.search_buffer.pop();
                editor.status_message = format!("Go to line: {}", editor.search_buffer);
            }
            KeyCode::Char(c) if c.is_ascii_digit() => {
                editor.search_buffer.push(c);
                editor.status_message = format!("Go to line: {}", editor.search_buffer);
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
        (KeyModifiers::CONTROL, KeyCode::Char('z')) => {
            editor.undo();
        }
        (KeyModifiers::CONTROL, KeyCode::Char('y')) => {
            editor.page_up();
        }
        (KeyModifiers::CONTROL, KeyCode::Char('f')) => {
            editor.start_find();
        }
        (KeyModifiers::CONTROL, KeyCode::Char('h')) => {
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
        (_, KeyCode::Tab) => editor.insert_tab(),
        (_, KeyCode::Enter) => editor.insert_newline(),
        (_, KeyCode::Backspace) => editor.delete_char(),
        (_, KeyCode::Esc) => {}, // Esc key - reserved for future use

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
        return syntax_spans.iter().map(|(style, text)| {
            let clean_text = text.trim_end_matches('\n').to_string();
            Span::styled(clean_text, *style)
        }).collect();
    }

    // Find matches on this line
    let line_matches: Vec<usize> = search_matches
        .iter()
        .enumerate()
        .filter_map(|(_match_idx, (match_line, match_col))| {
            if *match_line == line_idx {
                Some(*match_col)
            } else {
                None
            }
        })
        .collect();

    if line_matches.is_empty() {
        // No matches on this line - just apply syntax highlighting
        return syntax_spans.iter().map(|(style, text)| {
            let clean_text = text.trim_end_matches('\n').to_string();
            Span::styled(clean_text, *style)
        }).collect();
    }

    // Rebuild the line with search highlighting
    let mut result_spans = Vec::new();
    let mut current_pos = 0;
    
    // Find which match is currently selected on this line
    let current_match_col = current_match_index
        .and_then(|idx| search_matches.get(idx))
        .filter(|(match_line, _)| *match_line == line_idx)
        .map(|(_, match_col)| *match_col);

    // Process each character position, applying both syntax and search highlighting
    for &match_col in &line_matches {
        // Add text before the match
        if match_col > current_pos {
            let before_text = &line_content[current_pos..match_col];
            if !before_text.is_empty() {
                // Apply syntax highlighting to the text before the match
                result_spans.push(Span::styled(
                    before_text.to_string(),
                    get_syntax_style_at_position(syntax_spans, current_pos)
                ));
            }
        }

        // Add the highlighted match
        let match_end = (match_col + search_term.len()).min(line_content.len());
        let match_text = &line_content[match_col..match_end];
        
        let highlight_style = if Some(match_col) == current_match_col {
            // Current match - bright red/orange background
            Style::default().bg(Color::Red).fg(Color::White)
        } else {
            // Other matches - yellow background
            Style::default().bg(Color::Yellow).fg(Color::Black)
        };
        
        result_spans.push(Span::styled(match_text.to_string(), highlight_style));
        current_pos = match_end;
    }

    // Add remaining text after the last match
    if current_pos < line_content.len() {
        let remaining_text = &line_content[current_pos..];
        result_spans.push(Span::styled(
            remaining_text.to_string(),
            get_syntax_style_at_position(syntax_spans, current_pos)
        ));
    }

    result_spans
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

    // Main editor area
    let editor_area = Rect {
        x: area.x,
        y: area.y,
        width: area.width,
        height: area.height.saturating_sub(2),
    };

    // Status bar area
    let status_area = Rect {
        x: area.x,
        y: area.y + area.height.saturating_sub(2),
        width: area.width,
        height: 1,
    };

    // Help bar area
    let help_area = Rect {
        x: area.x,
        y: area.y + area.height.saturating_sub(1),
        width: area.width,
        height: 1,
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
                styled_spans.extend(
                    apply_search_highlighting(
                        &highlighted_spans,
                        line_content,
                        line_idx,
                        &editor.search_buffer,
                        &editor.search_matches,
                        editor.current_match_index,
                    )
                );

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

    let editor_widget = Paragraph::new(lines)
        .block(Block::default().borders(Borders::NONE))
        .wrap(Wrap { trim: false });

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
        format!(
            "{} {} | Ln {}, Col {} | Mouse: {}",
            filename,
            modified_indicator,
            editor.cursor_pos.0 + 1,
            editor.cursor_pos.1 + 1,
            if editor.mouse_enabled { "ON" } else { "OFF" }
        )
    };

    let status_widget =
        Paragraph::new(status_text).style(Style::default().bg(Color::DarkGray).fg(Color::White));

    f.render_widget(status_widget, status_area);

    // Draw help bar
    let help_text = match editor.input_mode {
        InputMode::ConfirmQuit => "Y: Save and quit | N: Quit without saving | ^C/Esc: Cancel",
        InputMode::EnteringFilename | InputMode::EnteringSaveAs => {
            "Enter: Confirm | Esc: Cancel | Type filename"
        }
        InputMode::OptionsMenu => "M: Mouse | L: Line Numbers | W: Word Wrap | T: Tab Width | Esc: Back",
        InputMode::Find => "Enter: Search/Exit | Esc: Cancel | ↑↓: Navigate matches | Type search term",
        InputMode::Replace => "Enter: Next step | Esc: Cancel | Type find/replace text",
        InputMode::GoToLine => "Enter: Go | Esc: Cancel | Type line number",
        _ => "^Q/^X Quit | ^S Save | ^F Find | ^H Replace | ^G Go to Line | ^Z Undo | ^R Redo | ^V Page Down | ^Y Page Up | ^O Options",
    };
    let help_widget =
        Paragraph::new(help_text).style(Style::default().bg(Color::Cyan).fg(Color::Black));

    f.render_widget(help_widget, help_area);
}
