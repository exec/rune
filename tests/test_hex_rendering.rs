#![cfg(feature = "terminal")]

use ratatui::{backend::TestBackend, buffer::Buffer, prelude::*, Terminal};
use rune::hex::{draw_hex_view, HexViewState};

const BYTES: [u8; 16] = [
    0, 1, 9, 10, 13, 31, 32, 33, 65, 90, 97, 122, 126, 127, 128, 255,
];
const GOLDEN: &str =
    "00000000 │ 00 01 09 0A 0D 1F 20 21  41 5A 61 7A 7E 7F 80 FF │...... ! AZaz~...";

fn initial_cell() -> ratatui::buffer::Cell {
    let mut cell = ratatui::buffer::Cell::default();
    cell.set_char('x').set_style(
        Style::default()
            .fg(Color::Magenta)
            .bg(Color::Blue)
            .add_modifier(Modifier::BOLD),
    );
    cell
}

fn render(
    width: u16,
    height: u16,
    bytes: &[u8],
    cursor: usize,
    scroll: usize,
) -> (Buffer, HexViewState) {
    let mut state = HexViewState::new(bytes.to_vec());
    state.cursor = cursor;
    state.scroll_offset = scroll;
    let mut terminal = Terminal::new(TestBackend::new(110, 10)).unwrap();
    terminal
        .draw(|f| {
            f.buffer_mut().content.fill(initial_cell());
            draw_hex_view(f, Rect::new(3, 2, width, height), &mut state);
        })
        .unwrap();
    (terminal.backend().buffer().clone(), state)
}

fn row_text(buffer: &Buffer, y: u16, width: u16) -> String {
    (3..3 + width).map(|x| buffer[(x, y)].symbol()).collect()
}

#[test]
fn full_row_matches_fixed_layout_and_styles() {
    let (buffer, state) = render(100, 2, &BYTES, 7, 0);
    assert_eq!(
        row_text(&buffer, 2, 100),
        format!("{GOLDEN}{}", "x".repeat(22))
    );
    assert_eq!(row_text(&buffer, 3, 100), format!("~{}", "x".repeat(99)));
    for col in 0..100 {
        let mut fg = if col < 11 || col == 59 || col == 60 {
            Color::DarkGray
        } else {
            Color::Magenta
        };
        let mut bg = Color::Blue;
        for (i, &byte) in BYTES.iter().enumerate() {
            let hex = 11 + 3 * i + usize::from(i >= 8);
            let ascii = 61 + i + usize::from(i >= 8);
            if col == hex || col == hex + 1 {
                fg = if i == 7 { Color::Black } else { Color::White };
                if i == 7 {
                    bg = Color::Cyan;
                }
            }
            if col == ascii {
                fg = if i == 7 {
                    Color::Black
                } else if byte.is_ascii_graphic() || byte == b' ' {
                    Color::Green
                } else {
                    Color::DarkGray
                };
                if i == 7 {
                    bg = Color::Cyan;
                }
            }
        }
        let cell = &buffer[(3 + col as u16, 2)];
        assert_eq!(
            (cell.fg, cell.bg, cell.modifier),
            (fg, bg, Modifier::BOLD),
            "column {col}"
        );
    }
    assert_eq!(state.raw_bytes, BYTES);
}

#[test]
fn narrow_and_zero_sized_areas_clip_without_touching_neighbors() {
    let (full, _) = render(100, 2, &BYTES, 7, 0);
    for width in 0..=100 {
        for height in 0..=2 {
            let (actual, _) = render(width, height, &BYTES, 7, 0);
            let mut expected = Buffer::filled(Rect::new(0, 0, 110, 10), initial_cell());
            for y in 2..2 + height {
                for x in 3..3 + width {
                    expected[(x, y)] = full[(x, y)].clone();
                }
            }
            assert_eq!(actual, expected, "width={width} height={height}");
        }
    }
}

#[test]
fn partial_row_pads_hex_columns_and_highlights_existing_bytes_only() {
    let (buffer, _) = render(100, 3, &[0x41, 0x09, 0xff], 2, 0);
    let expected = format!(
        "00000000 │ 41 09 FF{} │A..{}",
        " ".repeat(40),
        "x".repeat(36)
    );
    assert_eq!(row_text(&buffer, 2, 100), expected);
    for col in [17, 18, 63] {
        assert_eq!(
            (buffer[(3 + col, 2)].fg, buffer[(3 + col, 2)].bg),
            (Color::Black, Color::Cyan)
        );
    }
    assert_eq!(row_text(&buffer, 3, 1), "~");
}

#[test]
fn scrolling_and_zero_dimensions_preserve_cursor_visibility_rules() {
    let bytes: Vec<_> = (0..1024).map(|i| i as u8).collect();
    for (scroll, expected) in [(0, 43), (100, 45)] {
        let (buffer, state) = render(100, 3, &bytes, 45 * 16 + 7, scroll);
        assert_eq!(state.scroll_offset, expected);
        assert_eq!(row_text(&buffer, 2, 8), format!("{:08X}", expected * 16));
        let row = 2 + (45 - expected) as u16;
        assert_eq!(buffer[(3 + 32, row)].bg, Color::Cyan);
        assert_eq!(buffer[(3 + 68, row)].bg, Color::Cyan);
    }
    let (buffer, state) = render(100, 0, &bytes, 45 * 16 + 7, 100);
    assert_eq!(state.scroll_offset, 100);
    assert!(buffer.content.iter().all(|cell| cell == &initial_cell()));
    let (buffer, state) = render(0, 3, &bytes, 45 * 16 + 7, 100);
    assert_eq!(state.scroll_offset, 45);
    assert!(buffer.content.iter().all(|cell| cell == &initial_cell()));
}
