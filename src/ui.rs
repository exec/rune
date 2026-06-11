use ratatui::{
    prelude::*,
    text::{Line, Span},
    widgets::{Block, Borders, Paragraph},
};
use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;
use unicode_width::{UnicodeWidthChar, UnicodeWidthStr};

use crate::editor::{char_display_width, Editor, InputMode};
use crate::get_line_str;
use crate::tabs::TabManager;

/// Per-line cache of fully-assembled content spans (post-syntax, post-search,
/// pre-selection, pre-horizontal-slice). Only populated when search and
/// selection are both inactive — the common cursor-navigation case. The cache
/// is invalidated whenever `dirty_generation` or the active buffer identity
/// changes, so entries are always valid for the current frame if they exist.
struct RenderCache {
    buffer_id: Option<u64>,
    dirty_generation: u64,
    show_whitespace: bool,
    lines: HashMap<usize, Rc<Vec<Span<'static>>>>,
}

impl RenderCache {
    fn new() -> Self {
        Self {
            buffer_id: None,
            dirty_generation: 0,
            show_whitespace: false,
            lines: HashMap::new(),
        }
    }

    fn reset_if_stale(&mut self, buffer_id: u64, dirty_generation: u64, show_whitespace: bool) {
        if self.buffer_id != Some(buffer_id)
            || self.dirty_generation != dirty_generation
            || self.show_whitespace != show_whitespace
        {
            self.buffer_id = Some(buffer_id);
            self.dirty_generation = dirty_generation;
            self.show_whitespace = show_whitespace;
            self.lines.clear();
        }
    }
}

thread_local! {
    static RENDER_CACHE: RefCell<RenderCache> = RefCell::new(RenderCache::new());
}

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
        InputMode::HexView => (
            "Arrows: Navigate  PgUp/PgDn: Page  ^B/Esc: Exit".to_string(),
            String::new(),
        ),
        InputMode::VerbatimInput => (
            "Press any key to insert it literally".to_string(),
            String::new(),
        ),
        InputMode::ExecuteCommand => (
            "Enter: Execute  Esc/^C: Cancel  Type shell command".to_string(),
            String::new(),
        ),
        InputMode::ConfirmExecute => (
            "Y: Execute  N/Esc: Cancel".to_string(),
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

    if input_mode == InputMode::HexView {
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

        let view_mode = if tabs.read_only { " | VIEW MODE" } else { "" };
        format!(
            "{} {} | Ln {}, Col {} | Mouse: {}{}{}",
            filename,
            modified_indicator,
            editor.viewport.cursor_pos.0 + 1,
            editor.viewport.cursor_pos.1 + 1,
            if tabs.config.mouse_enabled {
                "ON"
            } else {
                "OFF"
            },
            search_modes,
            view_mode
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

    // Draw fuzzy finder overlay
    if input_mode == InputMode::FuzzyFinder {
        draw_fuzzy_finder(f, tabs, area);
    }
}

/// Render the tab bar at the top of the screen.
fn draw_tab_bar(f: &mut Frame, tabs: &mut TabManager, area: Rect) {
    let available_width = area.width as usize;
    let active = tabs.active_tab;
    let num_tabs = tabs.tabs.len();

    // Pre-compute tab title widths
    let tab_titles: Vec<String> = tabs
        .tabs
        .iter()
        .map(|tab| {
            let modified = if tab.modified { "*" } else { "" };
            format!(" {}{} ", tab.display_name, modified)
        })
        .collect();
    // Display width, not byte length -- multibyte tab names would otherwise
    // break the overflow math and the click hit-testing in tabs.rs (which
    // replays this layout and must agree with it).
    let tab_widths: Vec<usize> = tab_titles
        .iter()
        .map(|t| UnicodeWidthStr::width(t.as_str()))
        .collect();

    // Adjust scroll offset so the active tab is always visible.
    // 1) If active tab is before the scroll offset, scroll left.
    if active < tabs.tab_scroll_offset {
        tabs.tab_scroll_offset = active;
    }

    // 2) If active tab is past the right edge, scroll right until it fits.
    loop {
        let left_indicator_width = if tabs.tab_scroll_offset > 0 {
            UnicodeWidthStr::width(format!(" <{} ", tabs.tab_scroll_offset).as_str())
        } else {
            0
        };

        let mut used = left_indicator_width;
        let mut active_fits = false;
        #[allow(clippy::needless_range_loop)]
        for i in tabs.tab_scroll_offset..num_tabs {
            // Reserve space for right overflow indicator
            let remaining_after = num_tabs - i - 1;
            let right_reserve = if remaining_after > 0 { 4 } else { 0 };

            if used + tab_widths[i] > available_width.saturating_sub(right_reserve)
                && i != tabs.tab_scroll_offset
            {
                break;
            }
            if i == active {
                active_fits = true;
            }
            used += tab_widths[i];
        }

        if active_fits || tabs.tab_scroll_offset >= active {
            break;
        }
        tabs.tab_scroll_offset += 1;
        if tabs.tab_scroll_offset >= num_tabs {
            tabs.tab_scroll_offset = active;
            break;
        }
    }

    // Clamp scroll offset
    if tabs.tab_scroll_offset >= num_tabs {
        tabs.tab_scroll_offset = 0;
    }

    // Now render
    let mut spans: Vec<Span> = Vec::new();
    let mut used_width = 0;

    // Left overflow indicator
    if tabs.tab_scroll_offset > 0 {
        let left_label = format!(" <{} ", tabs.tab_scroll_offset);
        used_width += UnicodeWidthStr::width(left_label.as_str());
        spans.push(Span::styled(
            left_label,
            Style::default().fg(Color::DarkGray),
        ));
    }

    for i in tabs.tab_scroll_offset..num_tabs {
        let title = &tab_titles[i];
        let title_len = tab_widths[i];

        // Check if this tab fits; reserve space for right overflow indicator
        let remaining_after = num_tabs - i - 1;
        let right_reserve = if remaining_after > 0 { 4 } else { 0 };

        if used_width + title_len > available_width.saturating_sub(right_reserve) {
            let remaining = num_tabs - i;
            spans.push(Span::styled(
                format!(" +{remaining} "),
                Style::default().fg(Color::DarkGray),
            ));
            break;
        }

        let style = if i == active {
            Style::default().bg(Color::Cyan).fg(Color::Black)
        } else {
            Style::default().bg(Color::DarkGray).fg(Color::White)
        };
        spans.push(Span::styled(title.clone(), style));
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
    let search_term_char_len = editor.search.search_buffer.chars().count();
    let selection_range = compute_selection_range(editor);
    let mut line_chars_buf: Vec<char> = Vec::with_capacity(256);

    let buffer_id = editor.buffer_id;
    let dirty_gen = editor.dirty_generation;
    let can_cache = selection_range.is_none() && search_term_char_len == 0;
    RENDER_CACHE.with(|c| {
        c.borrow_mut()
            .reset_if_stale(buffer_id, dirty_gen, show_whitespace)
    });

    for i in 0..visible_lines {
        let line_idx = editor.viewport.viewport_offset.0 + i;
        if line_idx < editor.rope.len_lines() {
            let mut styled_spans: Vec<Span> = vec![];

            if show_line_numbers {
                let line_num = format!("{:width$} ", line_idx + 1, width = line_num_width - 1);
                styled_spans.push(Span::styled(line_num, Style::default().fg(Color::DarkGray)));
            }

            let cached_content: Option<Rc<Vec<Span<'static>>>> = if can_cache {
                RENDER_CACHE.with(|c| c.borrow().lines.get(&line_idx).cloned())
            } else {
                None
            };

            let sliced = if let Some(cached) = cached_content {
                slice_spans_horizontal(cached.as_slice(), h_offset, content_width)
            } else {
                let line_text = get_line_str(&editor.rope, line_idx);

                let highlighted_spans: Rc<Vec<(Style, String)>> =
                    editor.highlighter.highlight_line(line_idx, &line_text);

                let line_content = line_text.trim_end_matches('\n');

                let line_match_range = line_match_slice(&editor.search.search_matches, line_idx);
                let mut search_spans = apply_search_highlighting(
                    &highlighted_spans,
                    line_content,
                    line_idx,
                    search_term_char_len,
                    &editor.search.search_matches,
                    editor.search.current_match_index,
                    line_match_range,
                    &mut line_chars_buf,
                );

                if show_whitespace {
                    for span in &mut search_spans {
                        let rendered = render_whitespace(&span.content);
                        if rendered != span.content.as_ref() {
                            *span = Span::styled(rendered, span.style);
                        }
                    }
                }

                // Expand tabs last: search and selection highlighting work in
                // rope char positions, which a tab-to-spaces expansion would
                // shift.
                let final_spans = expand_tabs_in_spans(
                    apply_selection_highlighting(search_spans, line_idx, editor, selection_range),
                    show_whitespace,
                );

                let sliced = slice_spans_horizontal(&final_spans, h_offset, content_width);

                if can_cache {
                    RENDER_CACHE.with(|c| {
                        c.borrow_mut().lines.insert(line_idx, Rc::new(final_spans));
                    });
                }

                sliced
            };

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
        f.set_cursor_position(Position::new(
            cursor_x,
            cursor_screen_y as u16 + editor_area.y,
        ));
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
    let search_term_char_len = editor.search.search_buffer.chars().count();
    let selection_range = compute_selection_range(editor);

    let mut lines: Vec<Line> = vec![];
    let mut screen_row = 0;
    let mut cursor_screen_pos: Option<(usize, usize)> = None;
    let mut line_idx = editor.viewport.viewport_offset.0;
    let mut line_chars_buf: Vec<char> = Vec::with_capacity(256);

    while screen_row < visible_lines && line_idx < editor.rope.len_lines() {
        let line_text = get_line_str(&editor.rope, line_idx);

        let highlighted_spans: Rc<Vec<(Style, String)>> =
            editor.highlighter.highlight_line(line_idx, &line_text);

        let line_content = line_text.trim_end_matches('\n');

        let line_match_range = line_match_slice(&editor.search.search_matches, line_idx);
        let mut search_spans = apply_search_highlighting(
            &highlighted_spans,
            line_content,
            line_idx,
            search_term_char_len,
            &editor.search.search_matches,
            editor.search.current_match_index,
            line_match_range,
            &mut line_chars_buf,
        );

        if show_whitespace {
            for span in &mut search_spans {
                let rendered = render_whitespace(&span.content);
                if rendered != span.content.as_ref() {
                    *span = Span::styled(rendered, span.style);
                }
            }
        }

        // Expand tabs last: search and selection highlighting work in rope
        // char positions, which a tab-to-spaces expansion would shift.
        let final_spans = expand_tabs_in_spans(
            apply_selection_highlighting(search_spans, line_idx, editor, selection_range),
            show_whitespace,
        );

        // Wrap by display columns (wide chars count 2, tabs are expanded),
        // matching the editor's `wrapped_line_height` math.
        let line_width: usize = final_spans
            .iter()
            .map(|s| UnicodeWidthStr::width(s.content.as_ref()))
            .sum();
        let rows_needed = if content_width == 0 || line_width == 0 {
            1
        } else {
            line_width.div_ceil(content_width)
        };

        if line_idx == editor.viewport.cursor_pos.0 {
            let cursor_col = editor.viewport.cursor_pos.1;
            let (cursor_sub_row, cursor_col_in_row) = if content_width > 0 {
                // Clamp to the line's last rendered row: a cursor at the end
                // of an exactly-full row would otherwise land on the next
                // document line's row.
                let sub_row = (cursor_col / content_width).min(rows_needed - 1);
                (sub_row, cursor_col - sub_row * content_width)
            } else {
                (0, cursor_col)
            };
            cursor_screen_pos = Some((screen_row + cursor_sub_row, cursor_col_in_row));
        }

        for sub_row in 0..rows_needed {
            if screen_row >= visible_lines {
                break;
            }

            let mut styled_spans: Vec<Span> = vec![];

            if show_line_numbers {
                if sub_row == 0 {
                    let line_num = format!("{:width$} ", line_idx + 1, width = line_num_width - 1);
                    styled_spans.push(Span::styled(line_num, Style::default().fg(Color::DarkGray)));
                } else {
                    let empty_num = format!("{:width$} ", "", width = line_num_width - 1);
                    styled_spans.push(Span::styled(
                        empty_num,
                        Style::default().fg(Color::DarkGray),
                    ));
                }
            }

            let start_col = sub_row * content_width;
            let end_col = start_col + content_width;

            if start_col < line_width {
                let sub_chars = collect_span_chars_range(&final_spans, start_col, end_col);
                styled_spans.extend(group_chars_into_spans(&sub_chars));
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
    if let Some((screen_y, cursor_col_in_row)) = cursor_screen_pos {
        if screen_y < visible_lines {
            // A cursor clamped onto the last row of an exactly-full line sits
            // one column past it; keep the drawn cursor inside the area.
            let cursor_x = (cursor_col_in_row as u16 + line_num_width as u16)
                .min(editor_area.width.saturating_sub(1));
            f.set_cursor_position(Position::new(cursor_x, screen_y as u16 + editor_area.y));
        }
    }
}

/// Slice a list of spans to only include the display column range
/// [h_offset, h_offset + width). Columns are display columns (wide chars
/// count 2; tabs must already be expanded by `expand_tabs_in_spans`). A wide
/// char straddling either edge is replaced by padding spaces for its visible
/// columns so the slice stays column-exact.
fn slice_spans_horizontal(spans: &[Span<'_>], h_offset: usize, width: usize) -> Vec<Span<'static>> {
    if h_offset == 0 && width == usize::MAX {
        return spans
            .iter()
            .map(|s| Span::styled(s.content.to_string(), s.style))
            .collect();
    }

    let mut result = Vec::new();
    let mut col = 0usize;
    let end = h_offset.saturating_add(width);

    for span in spans {
        if col >= end {
            break;
        }

        let span_width = UnicodeWidthStr::width(span.content.as_ref());
        let span_end = col + span_width;

        if span_end <= h_offset {
            col = span_end;
            continue;
        }

        let mut visible = String::new();
        let mut ch_col = col;
        for ch in span.content.chars() {
            if ch_col >= end {
                break;
            }
            let w = UnicodeWidthChar::width(ch).unwrap_or(0);
            let ch_end = ch_col + w;
            if ch_end <= h_offset {
                ch_col = ch_end;
                continue;
            }
            if ch_col >= h_offset && ch_end <= end {
                visible.push(ch);
            } else {
                for _ in ch_col.max(h_offset)..ch_end.min(end) {
                    visible.push(' ');
                }
            }
            ch_col = ch_end;
        }
        if !visible.is_empty() {
            result.push(Span::styled(visible, span.style));
        }

        col = span_end;
    }

    result
}

/// Collect characters with their styles from a list of spans, but only those
/// whose display columns fall in `[start_col, end_col)`. Columns are display
/// columns (wide chars count 2; tabs must already be expanded). A wide char
/// straddling either edge contributes padding spaces for its visible columns
/// so each wrapped sub-row stays column-exact.
fn collect_span_chars_range(
    spans: &[Span<'_>],
    start_col: usize,
    end_col: usize,
) -> Vec<(char, Style)> {
    let mut result = Vec::new();
    if end_col <= start_col {
        return result;
    }
    let mut col = 0usize;
    for span in spans {
        if col >= end_col {
            break;
        }
        let span_style = span.style;
        for ch in span.content.chars() {
            if col >= end_col {
                break;
            }
            let w = UnicodeWidthChar::width(ch).unwrap_or(0);
            let ch_end = col + w;
            if ch_end <= start_col {
                col = ch_end;
                continue;
            }
            if col >= start_col && ch_end <= end_col {
                result.push((ch, span_style));
            } else {
                for _ in col.max(start_col)..ch_end.min(end_col) {
                    result.push((' ', span_style));
                }
            }
            col = ch_end;
        }
    }
    result
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

pub fn help_lines() -> Vec<&'static str> {
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
        "^PgUp    Previous tab",
        "^PgDn    Next tab",
        "M-,      Previous tab",
        "M-.      Next tab",
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

#[allow(clippy::too_many_arguments)]
fn apply_search_highlighting(
    syntax_spans: &[(Style, String)],
    line_content: &str,
    line_idx: usize,
    search_term_char_len: usize,
    search_matches: &[(usize, usize)],
    current_match_index: Option<usize>,
    line_match_range: (usize, usize),
    line_chars_buf: &mut Vec<char>,
) -> Vec<Span<'static>> {
    let (range_start, range_end) = line_match_range;
    if search_term_char_len == 0 || range_start >= range_end {
        return syntax_spans
            .iter()
            .map(|(style, text)| {
                let clean_text = text.trim_end_matches('\n').to_string();
                Span::styled(clean_text, *style)
            })
            .collect();
    }

    let line_matches = &search_matches[range_start..range_end];

    let current_match_col = current_match_index
        .and_then(|idx| search_matches.get(idx))
        .filter(|(match_line, _)| *match_line == line_idx)
        .map(|(_, match_col)| *match_col);

    // Clip overlapping matches up-front (the literal scanner can report
    // overlaps) into non-overlapping highlight ranges in char positions.
    let mut highlight_ranges: Vec<(usize, usize, Style)> = Vec::with_capacity(line_matches.len());
    let mut prev_end = 0;
    for &(_, match_char_pos) in line_matches {
        if match_char_pos < prev_end {
            continue;
        }
        let style = if Some(match_char_pos) == current_match_col {
            Style::default().bg(Color::Red).fg(Color::White)
        } else {
            Style::default().bg(Color::Yellow).fg(Color::Black)
        };
        prev_end = match_char_pos + search_term_char_len;
        highlight_ranges.push((match_char_pos, prev_end, style));
    }

    // Single forward pass: walk the line's chars and the syntax spans in
    // lockstep (one style per char), overriding with the highlight style
    // inside match ranges, and group consecutive same-style chars into spans.
    line_chars_buf.clear();
    line_chars_buf.extend(line_content.chars());
    let per_char_syntax = syntax_spans
        .iter()
        .flat_map(|(style, text)| text.chars().filter(|&c| c != '\n').map(move |_| *style))
        .chain(std::iter::repeat(Style::default()));

    let mut result_spans = Vec::new();
    let mut current_text = String::new();
    let mut current_style = Style::default();
    let mut range_i = 0;
    for (char_pos, (&ch, syntax_style)) in line_chars_buf.iter().zip(per_char_syntax).enumerate() {
        while range_i < highlight_ranges.len() && highlight_ranges[range_i].1 <= char_pos {
            range_i += 1;
        }
        let style = match highlight_ranges.get(range_i) {
            Some(&(start, _, highlight)) if char_pos >= start => highlight,
            _ => syntax_style,
        };
        if style != current_style && !current_text.is_empty() {
            result_spans.push(Span::styled(std::mem::take(&mut current_text), current_style));
        }
        current_style = style;
        current_text.push(ch);
    }
    if !current_text.is_empty() {
        result_spans.push(Span::styled(current_text, current_style));
    }

    result_spans
}

/// Find the slice of `search_matches` (pre-sorted by line, then column) whose
/// entries belong to `line_idx`. Returns `(start, end)` indices into the slice.
fn line_match_slice(search_matches: &[(usize, usize)], line_idx: usize) -> (usize, usize) {
    let start = search_matches.partition_point(|(l, _)| *l < line_idx);
    let end = search_matches.partition_point(|(l, _)| *l <= line_idx);
    (start, end)
}

/// Make spaces visible for show-whitespace mode. Hard tabs are left alone
/// here (so char positions stay aligned with the rope for selection
/// highlighting); `expand_tabs_in_spans` draws their visible marker instead.
fn render_whitespace(text: &str) -> String {
    text.replace(' ', "\u{00B7}")
}

/// Expand hard tabs in fully-styled spans into spaces aligned to `TAB_WIDTH`
/// stops, tracking the running display column across spans (wide chars count
/// 2 columns). This is the renderer half of the tab handling: it must agree
/// with `char_display_width` so the terminal layout matches the cursor math.
/// With `show_whitespace`, the first cell of each expansion is a visible
/// marker, padded with plain spaces.
fn expand_tabs_in_spans(spans: Vec<Span<'static>>, show_whitespace: bool) -> Vec<Span<'static>> {
    let mut col = 0usize;
    let mut result = Vec::with_capacity(spans.len());
    for span in spans {
        if !span.content.contains('\t') {
            col += UnicodeWidthStr::width(span.content.as_ref());
            result.push(span);
            continue;
        }
        let mut text = String::with_capacity(span.content.len());
        for ch in span.content.chars() {
            let w = char_display_width(ch, col);
            if ch == '\t' {
                text.push(if show_whitespace { '\u{2192}' } else { ' ' });
                for _ in 1..w {
                    text.push(' ');
                }
            } else {
                text.push(ch);
            }
            col += w;
        }
        result.push(Span::styled(text, span.style));
    }
    result
}

/// Compute the active selection range as `(start_char_idx, end_char_idx)` once
/// per frame, avoiding repeated `line_col_to_char_idx` walks in the per-line
/// highlighting loop. Returns `None` when no mark is set.
fn compute_selection_range(editor: &Editor) -> Option<(usize, usize)> {
    let anchor = editor.mark_anchor?;
    let cursor = editor.viewport.cursor_pos;
    let anchor_idx = editor.line_col_to_char_idx(anchor.0, anchor.1);
    let cursor_idx = editor.line_col_to_char_idx(cursor.0, cursor.1);
    Some(if anchor_idx <= cursor_idx {
        (anchor_idx, cursor_idx)
    } else {
        (cursor_idx, anchor_idx)
    })
}

fn apply_selection_highlighting(
    spans: Vec<Span<'static>>,
    line_idx: usize,
    editor: &Editor,
    selection_range: Option<(usize, usize)>,
) -> Vec<Span<'static>> {
    let (sel_start, sel_end) = match selection_range {
        Some(range) => range,
        None => return spans,
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

/// Truncate `label` to at most `max_width` display columns, appending "..."
/// when truncation occurs. Operates on display width rather than bytes, so
/// multibyte and wide (CJK) characters never split a char boundary.
fn truncate_to_width(label: String, max_width: usize) -> String {
    if UnicodeWidthStr::width(label.as_str()) <= max_width {
        return label;
    }
    let ellipsis = "...";
    let budget = max_width.saturating_sub(UnicodeWidthStr::width(ellipsis));
    let mut truncated = String::with_capacity(budget + ellipsis.len());
    let mut used = 0;
    for ch in label.chars() {
        let w = UnicodeWidthChar::width(ch).unwrap_or(0);
        if used + w > budget {
            break;
        }
        truncated.push(ch);
        used += w;
    }
    truncated.push_str(ellipsis);
    truncated
}

/// Render the fuzzy finder as a centered overlay.
fn draw_fuzzy_finder(f: &mut Frame, tabs: &TabManager, area: Rect) {
    use ratatui::widgets::Clear;

    let width = 50u16.min(area.width.saturating_sub(4));
    let max_results = 10usize;
    let height = (max_results as u16 + 3).min(area.height.saturating_sub(4)); // +3 for border + input line
    let x = area.x + (area.width.saturating_sub(width)) / 2;
    let y = area.y + (area.height.saturating_sub(height)) / 3; // position in upper third

    let overlay_area = Rect {
        x,
        y,
        width,
        height,
    };

    // Clear the area behind the overlay
    f.render_widget(Clear, overlay_area);

    // Build the candidate list and filter
    let candidates: Vec<(usize, String)> = tabs
        .tabs
        .iter()
        .enumerate()
        .map(|(i, t)| (i, t.display_name.clone()))
        .collect();
    let filtered = crate::fuzzy::fuzzy_filter(&tabs.fuzzy_query, &candidates);

    // Clamp selection
    let selected = if filtered.is_empty() {
        0
    } else {
        tabs.fuzzy_selected.min(filtered.len() - 1)
    };

    let mut lines: Vec<Line> = Vec::new();

    // Input line
    let input_line = format!("> {}", tabs.fuzzy_query);
    lines.push(Line::from(Span::styled(
        input_line,
        Style::default().fg(Color::White),
    )));

    // Separator
    let sep = "\u{2500}".repeat((width as usize).saturating_sub(2));
    lines.push(Line::from(Span::styled(
        sep,
        Style::default().fg(Color::DarkGray),
    )));

    // Results
    let visible_results = (height as usize).saturating_sub(4); // borders + input + separator
    for (i, (tab_idx, name, _score)) in filtered.iter().take(visible_results).enumerate() {
        let modified = if tabs.tabs[*tab_idx].modified {
            " [+]"
        } else {
            ""
        };
        let label = format!(" {}: {}{}", tab_idx + 1, name, modified);
        let truncated = truncate_to_width(label, (width as usize).saturating_sub(2));

        let style = if i == selected {
            Style::default().bg(Color::Cyan).fg(Color::Black)
        } else {
            Style::default().fg(Color::White)
        };
        lines.push(Line::from(Span::styled(truncated, style)));
    }

    if filtered.is_empty() {
        lines.push(Line::from(Span::styled(
            " No matching tabs",
            Style::default().fg(Color::DarkGray),
        )));
    }

    let block = Block::default()
        .borders(Borders::ALL)
        .title(" Switch Tab ")
        .border_style(Style::default().fg(Color::Cyan));

    let paragraph = Paragraph::new(lines).block(block);
    f.render_widget(paragraph, overlay_area);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn truncate_to_width_ascii() {
        let out = truncate_to_width("abcdefghij".to_string(), 8);
        assert_eq!(out, "abcde...");
        assert!(UnicodeWidthStr::width(out.as_str()) <= 8);
    }

    #[test]
    fn truncate_to_width_cjk() {
        // Each CJK char is 2 columns wide; total width 20 > 9.
        let out = truncate_to_width("日本語のファイル名前".to_string(), 9);
        // Budget is 6 columns -> 3 CJK chars, then "...": width 9.
        assert_eq!(out, "日本語...");
        assert!(UnicodeWidthStr::width(out.as_str()) <= 9);
    }

    #[test]
    fn truncate_to_width_short_label_unchanged() {
        let out = truncate_to_width("short".to_string(), 20);
        assert_eq!(out, "short");
    }

    use crate::editor::TAB_WIDTH;
    use crate::tabs::TabManager;
    use ratatui::backend::TestBackend;
    use ratatui::Terminal;
    use ropey::Rope;

    fn span_text(spans: &[Span<'_>]) -> String {
        spans.iter().map(|s| s.content.as_ref()).collect()
    }

    #[test]
    fn expand_tabs_places_next_char_at_tab_stop() {
        let spans = vec![Span::raw("a\tb".to_string())];
        let out = span_text(&expand_tabs_in_spans(spans, false));
        // 'b' must land at the column the editor math predicts.
        let mut e = crate::editor::Editor::new_for_test();
        e.rope = Rope::from_str("a\tb\n");
        let predicted = e.char_idx_to_display_col(0, 2);
        assert_eq!(predicted, TAB_WIDTH);
        assert_eq!(out, format!("a{}b", " ".repeat(TAB_WIDTH - 1)));
        assert_eq!(out.chars().position(|c| c == 'b'), Some(predicted));
    }

    #[test]
    fn expand_tabs_tracks_columns_across_spans_and_wide_chars() {
        // "あ" occupies cols 0-1, so the tab in the second span starts at
        // col 2 and expands to only 2 spaces.
        let spans = vec![
            Span::raw("あ".to_string()),
            Span::raw("\tx".to_string()),
        ];
        let out = expand_tabs_in_spans(spans, false);
        assert_eq!(out[1].content.as_ref(), "  x");
    }

    #[test]
    fn expand_tabs_show_whitespace_keeps_marker_first_cell() {
        let spans = vec![Span::raw("\t".to_string())];
        let out = span_text(&expand_tabs_in_spans(spans, true));
        assert_eq!(out, format!("\u{2192}{}", " ".repeat(TAB_WIDTH - 1)));
    }

    #[test]
    fn slice_spans_horizontal_uses_display_columns() {
        // 'あ' spans cols 0-1; slicing from col 1 pads its visible half with
        // a space so 'a' (col 2) stays at screen col 1.
        let spans = vec![Span::raw("あab".to_string())];
        let out = slice_spans_horizontal(&spans, 1, 3);
        assert_eq!(span_text(&out), " ab");
    }

    #[test]
    fn collect_span_chars_range_uses_display_columns() {
        // Row of width 2 over "aあ": 'あ' straddles the row end, so only its
        // first column is padded into this row.
        let spans = vec![Span::raw("aあ".to_string())];
        let out = collect_span_chars_range(&spans, 0, 2);
        let text: String = out.iter().map(|(c, _)| *c).collect();
        assert_eq!(text, "a ");
    }

    #[test]
    fn search_highlight_preserves_syntax_styles_per_char() {
        let s1 = Style::default().fg(Color::Green);
        let s2 = Style::default().fg(Color::Blue);
        // Multibyte chars: byte-based style lookup would misattribute these.
        let syntax = vec![(s1, "あa".to_string()), (s2, "bc\n".to_string())];
        let matches = vec![(0usize, 3usize)];
        let mut buf = Vec::new();
        let spans =
            apply_search_highlighting(&syntax, "あabc", 0, 1, &matches, None, (0, 1), &mut buf);
        let texts: Vec<&str> = spans.iter().map(|s| s.content.as_ref()).collect();
        assert_eq!(texts, vec!["あa", "b", "c"]);
        assert_eq!(spans[0].style, s1);
        assert_eq!(spans[1].style, s2);
        assert_eq!(
            spans[2].style,
            Style::default().bg(Color::Yellow).fg(Color::Black)
        );
    }

    #[test]
    fn rendered_tab_line_matches_editor_column_math() {
        let mut t = TabManager::new_for_test();
        t.active_editor_mut().rope = Rope::from_str("a\tb\n");
        let predicted = t.active_editor().char_idx_to_display_col(0, 2);

        let backend = TestBackend::new(40, 8);
        let mut term = Terminal::new(backend).unwrap();
        term.draw(|f| draw_ui(f, &mut t)).unwrap();
        let buf = term.backend().buffer();
        // Editor area starts at row 1 (below the tab bar).
        assert_eq!(buf[(predicted as u16, 1)].symbol(), "b");
    }

    #[test]
    fn rendered_search_highlight_lands_on_tab_expanded_column() {
        let mut t = TabManager::new_for_test();
        {
            let e = t.active_editor_mut();
            e.rope = Rope::from_str("\tfoo\n");
            // Matches store (line, CHAR col); 'f' is char 1, display col
            // TAB_WIDTH.
            e.search.search_buffer = "foo".to_string();
            e.search.search_matches = vec![(0, 1)];
            e.search.current_match_index = Some(0);
        }

        let backend = TestBackend::new(40, 8);
        let mut term = Terminal::new(backend).unwrap();
        term.draw(|f| draw_ui(f, &mut t)).unwrap();
        let buf = term.backend().buffer();
        let cell = &buf[(TAB_WIDTH as u16, 1)];
        assert_eq!(cell.symbol(), "f");
        assert_eq!(cell.style().bg, Some(Color::Red));
        // The tab's own cells are not highlighted.
        assert_ne!(buf[(0, 1)].style().bg, Some(Color::Red));
    }

    #[test]
    fn rendered_selection_covers_whole_tab_expansion() {
        let mut t = TabManager::new_for_test();
        {
            let e = t.active_editor_mut();
            e.rope = Rope::from_str("\tx\n");
            e.mark_anchor = Some((0, 0));
            e.viewport.cursor_pos = (0, TAB_WIDTH);
        }

        let backend = TestBackend::new(40, 8);
        let mut term = Terminal::new(backend).unwrap();
        term.draw(|f| draw_ui(f, &mut t)).unwrap();
        let buf = term.backend().buffer();
        for col in 0..TAB_WIDTH as u16 {
            assert_eq!(
                buf[(col, 1)].style().bg,
                Some(Color::White),
                "tab expansion col {col} should be selected"
            );
        }
        assert_ne!(buf[(TAB_WIDTH as u16, 1)].style().bg, Some(Color::White));
    }

    #[test]
    fn word_wrap_cursor_at_exact_row_end_stays_on_line() {
        let width = 40u16;
        let mut t = TabManager::new_for_test();
        t.config.word_wrap = true;
        {
            let e = t.active_editor_mut();
            // First line exactly fills one wrapped row; cursor at its end.
            e.rope = Rope::from_str(&format!("{}\nzz\n", "a".repeat(width as usize)));
            e.viewport.cursor_pos = (0, width as usize);
        }

        let backend = TestBackend::new(width, 8);
        let mut term = Terminal::new(backend).unwrap();
        term.draw(|f| draw_ui(f, &mut t)).unwrap();
        let pos = term.get_cursor_position().unwrap();
        // Row 1 is the first editor row (line 0); the cursor must not be
        // drawn on row 2 (which renders document line 1).
        assert_eq!(pos.y, 1);
    }

    #[test]
    fn word_wrap_wraps_wide_chars_by_display_width() {
        let width = 40u16;
        let mut t = TabManager::new_for_test();
        t.config.word_wrap = true;
        {
            let e = t.active_editor_mut();
            // 30 CJK chars = 60 display columns: two wrapped rows even though
            // the char count (30) fits in one.
            e.rope = Rope::from_str(&format!("{}\n", "あ".repeat(30)));
        }

        let backend = TestBackend::new(width, 8);
        let mut term = Terminal::new(backend).unwrap();
        term.draw(|f| draw_ui(f, &mut t)).unwrap();
        let buf = term.backend().buffer();
        // Row 2 is the continuation row; it must start with the 21st 'あ'
        // (display col 40 = char 20).
        assert_eq!(buf[(0, 2)].symbol(), "あ");
    }
}
