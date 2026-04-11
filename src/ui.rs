use ratatui::{
    prelude::*,
    text::{Line, Span},
    widgets::{Block, Borders, Paragraph},
};
use std::rc::Rc;

use crate::editor::{Editor, InputMode};
use crate::search::validate_match_at_position;
use crate::tabs::TabManager;

pub fn draw_ui(f: &mut Frame, tabs: &mut TabManager) {
    let area = f.area();

    let (help_left, help_right) = match tabs.input_mode {
        InputMode::ConfirmQuit => (
            "Y: Save and quit  N: Quit without saving  ^C/Esc: Cancel".to_string(),
            String::new(),
        ),
        InputMode::ConfirmCloseTab => (
            "Y: Save and close  N: Close without saving  ^C/Esc: Cancel".to_string(),
            String::new(),
        ),
        InputMode::EnteringFilename | InputMode::EnteringSaveAs => (
            "Enter: Confirm  Esc: Cancel  Type filename".to_string(),
            String::new(),
        ),
        InputMode::OpenFileCurrentTab | InputMode::OpenFileNewTab => (
            "Enter: Open  Esc: Cancel  Type file path".to_string(),
            String::new(),
        ),
        InputMode::OptionsMenu => (
            "M: Mouse  L: Line Numbers  W: Word Wrap  T: Tab Width  I: Auto-indent  P: Whitespace  O: Open File  N: New Tab File  Esc: Back".to_string(),
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
            "^H Help  ^T New Tab  ^P Finder".to_string(),
            format!("Rune v{}", env!("CARGO_PKG_VERSION")),
        ),
    };
    let help_height = 1u16;
    let tab_bar_height = 1u16;

    let tab_bar_area = Rect {
        x: area.x,
        y: area.y,
        width: area.width,
        height: tab_bar_height,
    };

    let editor_area = Rect {
        x: area.x,
        y: area.y + tab_bar_height,
        width: area.width,
        height: area.height.saturating_sub(1 + help_height + tab_bar_height),
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

    // Draw tab bar
    draw_tab_bar(f, tabs, tab_bar_area);

    let show_line_numbers = tabs.config.show_line_numbers;
    let word_wrap = tabs.config.word_wrap;
    let input_mode = tabs.input_mode.clone();

    let line_num_width = if show_line_numbers {
        tabs.active_editor().rope.len_lines().to_string().len() + 1
    } else {
        0
    };

    // Update viewport using the actual rendered editor area dimensions
    tabs.active_editor_mut().update_viewport_for_size(
        editor_area.height as usize,
        editor_area.width as usize,
        line_num_width,
        word_wrap,
    );

    if input_mode == InputMode::Help {
        draw_help_fullscreen(f, tabs, editor_area);
    } else if input_mode == InputMode::HexView {
        if let Some(state) = &mut tabs.active_editor_mut().hex_state {
            crate::hex::draw_hex_view(f, editor_area, state);
        }
    } else if word_wrap {
        draw_editor_word_wrap(f, tabs, editor_area, line_num_width);
    } else {
        draw_editor_horizontal_scroll(f, tabs, editor_area, line_num_width);
    }

    // Draw status bar
    let status_text = if !tabs.status_message.is_empty() {
        if tabs.config.constant_cursor_position {
            let editor = tabs.active_editor();
            format!(
                "{} | Ln {}, Col {}",
                tabs.status_message,
                editor.viewport.cursor_pos.0 + 1,
                editor.viewport.cursor_pos.1 + 1
            )
        } else {
            tabs.status_message.clone()
        }
    } else if tabs.input_mode == InputMode::HexView {
        let editor = tabs.active_editor();
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
        let editor = tabs.active_editor();
        let modified_indicator = if editor.modified { "[+]" } else { "" };
        let filename = editor
            .file_path
            .as_ref()
            .map(|p| p.display().to_string())
            .unwrap_or_else(|| "[No Name]".to_string());
        let search_modes = if tabs.input_mode == InputMode::Find {
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
            if tabs.config.mouse_enabled {
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
}

/// Render the tab bar at the top of the screen.
fn draw_tab_bar(f: &mut Frame, tabs: &TabManager, area: Rect) {
    let mut spans: Vec<Span> = Vec::new();
    let available_width = area.width as usize;
    let mut used_width = 0;

    for (i, tab) in tabs.tabs.iter().enumerate() {
        let modified = if tab.modified { "*" } else { "" };
        let title = format!(" {}{} ", tab.display_name, modified);
        let title_len = title.len();

        if used_width + title_len > available_width.saturating_sub(4) {
            let remaining = tabs.tabs.len() - i;
            spans.push(Span::styled(
                format!(" +{remaining} "),
                Style::default().fg(Color::DarkGray),
            ));
            break;
        }

        let style = if i == tabs.active_tab {
            Style::default().bg(Color::Cyan).fg(Color::Black)
        } else {
            Style::default().bg(Color::DarkGray).fg(Color::White)
        };
        spans.push(Span::styled(title, style));
        used_width += title_len;
    }

    let tab_line = Line::from(spans);
    let tab_widget = Paragraph::new(tab_line).style(Style::default().bg(Color::Black));
    f.render_widget(tab_widget, area);
}

/// Render editor content with horizontal scrolling (word_wrap OFF).
fn draw_editor_horizontal_scroll(
    f: &mut Frame,
    tabs: &mut TabManager,
    editor_area: Rect,
    line_num_width: usize,
) {
    let show_line_numbers = tabs.config.show_line_numbers;
    let show_whitespace = tabs.config.show_whitespace;
    let editor = tabs.active_editor_mut();

    let mut lines = vec![];
    let visible_lines = editor_area.height as usize;
    let content_width = (editor_area.width as usize).saturating_sub(line_num_width);
    let h_offset = editor.viewport.viewport_offset.1;

    for i in 0..visible_lines {
        let line_idx = editor.viewport.viewport_offset.0 + i;
        if line_idx < editor.rope.len_lines() {
            let rope_line = editor.rope.line(line_idx);
            let owned_line: String;
            let line_text = match rope_line.as_str() {
                Some(s) => s,
                None => {
                    owned_line = rope_line.chars().collect::<String>();
                    &owned_line
                }
            };

            let highlighted_spans: Rc<Vec<(Style, String)>> =
                editor.highlighter.highlight_line(line_idx, line_text);

            let mut styled_spans: Vec<Span> = vec![];

            if show_line_numbers {
                let line_num = format!("{:width$} ", line_idx + 1, width = line_num_width - 1);
                styled_spans.push(Span::styled(line_num, Style::default().fg(Color::DarkGray)));
            }

            let line_content = line_text.trim_end_matches('\n');

            let mut search_spans = apply_search_highlighting(
                &highlighted_spans,
                line_content,
                line_idx,
                &editor.search.search_buffer,
                &editor.search.search_matches,
                editor.search.current_match_index,
                editor.search.case_sensitive,
            );

            if show_whitespace {
                for span in &mut search_spans {
                    let rendered = render_whitespace(&span.content);
                    if rendered != span.content.as_ref() {
                        *span = Span::styled(rendered, span.style);
                    }
                }
            }

            let final_spans = apply_selection_highlighting(search_spans, line_idx, editor);

            // Apply horizontal scrolling: slice spans to show only the visible portion
            let sliced = slice_spans_horizontal(&final_spans, h_offset, content_width);
            styled_spans.extend(sliced);

            lines.push(Line::from(styled_spans));
        } else {
            let mut styled_spans: Vec<Span> = vec![];

            if show_line_numbers {
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

    // Draw cursor -- account for horizontal offset
    let cursor_screen_y = editor
        .viewport
        .cursor_pos
        .0
        .saturating_sub(editor.viewport.viewport_offset.0);
    if cursor_screen_y < visible_lines {
        let cursor_col_on_screen = editor.viewport.cursor_pos.1.saturating_sub(h_offset);
        let cursor_x = cursor_col_on_screen as u16 + line_num_width as u16;
        f.set_cursor_position(Position::new(cursor_x, cursor_screen_y as u16));
    }
}

/// Render editor content with word wrapping (word_wrap ON).
fn draw_editor_word_wrap(
    f: &mut Frame,
    tabs: &mut TabManager,
    editor_area: Rect,
    line_num_width: usize,
) {
    let show_line_numbers = tabs.config.show_line_numbers;
    let show_whitespace = tabs.config.show_whitespace;
    let editor = tabs.active_editor_mut();

    let visible_lines = editor_area.height as usize;
    let content_width = (editor_area.width as usize).saturating_sub(line_num_width);

    let mut lines: Vec<Line> = vec![];
    let mut screen_row = 0;
    let mut cursor_screen_y: Option<usize> = None;
    let mut line_idx = editor.viewport.viewport_offset.0;

    while screen_row < visible_lines && line_idx < editor.rope.len_lines() {
        let rope_line = editor.rope.line(line_idx);
        let owned_line: String;
        let line_text = match rope_line.as_str() {
            Some(s) => s,
            None => {
                owned_line = rope_line.chars().collect::<String>();
                &owned_line
            }
        };

        let highlighted_spans: Rc<Vec<(Style, String)>> =
            editor.highlighter.highlight_line(line_idx, line_text);

        let line_content = line_text.trim_end_matches('\n');

        let mut search_spans = apply_search_highlighting(
            &highlighted_spans,
            line_content,
            line_idx,
            &editor.search.search_buffer,
            &editor.search.search_matches,
            editor.search.current_match_index,
            editor.search.case_sensitive,
        );

        if show_whitespace {
            for span in &mut search_spans {
                let rendered = render_whitespace(&span.content);
                if rendered != span.content.as_ref() {
                    *span = Span::styled(rendered, span.style);
                }
            }
        }

        let final_spans = apply_selection_highlighting(search_spans, line_idx, editor);

        let line_width = line_content.len().max(
            final_spans
                .iter()
                .map(|s| s.content.len())
                .sum::<usize>(),
        );
        let rows_needed = if content_width == 0 || line_width == 0 {
            1
        } else {
            line_width.div_ceil(content_width)
        };

        if line_idx == editor.viewport.cursor_pos.0 {
            let cursor_sub_row = if content_width > 0 {
                editor.viewport.cursor_pos.1 / content_width
            } else {
                0
            };
            cursor_screen_y = Some(screen_row + cursor_sub_row);
        }

        let all_chars: Vec<(char, Style)> = collect_span_chars(&final_spans);

        for sub_row in 0..rows_needed {
            if screen_row >= visible_lines {
                break;
            }

            let mut styled_spans: Vec<Span> = vec![];

            if show_line_numbers {
                if sub_row == 0 {
                    let line_num =
                        format!("{:width$} ", line_idx + 1, width = line_num_width - 1);
                    styled_spans
                        .push(Span::styled(line_num, Style::default().fg(Color::DarkGray)));
                } else {
                    let empty_num = format!("{:width$} ", "", width = line_num_width - 1);
                    styled_spans
                        .push(Span::styled(empty_num, Style::default().fg(Color::DarkGray)));
                }
            }

            let start_col = sub_row * content_width;
            let end_col = (start_col + content_width).min(all_chars.len());

            if start_col < all_chars.len() {
                let sub_chars = &all_chars[start_col..end_col];
                styled_spans.extend(group_chars_into_spans(sub_chars));
            }

            lines.push(Line::from(styled_spans));
            screen_row += 1;
        }

        line_idx += 1;
    }

    // Fill remaining rows with tilde markers
    while screen_row < visible_lines {
        let mut styled_spans: Vec<Span> = vec![];

        if show_line_numbers {
            let empty_line_num = format!("{:width$} ", "", width = line_num_width - 1);
            styled_spans.push(Span::styled(
                empty_line_num,
                Style::default().fg(Color::DarkGray),
            ));
        }

        styled_spans.push(Span::styled("~", Style::default().fg(Color::DarkGray)));
        lines.push(Line::from(styled_spans));
        screen_row += 1;
    }

    let editor_widget = Paragraph::new(lines).block(Block::default().borders(Borders::NONE));
    f.render_widget(editor_widget, editor_area);

    // Draw cursor
    if let Some(screen_y) = cursor_screen_y {
        if screen_y < visible_lines {
            let cursor_col_in_row = if content_width > 0 {
                editor.viewport.cursor_pos.1 % content_width
            } else {
                editor.viewport.cursor_pos.1
            };
            let cursor_x = cursor_col_in_row as u16 + line_num_width as u16;
            f.set_cursor_position(Position::new(cursor_x, screen_y as u16));
        }
    }
}

/// Slice a list of spans to only include characters in the display column range
/// [h_offset, h_offset + width). This handles multi-char spans that straddle the boundary.
fn slice_spans_horizontal(spans: &[Span<'_>], h_offset: usize, width: usize) -> Vec<Span<'static>> {
    if h_offset == 0 && width == usize::MAX {
        return spans
            .iter()
            .map(|s| Span::styled(s.content.to_string(), s.style))
            .collect();
    }

    let mut result = Vec::new();
    let mut col = 0;
    let end = h_offset + width;

    for span in spans {
        let span_chars: Vec<char> = span.content.chars().collect();
        let span_len = span_chars.len();
        let span_end = col + span_len;

        if span_end <= h_offset || col >= end {
            col = span_end;
            continue;
        }

        let start_in_span = h_offset.saturating_sub(col);
        let end_in_span = if span_end > end { end - col } else { span_len };

        if start_in_span < end_in_span {
            let visible: String = span_chars[start_in_span..end_in_span].iter().collect();
            result.push(Span::styled(visible, span.style));
        }

        col = span_end;
    }

    result
}

/// Collect all characters with their styles from a list of spans.
fn collect_span_chars(spans: &[Span<'_>]) -> Vec<(char, Style)> {
    let mut chars = Vec::new();
    for span in spans {
        for ch in span.content.chars() {
            chars.push((ch, span.style));
        }
    }
    chars
}

/// Group consecutive (char, Style) pairs with the same style into Spans.
fn group_chars_into_spans(chars: &[(char, Style)]) -> Vec<Span<'static>> {
    let mut result = Vec::new();
    if chars.is_empty() {
        return result;
    }

    let mut current_text = String::new();
    let mut current_style = chars[0].1;
    current_text.push(chars[0].0);

    for &(ch, style) in &chars[1..] {
        if style == current_style {
            current_text.push(ch);
        } else {
            result.push(Span::styled(current_text.clone(), current_style));
            current_text.clear();
            current_text.push(ch);
            current_style = style;
        }
    }

    if !current_text.is_empty() {
        result.push(Span::styled(current_text, current_style));
    }

    result
}

fn help_lines() -> Vec<&'static str> {
    vec![
        "               FILE OPERATIONS",
        "\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}",
        "^Q / ^X  Quit editor",
        "^S       Save file",
        "^W       Save as (write file)",
        "^O       Options menu",
        "",
        "                 EDITING",
        "\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}",
        "^Z       Undo",
        "^R       Redo",
        "^K       Cut line/selection",
        "^U       Paste",
        "M-6      Copy line/selection",
        "M-A      Toggle mark (selection)",
        "M-}      Indent selection",
        "M-{      Unindent selection",
        "M-;      Toggle comment",
        "Delete   Delete forward",
        "M-\\      Word completion",
        "M-V      Verbatim input (raw char)",
        "^E       Execute command",
        "",
        "               NAVIGATION",
        "\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}",
        "^F       Find text",
        "^\\       Replace text",
        "^G       Go to line",
        "^C       Cursor position info",
        "^V       Page down",
        "^Y       Page up",
        "^Home    Start of file",
        "^End     End of file",
        "^Left    Previous word",
        "^Right   Next word",
        "M-]      Match bracket",
        "Arrows   Move cursor",
        "",
        "                  TABS",
        "\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}",
        "^T       New tab",
        "M-Left   Previous tab",
        "M-Right  Next tab",
        "M-W      Close tab",
        "^P       Fuzzy finder (switch tab)",
        "",
        "                  VIEW",
        "\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}",
        "^B       Hex view (live buffer)",
        "M-P      Toggle whitespace display",
        "",
        "                OPTIONS",
        "\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}",
        "^O       Open options menu",
        "  M      Toggle mouse mode",
        "  L      Toggle line numbers",
        "  W      Toggle word wrap",
        "  T      Set tab width",
        "  I      Toggle auto-indent",
        "  P      Toggle whitespace",
        "  O      Open file in current tab",
        "  N      Open file in new tab",
        "  B      Toggle backup on save",
        "",
        "Note: M- prefix means Alt/Meta key.",
        "      ^ prefix means Ctrl key.",
    ]
}

fn draw_help_fullscreen(f: &mut Frame, tabs: &mut TabManager, area: Rect) {
    let lines = help_lines();
    let visible_height = area.height as usize;
    let max_scroll = lines.len().saturating_sub(visible_height);
    if tabs.help_scroll > max_scroll {
        tabs.help_scroll = max_scroll;
    }

    let display_lines: Vec<Line> = lines
        .iter()
        .skip(tabs.help_scroll)
        .take(visible_height)
        .map(|l| Line::from(Span::raw(*l)))
        .collect();

    let help_widget = Paragraph::new(display_lines)
        .style(Style::default().fg(Color::White))
        .block(Block::default().borders(Borders::NONE));
    f.render_widget(help_widget, area);
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

fn render_whitespace(text: &str) -> String {
    text.replace(' ', "\u{00B7}").replace('\t', "\u{2192}")
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

fn apply_selection_highlighting(
    spans: Vec<Span<'static>>,
    line_idx: usize,
    editor: &Editor,
) -> Vec<Span<'static>> {
    let (anchor, cursor) = match editor.mark_anchor {
        Some(anchor) => (anchor, editor.viewport.cursor_pos),
        None => return spans,
    };

    let anchor_idx = editor.line_col_to_char_idx(anchor.0, anchor.1);
    let cursor_idx = editor.line_col_to_char_idx(cursor.0, cursor.1);
    let (sel_start, sel_end) = if anchor_idx <= cursor_idx {
        (anchor_idx, cursor_idx)
    } else {
        (cursor_idx, anchor_idx)
    };

    let line_start_char = editor.rope.line_to_char(line_idx);
    let line_end_char = if line_idx + 1 < editor.rope.len_lines() {
        editor.rope.line_to_char(line_idx + 1)
    } else {
        editor.rope.len_chars()
    };

    // Check if this line intersects the selection
    if sel_end <= line_start_char || sel_start >= line_end_char {
        return spans;
    }

    let sel_start_in_line = sel_start.saturating_sub(line_start_char);
    let sel_end_in_line = (sel_end - line_start_char).min(line_end_char - line_start_char);

    let mut result = Vec::new();
    let mut char_pos = 0;
    for span in spans {
        let span_len = span.content.chars().count();
        let span_end = char_pos + span_len;

        if span_end <= sel_start_in_line || char_pos >= sel_end_in_line {
            result.push(span);
        } else if char_pos >= sel_start_in_line && span_end <= sel_end_in_line {
            result.push(Span::styled(
                span.content.to_string(),
                span.style.bg(Color::White).fg(Color::Black),
            ));
        } else {
            let chars: Vec<char> = span.content.chars().collect();
            let mut i = 0;
            while i < chars.len() {
                let abs_pos = char_pos + i;
                let in_sel = abs_pos >= sel_start_in_line && abs_pos < sel_end_in_line;
                let start_i = i;
                while i < chars.len() {
                    let p = char_pos + i;
                    let p_in_sel = p >= sel_start_in_line && p < sel_end_in_line;
                    if p_in_sel != in_sel {
                        break;
                    }
                    i += 1;
                }
                let text: String = chars[start_i..i].iter().collect();
                let style = if in_sel {
                    span.style.bg(Color::White).fg(Color::Black)
                } else {
                    span.style
                };
                result.push(Span::styled(text, style));
            }
        }
        char_pos = span_end;
    }
    result
}
