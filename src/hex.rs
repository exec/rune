#[cfg(feature = "terminal")]
use ratatui::prelude::*;

pub use rune_core::hex::{HexViewState, BYTES_PER_ROW};

#[cfg(feature = "terminal")]
fn byte_to_ascii_char(b: u8) -> char {
    if b.is_ascii_graphic() || b == b' ' {
        b as char
    } else {
        '.'
    }
}

#[cfg(feature = "terminal")]
pub fn draw_hex_view(f: &mut Frame, area: Rect, state: &mut HexViewState) {
    let visible_rows = area.height as usize;
    if visible_rows == 0 {
        return;
    }

    // Adjust scroll to keep cursor visible
    let cursor_row = state.cursor / BYTES_PER_ROW;
    if cursor_row < state.scroll_offset {
        state.scroll_offset = cursor_row;
    }
    if cursor_row >= state.scroll_offset + visible_rows {
        state.scroll_offset = cursor_row.saturating_sub(visible_rows - 1);
    }

    let cursor_col = state.cursor % BYTES_PER_ROW;

    let dim = Style::default().fg(Color::DarkGray);
    let hex_style = Style::default().fg(Color::White);
    let ascii_style = Style::default().fg(Color::Green);
    let selected = Style::default().bg(Color::Cyan).fg(Color::Black);
    let plain = Style::default();
    let digits = b"0123456789ABCDEF";

    for row_idx in 0..visible_rows {
        let row = state.scroll_offset + row_idx;
        let row_start = row * BYTES_PER_ROW;
        let y = area.y + row_idx as u16;
        let mut col = 0usize;
        // Every emitted glyph is one cell wide; avoid allocating strings and spans.
        let mut put = |ch, style| {
            if col < area.width as usize {
                f.buffer_mut()[(area.x + col as u16, y)]
                    .set_char(ch)
                    .set_style(style);
            }
            col += 1;
        };
        if row_start >= state.raw_bytes.len() {
            put('~', dim);
            continue;
        }
        let row_end = (row_start + BYTES_PER_ROW).min(state.raw_bytes.len());
        let bytes = &state.raw_bytes[row_start..row_end];
        let is_cursor_row = row == cursor_row;

        let offset_digits = (usize::BITS - row_start.leading_zeros()).div_ceil(4).max(8);
        for shift in (0..offset_digits).rev() {
            put(digits[(row_start >> (shift * 4)) & 0xf] as char, dim);
        }
        put(' ', dim);
        put('│', dim);
        put(' ', dim);

        for i in 0..BYTES_PER_ROW {
            if let Some(&byte) = bytes.get(i) {
                let style = if is_cursor_row && i == cursor_col {
                    selected
                } else {
                    hex_style
                };
                put(digits[(byte >> 4) as usize] as char, style);
                put(digits[(byte & 0xf) as usize] as char, style);
            } else {
                put(' ', plain);
                put(' ', plain);
            }
            if i == 7 {
                put(' ', plain);
                put(' ', plain);
            } else if i < BYTES_PER_ROW - 1 {
                put(' ', plain);
            }
        }
        put(' ', dim);
        put('│', dim);

        for (i, &byte) in bytes.iter().enumerate() {
            let style = if is_cursor_row && i == cursor_col {
                selected
            } else if byte.is_ascii_graphic() || byte == b' ' {
                ascii_style
            } else {
                dim
            };
            put(byte_to_ascii_char(byte), style);
            if i == 7 {
                put(' ', plain);
            }
        }
    }
}
