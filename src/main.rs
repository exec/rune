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
use std::{
    fs,
    io::{self, stdout},
    path::PathBuf,
    time::Duration,
};
use unicode_width::{UnicodeWidthChar, UnicodeWidthStr};

mod syntax;
use syntax::SyntaxHighlighter;

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
    selection_start: Option<(usize, usize)>,
    selection_end: Option<(usize, usize)>,
    highlighter: SyntaxHighlighter,
    syntax_name: Option<String>,
    input_mode: InputMode,
    filename_buffer: String,
    quit_after_save: bool,
    mouse_enabled: bool,
}

#[derive(Debug, Clone, PartialEq)]
enum InputMode {
    Normal,
    EnteringFilename,
    EnteringSaveAs,
    ConfirmQuit,
}

impl Editor {
    fn new() -> Self {
        Self {
            rope: Rope::new(),
            cursor_pos: (0, 0),
            viewport_offset: (0, 0),
            file_path: None,
            modified: false,
            status_message: String::new(),
            selection_start: None,
            selection_end: None,
            highlighter: SyntaxHighlighter::new(),
            syntax_name: None,
            input_mode: InputMode::Normal,
            filename_buffer: String::new(),
            quit_after_save: false,
            mouse_enabled: true,
        }
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

                self.status_message = format!("Saved: {}", path.display());
            }
            Err(e) => {
                self.status_message = format!("Error saving file: {e}");
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
        let pos = self.line_col_to_char_idx(self.cursor_pos.0, self.cursor_pos.1);
        self.rope.insert_char(pos, c);

        // Invalidate highlighting cache from current line
        self.highlighter
            .invalidate_cache_from_line(self.cursor_pos.0);

        self.move_cursor_right();
        self.modified = true;
    }

    fn delete_char(&mut self) {
        if self.cursor_pos.1 > 0 {
            let pos = self.line_col_to_char_idx(self.cursor_pos.0, self.cursor_pos.1);
            if pos > 0 {
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

                    // Start selection
                    self.selection_start = Some(self.cursor_pos);
                    self.selection_end = None;
                }
            }
            MouseEventKind::Drag(_) => {
                if self.selection_start.is_some() {
                    let clicked_line = self.viewport_offset.0 + event.row as usize;
                    let clicked_col = event.column as usize;

                    if clicked_line < self.rope.len_lines() {
                        self.cursor_pos.0 = clicked_line;
                        self.cursor_pos.1 = clicked_col;
                        self.clamp_cursor_to_line();
                        self.selection_end = Some(self.cursor_pos);
                    }
                }
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

    fn start_selection(&mut self) {
        self.selection_start = Some(self.cursor_pos);
        self.selection_end = None;
        self.status_message = "-- VISUAL --".to_string();
    }

    fn update_selection(&mut self) {
        if self.selection_start.is_some() {
            self.selection_end = Some(self.cursor_pos);
        }
    }

    fn cancel_selection(&mut self) {
        self.selection_start = None;
        self.selection_end = None;
        self.status_message.clear();
    }

    fn toggle_mouse_mode(&mut self) {
        self.mouse_enabled = !self.mouse_enabled;
        self.status_message = if self.mouse_enabled {
            "Mouse mode enabled".to_string()
        } else {
            "Mouse mode disabled".to_string()
        };
    }

    #[allow(dead_code)]
    fn get_selected_text(&self) -> Option<String> {
        if let (Some(start), Some(end)) = (self.selection_start, self.selection_end) {
            let start_idx = self.line_col_to_char_idx(start.0, start.1);
            let end_idx = self.line_col_to_char_idx(end.0, end.1);

            let (start_idx, end_idx) = if start_idx <= end_idx {
                (start_idx, end_idx)
            } else {
                (end_idx, start_idx)
            };

            Some(self.rope.slice(start_idx..end_idx).to_string())
        } else {
            None
        }
    }
}

fn main() -> Result<()> {
    let cli = Cli::parse();

    enable_raw_mode()?;
    execute!(stdout(), EnterAlternateScreen)?;
    crossterm::execute!(stdout(), crossterm::event::EnableMouseCapture)?;

    let mut terminal = Terminal::new(CrosstermBackend::new(stdout()))?;
    terminal.clear()?;

    let mut editor = Editor::new();
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

    // Handle visual mode
    if editor.selection_start.is_some() {
        match key.code {
            KeyCode::Esc => {
                editor.cancel_selection();
                return Ok(false);
            }
            KeyCode::Up => {
                editor.move_cursor_up();
                editor.update_selection();
                return Ok(false);
            }
            KeyCode::Down => {
                editor.move_cursor_down();
                editor.update_selection();
                return Ok(false);
            }
            KeyCode::Left => {
                editor.move_cursor_left();
                editor.update_selection();
                return Ok(false);
            }
            KeyCode::Right => {
                editor.move_cursor_right();
                editor.update_selection();
                return Ok(false);
            }
            _ => {}
        }
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
            editor.start_selection();
        }
        (KeyModifiers::CONTROL, KeyCode::Char('m')) => {
            editor.toggle_mouse_mode();
        }

        // Navigation
        (_, KeyCode::Up) => editor.move_cursor_up(),
        (_, KeyCode::Down) => editor.move_cursor_down(),
        (_, KeyCode::Left) => editor.move_cursor_left(),
        (_, KeyCode::Right) => editor.move_cursor_right(),
        (_, KeyCode::Home) => editor.cursor_pos.1 = 0,
        (_, KeyCode::End) => {
            if let Some(line) = editor.rope.line(editor.cursor_pos.0).as_str() {
                editor.cursor_pos.1 = line.trim_end_matches('\n').width();
            }
        }

        // Editing
        (_, KeyCode::Char(c)) => editor.insert_char(c),
        (_, KeyCode::Enter) => editor.insert_newline(),
        (_, KeyCode::Backspace) => editor.delete_char(),
        (_, KeyCode::Esc) => editor.cancel_selection(),

        _ => {}
    }

    Ok(false)
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

    // Draw editor content with lazy syntax highlighting
    let mut lines = vec![];
    let visible_lines = editor_area.height as usize;

    for i in 0..visible_lines {
        let line_idx = editor.viewport_offset.0 + i;
        if line_idx < editor.rope.len_lines() {
            if let Some(line_text) = editor.rope.line(line_idx).as_str() {
                // Use lazy highlighting - only highlight visible lines
                let highlighted_spans = editor.highlighter.highlight_line(line_idx, line_text);

                let styled_spans: Vec<Span> = highlighted_spans
                    .into_iter()
                    .map(|(style, text)| {
                        let clean_text = text.trim_end_matches('\n').to_string();
                        Span::styled(clean_text, style)
                    })
                    .collect();
                lines.push(Line::from(styled_spans));
            }
        } else {
            lines.push(Line::from(Span::styled(
                "~",
                Style::default().fg(Color::DarkGray),
            )));
        }
    }

    let editor_widget = Paragraph::new(lines)
        .block(Block::default().borders(Borders::NONE))
        .wrap(Wrap { trim: false });

    f.render_widget(editor_widget, editor_area);

    // Draw cursor
    let cursor_screen_y = editor.cursor_pos.0.saturating_sub(editor.viewport_offset.0);
    if cursor_screen_y < visible_lines {
        f.set_cursor_position(Position::new(
            editor.cursor_pos.1 as u16,
            cursor_screen_y as u16,
        ));
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
        _ => "^Q/^X Quit | ^S Save | ^W Save As | ^V Visual | ^M Toggle Mouse",
    };
    let help_widget =
        Paragraph::new(help_text).style(Style::default().bg(Color::Cyan).fg(Color::Black));

    f.render_widget(help_widget, help_area);
}
