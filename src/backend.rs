// Draw and modifier serialization adapted from Ratatui 0.29 (MIT); see LICENSE-RATATUI.
use std::io::{self, Write};

use crossterm::{
    cursor::{MoveTo, MoveToColumn},
    queue,
    style::{
        Attribute as CAttribute, Color as CColor, Colors, Print, SetAttribute, SetBackgroundColor,
        SetColors, SetForegroundColor, SetUnderlineColor,
    },
};
use ratatui::style::{Color, Modifier};
use ratatui::{
    backend::{Backend, ClearType, CrosstermBackend, WindowSize},
    buffer::Cell,
    layout::{Position, Size},
};
use unicode_width::UnicodeWidthStr;

/// Crossterm output with glyph-width tracking and short horizontal cursor moves.
/// shortcut: use upstream once a release supplies width tracking and short horizontal moves.
pub struct TerminalBackend<W: Write> {
    writer: W,
}

impl<W: Write> TerminalBackend<W> {
    pub fn new(writer: W) -> Self {
        Self { writer }
    }
}

impl<W: Write> Backend for TerminalBackend<W> {
    fn draw<'a, I>(&mut self, content: I) -> io::Result<()>
    where
        I: Iterator<Item = (u16, u16, &'a Cell)>,
    {
        let mut fg = Color::Reset;
        let mut bg = Color::Reset;
        let mut underline_color = Color::Reset;
        let mut modifier = Modifier::empty();
        let mut last_pos: Option<Position> = None;
        for (x, y, cell) in content {
            if !matches!(last_pos, Some(p) if x == p.x + 1 && y == p.y) {
                if last_pos.is_some_and(|p| p.y == y) {
                    queue!(self.writer, MoveToColumn(x))?;
                } else {
                    queue!(self.writer, MoveTo(x, y))?;
                }
            }
            if cell.modifier != modifier {
                queue_modifier_diff(&mut self.writer, modifier, cell.modifier)?;
                modifier = cell.modifier;
            }
            if cell.fg != fg || cell.bg != bg {
                queue!(
                    self.writer,
                    SetColors(Colors::new(cell.fg.into(), cell.bg.into()))
                )?;
                fg = cell.fg;
                bg = cell.bg;
            }
            if cell.underline_color != underline_color {
                queue!(self.writer, SetUnderlineColor(cell.underline_color.into()))?;
                underline_color = cell.underline_color;
            }
            let symbol = cell.symbol();
            queue!(self.writer, Print(symbol))?;
            last_pos = Some(Position { x, y });
            if symbol.len() != 1 {
                // VS16 widths vary; position the next cell explicitly.
                let last_column = if symbol.contains('\u{fe0f}') {
                    None
                } else {
                    match UnicodeWidthStr::width(symbol) {
                        0 => x.checked_sub(1),
                        width => u16::try_from(width - 1).ok().and_then(|w| x.checked_add(w)),
                    }
                };
                last_pos = last_column
                    .filter(|&x| x < u16::MAX)
                    .map(|x| Position { x, y });
            }
        }
        queue!(
            self.writer,
            SetForegroundColor(CColor::Reset),
            SetBackgroundColor(CColor::Reset),
            SetUnderlineColor(CColor::Reset),
            SetAttribute(CAttribute::Reset)
        )
    }

    fn append_lines(&mut self, n: u16) -> io::Result<()> {
        CrosstermBackend::new(&mut self.writer).append_lines(n)
    }

    fn hide_cursor(&mut self) -> io::Result<()> {
        CrosstermBackend::new(&mut self.writer).hide_cursor()
    }

    fn show_cursor(&mut self) -> io::Result<()> {
        CrosstermBackend::new(&mut self.writer).show_cursor()
    }

    fn get_cursor_position(&mut self) -> io::Result<Position> {
        CrosstermBackend::new(&mut self.writer).get_cursor_position()
    }

    fn set_cursor_position<P: Into<Position>>(&mut self, position: P) -> io::Result<()> {
        CrosstermBackend::new(&mut self.writer).set_cursor_position(position)
    }

    fn clear(&mut self) -> io::Result<()> {
        CrosstermBackend::new(&mut self.writer).clear()
    }

    fn clear_region(&mut self, clear_type: ClearType) -> io::Result<()> {
        CrosstermBackend::new(&mut self.writer).clear_region(clear_type)
    }

    fn size(&self) -> io::Result<Size> {
        let (width, height) = crossterm::terminal::size()?;
        Ok(Size::new(width, height))
    }

    fn window_size(&mut self) -> io::Result<WindowSize> {
        CrosstermBackend::new(&mut self.writer).window_size()
    }

    fn flush(&mut self) -> io::Result<()> {
        self.writer.flush()
    }
}

fn queue_modifier_diff(mut w: impl Write, from: Modifier, to: Modifier) -> io::Result<()> {
    let removed = from - to;
    if removed.contains(Modifier::REVERSED) {
        queue!(w, SetAttribute(CAttribute::NoReverse))?;
    }
    if removed.contains(Modifier::BOLD) {
        queue!(w, SetAttribute(CAttribute::NormalIntensity))?;
        if to.contains(Modifier::DIM) {
            queue!(w, SetAttribute(CAttribute::Dim))?;
        }
    }
    if removed.contains(Modifier::ITALIC) {
        queue!(w, SetAttribute(CAttribute::NoItalic))?;
    }
    if removed.contains(Modifier::UNDERLINED) {
        queue!(w, SetAttribute(CAttribute::NoUnderline))?;
    }
    if removed.contains(Modifier::DIM) {
        queue!(w, SetAttribute(CAttribute::NormalIntensity))?;
    }
    if removed.contains(Modifier::CROSSED_OUT) {
        queue!(w, SetAttribute(CAttribute::NotCrossedOut))?;
    }
    if removed.contains(Modifier::SLOW_BLINK) || removed.contains(Modifier::RAPID_BLINK) {
        queue!(w, SetAttribute(CAttribute::NoBlink))?;
    }

    let added = to - from;
    if added.contains(Modifier::REVERSED) {
        queue!(w, SetAttribute(CAttribute::Reverse))?;
    }
    if added.contains(Modifier::BOLD) {
        queue!(w, SetAttribute(CAttribute::Bold))?;
    }
    if added.contains(Modifier::ITALIC) {
        queue!(w, SetAttribute(CAttribute::Italic))?;
    }
    if added.contains(Modifier::UNDERLINED) {
        queue!(w, SetAttribute(CAttribute::Underlined))?;
    }
    if added.contains(Modifier::DIM) {
        queue!(w, SetAttribute(CAttribute::Dim))?;
    }
    if added.contains(Modifier::CROSSED_OUT) {
        queue!(w, SetAttribute(CAttribute::CrossedOut))?;
    }
    if added.contains(Modifier::SLOW_BLINK) {
        queue!(w, SetAttribute(CAttribute::SlowBlink))?;
    }
    if added.contains(Modifier::RAPID_BLINK) {
        queue!(w, SetAttribute(CAttribute::RapidBlink))?;
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crossterm::cursor::MoveTo;
    use ratatui::style::{Color, Modifier, Style};

    fn output(cells: &[(u16, u16, &Cell)], optimized: bool) -> String {
        let mut bytes = Vec::new();
        if optimized {
            TerminalBackend::new(&mut bytes)
                .draw(cells.iter().copied())
                .unwrap();
        } else {
            CrosstermBackend::new(&mut bytes)
                .draw(cells.iter().copied())
                .unwrap();
        }
        String::from_utf8(bytes).unwrap()
    }

    #[test]
    fn wide_glyphs_keep_identical_styles_with_only_redundant_moves_removed() {
        let styles = [
            Style::default(),
            Style::default().fg(Color::Red).bg(Color::Blue),
            Style::default().add_modifier(Modifier::BOLD | Modifier::ITALIC),
            Style::default()
                .underline_color(Color::Green)
                .add_modifier(Modifier::UNDERLINED),
            Style::default()
                .fg(Color::Rgb(12, 34, 56))
                .add_modifier(Modifier::REVERSED),
        ];
        for glyph in ["界", "🙂", "👩\u{200d}💻"] {
            for style in styles {
                let mut wide = Cell::new(glyph);
                wide.set_style(style);
                let mut next = Cell::new("e\u{301}");
                next.set_style(style);
                let mut changed = Cell::new("z");
                changed.set_fg(Color::Yellow);
                let cells = [(0, 0, &wide), (2, 0, &next), (3, 0, &changed)];
                let expected = output(&cells, false).replace(&MoveTo(2, 0).to_string(), "");
                assert_eq!(output(&cells, true), expected, "{glyph:?}, {style:?}");
            }
        }
    }

    #[test]
    fn gaps_row_changes_and_overlapping_updates_still_reposition() {
        let wide = Cell::new("界");
        let next = Cell::new("x");
        for (x, y) in [(3, 0), (0, 1), (1, 0)] {
            let actual = output(&[(0, 0, &wide), (x, y, &next)], true);
            let movement = if y == 0 {
                MoveToColumn(x).to_string()
            } else {
                MoveTo(x, y).to_string()
            };
            assert!(actual.contains(&movement), "{actual:?}");
        }
        let moved = output(&[(8, 0, &wide), (0, 0, &next)], true);
        assert!(moved.contains(&MoveToColumn(0).to_string()));
    }

    #[test]
    fn known_rows_use_horizontal_moves_and_uncertain_positions_use_full_moves() {
        for (symbol, known) in [
            ("a", true),
            ("e\u{301}", true),
            ("\u{301}", false),
            ("❤\u{fe0f}", false),
        ] {
            let cell = Cell::new(symbol);
            let next = Cell::new("x");
            let cells = [(0, 0, &cell), (2, 0, &next)];
            let expected = output(&cells, false);
            let expected = if known {
                expected.replace(&MoveTo(2, 0).to_string(), &MoveToColumn(2).to_string())
            } else {
                expected
            };
            assert_eq!(output(&cells, true), expected);
        }
        let wide = Cell::new("界界");
        let cells = [(u16::MAX - 1, 0, &wide)];
        assert_eq!(output(&cells, true), output(&cells, false));
    }

    #[test]
    fn horizontal_positioning_is_shorter_at_gaps_and_safe_when_backtracking() {
        let a = Cell::new("a");
        let b = Cell::new("b");
        for (row, from, to) in [
            (0, 0, 2),
            (9, 5, 20),
            (99, 10, 100),
            (12, 79, 0),
            (u16::MAX - 1, u16::MAX - 4, u16::MAX - 1),
        ] {
            let cells = [(from, row, &a), (to, row, &b)];
            let old = output(&cells, false);
            let new = output(&cells, true);
            assert_eq!(
                new,
                old.replace(&MoveTo(to, row).to_string(), &MoveToColumn(to).to_string())
            );
            assert!(new.len() < old.len());
        }
        let contiguous = [(0, 9, &a), (1, 9, &b)];
        assert_eq!(output(&contiguous, true), output(&contiguous, false));
    }

    #[test]
    fn horizontal_positioning_errors_are_propagated() {
        struct FailsOnColumnMove;
        impl Write for FailsOnColumnMove {
            fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
                if bytes.contains(&b'G') {
                    Err(io::Error::other("column move failed"))
                } else {
                    Ok(bytes.len())
                }
            }
            fn flush(&mut self) -> io::Result<()> {
                Ok(())
            }
        }
        let cell = Cell::new("a");
        let error = TerminalBackend::new(FailsOnColumnMove)
            .draw([(0, 0, &cell), (5, 0, &cell)].into_iter())
            .unwrap_err();
        assert_eq!(error.to_string(), "column move failed");
    }

    #[test]
    fn cursor_tracking_is_reset_between_frames() {
        let wide = Cell::new("界");
        let next = Cell::new("x");
        let frames = [
            vec![(0, 0, &wide), (2, 0, &next)],
            vec![(5, 1, &next)],
            vec![(0, 2, &wide), (2, 2, &next)],
        ];
        let mut actual = Vec::new();
        let mut expected = Vec::new();
        {
            let mut new = TerminalBackend::new(&mut actual);
            let mut old = CrosstermBackend::new(&mut expected);
            for frame in frames {
                new.draw(frame.iter().copied()).unwrap();
                old.draw(frame.into_iter()).unwrap();
            }
        }
        let expected = String::from_utf8(expected)
            .unwrap()
            .replace(&MoveTo(2, 0).to_string(), "")
            .replace(&MoveTo(2, 2).to_string(), "");
        assert_eq!(String::from_utf8(actual).unwrap(), expected);
    }

    #[test]
    fn terminal_write_errors_are_propagated() {
        struct FailingWriter;
        impl Write for FailingWriter {
            fn write(&mut self, _: &[u8]) -> io::Result<usize> {
                Err(io::Error::other("terminal failed"))
            }
            fn flush(&mut self) -> io::Result<()> {
                Err(io::Error::other("flush failed"))
            }
        }
        let mut backend = TerminalBackend::new(FailingWriter);
        let wide = Cell::new("界");
        assert!(backend.draw([(0, 0, &wide)].into_iter()).is_err());
        assert!(backend.flush().is_err());
        assert!(backend.clear().is_err());
    }

    #[test]
    fn style_transitions_match_the_existing_serializer() {
        let mut modifiers = vec![Modifier::empty(), Modifier::all()];
        modifiers.extend(Modifier::all().iter());
        modifiers.extend([
            Modifier::BOLD | Modifier::DIM,
            Modifier::SLOW_BLINK | Modifier::RAPID_BLINK,
        ]);
        for &from in &modifiers {
            for &to in &modifiers {
                let mut a = Cell::new("a");
                a.set_style(
                    Style::default()
                        .fg(Color::Red)
                        .bg(Color::Blue)
                        .add_modifier(from),
                );
                let mut b = Cell::new("b");
                b.set_style(
                    Style::default()
                        .fg(Color::Indexed(42))
                        .underline_color(Color::Rgb(1, 2, 3))
                        .add_modifier(to),
                );
                let cells = [(0, 0, &a), (1, 0, &b)];
                assert_eq!(
                    output(&cells, true),
                    output(&cells, false),
                    "{from:?} -> {to:?}"
                );
            }
        }
        assert_eq!(output(&[], true), output(&[], false));
    }
}
