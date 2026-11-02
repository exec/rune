use ratatui::style::{Color, Style};
pub use rune_core::syntax::{HighlightKind, HighlightTheme, TokenHighlighter};

pub trait HighlightStyle: Copy + Default + PartialEq {
    fn from_kind(kind: HighlightKind) -> Self;
}
impl HighlightStyle for HighlightKind {
    #[inline]
    fn from_kind(kind: HighlightKind) -> Self {
        kind
    }
}
impl HighlightStyle for Style {
    #[inline]
    fn from_kind(kind: HighlightKind) -> Self {
        let color = match kind {
            HighlightKind::Plain => return Self::default(),
            HighlightKind::Keyword => Color::Magenta,
            HighlightKind::Control => Color::Yellow,
            HighlightKind::Literal => Color::Cyan,
            HighlightKind::Type => Color::Blue,
            HighlightKind::String => Color::Green,
            HighlightKind::Comment => Color::DarkGray,
        };
        Self::default().fg(color)
    }
}

/// Compile-time adapter; cached spans retain their native style type.
pub struct Theme<S>(std::marker::PhantomData<S>);
impl<S: HighlightStyle> HighlightTheme for Theme<S> {
    type Style = S;
    #[inline]
    fn from_kind(kind: HighlightKind) -> S {
        S::from_kind(kind)
    }
}
pub type Highlighter<S> = rune_core::syntax::Highlighter<Theme<S>>;
pub type SyntaxHighlighter = Highlighter<Style>;
pub type HighlightedLine<S = Style> = rune_core::syntax::HighlightedLine<S>;

#[cfg(test)]
mod tests {
    use super::*;
    use std::rc::Rc;

    #[test]
    fn terminal_adapter_preserves_native_colors_and_cached_spans() {
        let mut h = SyntaxHighlighter::new();
        h.set_syntax(Some("Rust"));
        let text = "fn if true String \"café\" // tail";
        let spans = h.highlight_line(0, text);
        let colored: Vec<_> = spans
            .iter()
            .filter_map(|(style, text)| style.fg.map(|fg| (text.as_str(), fg)))
            .collect();
        assert_eq!(
            colored,
            [
                ("fn", Color::Magenta),
                ("if", Color::Yellow),
                ("true", Color::Cyan),
                ("String", Color::Blue),
                ("\"café\"", Color::Green),
                ("// tail", Color::DarkGray)
            ]
        );
        assert!(Rc::ptr_eq(&spans, &h.highlight_line(0, text)));
        h.set_syntax(None);
        assert_eq!(
            *h.highlight_line(0, text),
            [(Style::default(), text.into())]
        );
    }
}
