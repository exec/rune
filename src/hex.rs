use ratatui::{
    prelude::*,
    text::{Line, Span},
    widgets::{Block, Borders, Paragraph},
};

pub const BYTES_PER_ROW: usize = 16;

pub struct HexViewState {
    pub raw_bytes: Vec<u8>,
    pub cursor: usize,
    pub scroll_offset: usize,
}

impl HexViewState {
    pub fn new(raw_bytes: Vec<u8>) -> Self {
        Self {
            raw_bytes,
            cursor: 0,
            scroll_offset: 0,
        }
    }
}

fn byte_to_ascii_char(b: u8) -> char {
    if b.is_ascii_graphic() || b == b' ' {
        b as char
    } else {
        '.'
    }
}

pub fn draw_hex_view(f: &mut Frame, area: Rect, state: &mut HexViewState) {
    let visible_rows = area.height as usize;

    // Adjust scroll to keep cursor visible
    let cursor_row = state.cursor / BYTES_PER_ROW;
    if cursor_row < state.scroll_offset {
        state.scroll_offset = cursor_row;
    }
    if cursor_row >= state.scroll_offset + visible_rows {
        state.scroll_offset = cursor_row.saturating_sub(visible_rows - 1);
    }

    let cursor_col = state.cursor % BYTES_PER_ROW;

    let mut lines = Vec::with_capacity(visible_rows);

    for row_idx in 0..visible_rows {
        let row = state.scroll_offset + row_idx;
        let row_start = row * BYTES_PER_ROW;

        if row_start >= state.raw_bytes.len() {
            lines.push(Line::from(Span::styled(
                "~",
                Style::default().fg(Color::DarkGray),
            )));
            continue;
        }

        let row_end = (row_start + BYTES_PER_ROW).min(state.raw_bytes.len());
        let row_bytes = &state.raw_bytes[row_start..row_end];
        let is_cursor_row = row == cursor_row;

        let mut spans: Vec<Span> = Vec::new();

        // Offset column
        spans.push(Span::styled(
            format!("{:08X} ", row_start),
            Style::default().fg(Color::DarkGray),
        ));
        spans.push(Span::styled(
            "\u{2502} ",
            Style::default().fg(Color::DarkGray),
        ));

        // Hex column
        for (i, &byte) in row_bytes.iter().enumerate() {
            let hex_str = format!("{:02X}", byte);
            let style = if is_cursor_row && i == cursor_col {
                Style::default().bg(Color::Cyan).fg(Color::Black)
            } else {
                Style::default().fg(Color::White)
            };
            spans.push(Span::styled(hex_str, style));

            if i == 7 {
                spans.push(Span::raw("  "));
            } else if i < BYTES_PER_ROW - 1 {
                spans.push(Span::raw(" "));
            }
        }

        // Pad remaining hex columns if row is short
        let missing = BYTES_PER_ROW - row_bytes.len();
        for i in 0..missing {
            spans.push(Span::raw("  "));
            let col = row_bytes.len() + i;
            if col == 7 {
                spans.push(Span::raw("  "));
            } else if col < BYTES_PER_ROW - 1 {
                spans.push(Span::raw(" "));
            }
        }

        // Separator
        spans.push(Span::styled(
            " \u{2502}",
            Style::default().fg(Color::DarkGray),
        ));

        // ASCII column
        for (i, &byte) in row_bytes.iter().enumerate() {
            let ch = byte_to_ascii_char(byte);
            let style = if is_cursor_row && i == cursor_col {
                Style::default().bg(Color::Cyan).fg(Color::Black)
            } else if byte.is_ascii_graphic() || byte == b' ' {
                Style::default().fg(Color::Green)
            } else {
                Style::default().fg(Color::DarkGray)
            };
            spans.push(Span::styled(String::from(ch), style));

            if i == 7 {
                spans.push(Span::raw(" "));
            }
        }

        lines.push(Line::from(spans));
    }

    let widget = Paragraph::new(lines).block(Block::default().borders(Borders::NONE));
    f.render_widget(widget, area);
}
