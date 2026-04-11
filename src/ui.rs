use ratatui::{
    prelude::*,
    text::{Line, Span},
    widgets::{Block, Borders, Clear, Paragraph},
};
use std::rc::Rc;

use crate::constants::HELP_MODAL_WIDTH;
use crate::editor::{Editor, InputMode};
use crate::search::validate_match_at_position;

pub fn draw_ui(f: &mut Frame, editor: &mut Editor) {
    let area = f.area();

    let (help_left, help_right) = match editor.input_mode {
        InputMode::ConfirmQuit => (
            "Y: Save and quit  N: Quit without saving  ^C/Esc: Cancel".to_string(),
            String::new(),
        ),
        InputMode::EnteringFilename | InputMode::EnteringSaveAs => (
            "Enter: Confirm  Esc: Cancel  Type filename".to_string(),
            String::new(),
        ),
        InputMode::OptionsMenu => (
            "M: Mouse  L: Line Numbers  W: Word Wrap  T: Tab Width  Esc: Back".to_string(),
            String::new(),
        ),
        InputMode::Find => (
            "Enter: Search/Exit  Esc/^C: Cancel  Arrows: Navigate  ^R: Replace  ^O: Options"
                .to_string(),
            String::new(),
        ),
        InputMode::FindOptionsMenu => (
            "C: Case sensitivity  R: Regex mode  Esc: Back to find".to_string(),
            String::new(),
        ),
        InputMode::Replace => (
            "Enter: Next step  Esc/^C: Cancel  ^O: Options".to_string(),
            String::new(),
        ),
        InputMode::ReplaceConfirm => (
            "Y: Replace This  N: Skip  A: Replace All  ^C: Cancel".to_string(),
            String::new(),
        ),
        InputMode::GoToLine => (
            "Enter: Go  Esc/^C: Cancel  Type line number".to_string(),
            String::new(),
        ),
        InputMode::Help => (
            "^H Help".to_string(),
            format!("Rune v{}", env!("CARGO_PKG_VERSION")),
        ),
        InputMode::HexView => (
            "Arrows: Navigate  PgUp/PgDn: Page  ^B/Esc: Exit".to_string(),
            String::new(),
        ),
        _ => (
            "^H Help".to_string(),
            format!("Rune v{}", env!("CARGO_PKG_VERSION")),
        ),
    };
    let help_height = 1u16;

    let editor_area = Rect {
        x: area.x,
        y: area.y,
        width: area.width,
        height: area.height.saturating_sub(1 + help_height),
    };

    let status_area = Rect {
        x: area.x,
        y: area.y + area.height.saturating_sub(1 + help_height),
        width: area.width,
        height: 1,
    };

    let help_area = Rect {
        x: area.x,
        y: area.y + area.height.saturating_sub(help_height),
        width: area.width,
        height: help_height,
    };

    // Update viewport using the actual rendered editor area height —
    // this ensures the viewport calculation matches the rendering exactly.
    editor.update_viewport_for_height(editor_area.height as usize);

    if editor.input_mode == InputMode::HexView {
        if let Some(state) = &mut editor.hex_state {
            crate::hex::draw_hex_view(f, editor_area, state);
        }
    } else {
        let line_num_width = if editor.config.show_line_numbers {
            editor.rope.len_lines().to_string().len() + 1
        } else {
            0
        };

        let mut lines = vec![];
        let visible_lines = editor_area.height as usize;

        for i in 0..visible_lines {
            let line_idx = editor.viewport.viewport_offset.0 + i;
            if line_idx < editor.rope.len_lines() {
                // Get line text — use as_str() for zero-copy when possible,
                // fall back to collecting chars when the line spans chunk boundaries.
                let rope_line = editor.rope.line(line_idx);
                let owned_line: String;
                let line_text = match rope_line.as_str() {
                    Some(s) => s,
                    None => {
                        owned_line = rope_line.chars().collect::<String>();
                        &owned_line
                    }
                };

                let highlighted_spans: Rc<Vec<(Style, String)>> = editor.highlighter.highlight_line(line_idx, line_text);

                let mut styled_spans: Vec<Span> = vec![];

                if editor.config.show_line_numbers {
                    let line_num = format!("{:width$} ", line_idx + 1, width = line_num_width - 1);
                    styled_spans.push(Span::styled(line_num, Style::default().fg(Color::DarkGray)));
                }

                let line_content = line_text.trim_end_matches('\n');

                styled_spans.extend(apply_search_highlighting(
                    &highlighted_spans,
                    line_content,
                    line_idx,
                    &editor.search.search_buffer,
                    &editor.search.search_matches,
                    editor.search.current_match_index,
                    editor.search.case_sensitive,
                ));

                lines.push(Line::from(styled_spans));
            } else {
                let mut styled_spans: Vec<Span> = vec![];

                if editor.config.show_line_numbers {
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
        let cursor_screen_y = editor
            .viewport
            .cursor_pos
            .0
            .saturating_sub(editor.viewport.viewport_offset.0);
        if cursor_screen_y < visible_lines {
            let cursor_x = if editor.config.show_line_numbers {
                editor.viewport.cursor_pos.1 as u16 + line_num_width as u16
            } else {
                editor.viewport.cursor_pos.1 as u16
            };
            f.set_cursor_position(Position::new(cursor_x, cursor_screen_y as u16));
        }
    }

    // Draw status bar
    let status_text = if !editor.status_message.is_empty() {
        editor.status_message.clone()
    } else if editor.input_mode == InputMode::HexView {
        if let Some(state) = &editor.hex_state {
            let filename = editor
                .file_path
                .as_ref()
                .map(|p| p.display().to_string())
                .unwrap_or_else(|| "[No Name]".to_string());
            format!(
                "{} | HEX VIEW | Offset: 0x{:08X} ({}/{} bytes)",
                filename,
                state.cursor,
                state.cursor + 1,
                state.raw_bytes.len()
            )
        } else {
            String::new()
        }
    } else {
        let modified_indicator = if editor.modified { "[+]" } else { "" };
        let filename = editor
            .file_path
            .as_ref()
            .map(|p| p.display().to_string())
            .unwrap_or_else(|| "[No Name]".to_string());
        let search_modes = if editor.input_mode == InputMode::Find {
            format!(
                " | Search: {} {}",
                if editor.search.use_regex {
                    "Regex"
                } else {
                    "Literal"
                },
                if editor.search.case_sensitive {
                    "(Case)"
                } else {
                    "(NoCase)"
                }
            )
        } else {
            String::new()
        };

        format!(
            "{} {} | Ln {}, Col {} | Mouse: {}{}",
            filename,
            modified_indicator,
            editor.viewport.cursor_pos.0 + 1,
            editor.viewport.cursor_pos.1 + 1,
            if editor.config.mouse_enabled {
                "ON"
            } else {
                "OFF"
            },
            search_modes
        )
    };

    let status_widget =
        Paragraph::new(status_text).style(Style::default().bg(Color::DarkGray).fg(Color::White));
    f.render_widget(status_widget, status_area);

    // Draw help bar
    let help_line = if help_right.is_empty() {
        Line::from(Span::raw(&help_left))
    } else {
        let remaining_space = (help_area.width as usize)
            .saturating_sub(help_left.len())
            .saturating_sub(help_right.len());
        let spaces = " ".repeat(remaining_space.max(1));
        Line::from(vec![
            Span::raw(&help_left),
            Span::raw(spaces),
            Span::raw(&help_right),
        ])
    };

    let help_widget =
        Paragraph::new(help_line).style(Style::default().bg(Color::Cyan).fg(Color::Black));
    f.render_widget(help_widget, help_area);

    // Draw help modal if in help mode
    if editor.input_mode == InputMode::Help {
        draw_help_modal(f, area);
    }
}

fn draw_help_modal(f: &mut Frame, area: Rect) {
    let modal_width = HELP_MODAL_WIDTH;
    let modal_height = (area.height as f32 * 0.8) as u16;
    let modal_x = (area.width - modal_width) / 2;
    let modal_y = (area.height - modal_height) / 2;

    let modal_area = Rect {
        x: modal_x,
        y: modal_y,
        width: modal_width,
        height: modal_height,
    };

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
                  VIEW
─────────────────────────────────────────
^B       Hex view (live buffer)

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

    let clear = Clear;
    f.render_widget(clear, modal_area);

    let help_block = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(Color::Cyan))
        .style(Style::default().bg(Color::Black).fg(Color::White));

    let help_paragraph = Paragraph::new(help_content)
        .block(help_block)
        .alignment(ratatui::layout::Alignment::Center);

    f.render_widget(help_paragraph, modal_area);
}

fn apply_search_highlighting(
    syntax_spans: &[(Style, String)],
    line_content: &str,
    line_idx: usize,
    search_term: &str,
    search_matches: &[(usize, usize)],
    current_match_index: Option<usize>,
    case_sensitive: bool,
) -> Vec<Span<'static>> {
    if search_term.is_empty() || search_matches.is_empty() {
        return syntax_spans
            .iter()
            .map(|(style, text)| {
                let clean_text = text.trim_end_matches('\n').to_string();
                Span::styled(clean_text, *style)
            })
            .collect();
    }

    let mut validated_matches: Vec<usize> = Vec::new();
    for (match_line, match_col) in search_matches {
        if *match_line == line_idx
            && validate_match_at_position(line_content, *match_col, search_term, case_sensitive)
        {
            validated_matches.push(*match_col);
        }
    }

    if validated_matches.is_empty() {
        return syntax_spans
            .iter()
            .map(|(style, text)| {
                let clean_text = text.trim_end_matches('\n').to_string();
                Span::styled(clean_text, *style)
            })
            .collect();
    }

    validated_matches.sort_unstable();

    let mut result_spans = Vec::new();
    let line_chars: Vec<char> = line_content.chars().collect();
    let search_chars: Vec<char> = search_term.chars().collect();
    let mut current_char_pos = 0;

    let current_match_col = current_match_index
        .and_then(|idx| search_matches.get(idx))
        .filter(|(match_line, _)| *match_line == line_idx)
        .map(|(_, match_col)| *match_col);

    for &match_char_pos in &validated_matches {
        if match_char_pos > current_char_pos {
            let before_chars: String = line_chars[current_char_pos..match_char_pos]
                .iter()
                .collect();
            if !before_chars.is_empty() {
                result_spans.push(Span::styled(
                    before_chars,
                    get_syntax_style_at_position(syntax_spans, current_char_pos),
                ));
            }
        }

        let match_end_char = (match_char_pos + search_chars.len()).min(line_chars.len());
        let match_chars: String = line_chars[match_char_pos..match_end_char].iter().collect();

        let highlight_style = if Some(match_char_pos) == current_match_col {
            Style::default().bg(Color::Red).fg(Color::White)
        } else {
            Style::default().bg(Color::Yellow).fg(Color::Black)
        };

        result_spans.push(Span::styled(match_chars, highlight_style));
        current_char_pos = match_end_char;
    }

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
