use rune::syntax::{HighlightKind, TokenHighlighter};
use std::rc::Rc;

#[test]
fn document_changes_invalidate_only_affected_neutral_lines() {
    let mut doc = rune::Document::new();
    doc.insert_text("fn alpha() {}\nfn beta() {}\n").unwrap();
    doc.take_changed_lines();
    let mut highlighter = TokenHighlighter::new();
    highlighter.set_syntax(Some("Rust"));
    let first = highlighter.highlight_line(0, "fn alpha() {}\n");
    let second = highlighter.highlight_line(1, "fn beta() {}\n");
    let start = doc.text().line_to_char(1) + 3;
    doc.replace_range(start..start, "x").unwrap();
    let changed = doc.take_changed_lines().unwrap();
    highlighter.invalidate_cache_range(changed.start, changed.end);
    assert!(Rc::ptr_eq(
        &first,
        &highlighter.highlight_line(0, "fn alpha() {}\n")
    ));
    assert!(!Rc::ptr_eq(
        &second,
        &highlighter.highlight_line(1, "fn xbeta() {}\n")
    ));
}

#[test]
fn neutral_tokens_preserve_text_roles_and_cache_invalidation() {
    let mut highlighter = TokenHighlighter::new();
    highlighter.set_syntax(Some("Rust"));
    let text = "fn main() { let x: String = \"café\"; } // 界";
    let spans = highlighter.highlight_line(0, text);
    assert_eq!(
        spans.iter().map(|(_, s)| s.as_str()).collect::<String>(),
        text
    );
    assert!(spans
        .iter()
        .any(|(kind, s)| *kind == HighlightKind::Keyword && s == "fn"));
    assert!(spans
        .iter()
        .any(|(kind, s)| *kind == HighlightKind::String && s == "\"café\""));
    assert!(spans
        .iter()
        .any(|(kind, s)| *kind == HighlightKind::Comment && s == "// 界"));
    assert!(Rc::ptr_eq(&spans, &highlighter.highlight_line(0, text)));
    highlighter.invalidate_cache_from_line(0);
    assert!(!Rc::ptr_eq(&spans, &highlighter.highlight_line(0, text)));
    highlighter.set_syntax(None);
    assert_eq!(
        *highlighter.highlight_line(0, text),
        [(HighlightKind::Plain, text.into())]
    );
}

#[cfg(feature = "terminal")]
#[test]
fn neutral_and_terminal_highlighters_have_identical_spans() {
    use ratatui::style::Style;
    use rune::syntax::{HighlightStyle, SyntaxHighlighter};
    for language in ["Rust", "Python", "JavaScript", "SQL", "unknown"] {
        let mut neutral = TokenHighlighter::new();
        let mut terminal = SyntaxHighlighter::new();
        neutral.set_syntax(Some(language));
        terminal.set_syntax(Some(language));
        for (line, text) in [
            "fn String true if \'界\'",
            "/* café */ let a = 42;",
            "# test",
            "-- x",
            "",
        ]
        .iter()
        .enumerate()
        {
            let expected: Vec<_> = neutral
                .highlight_line(line, text)
                .iter()
                .map(|(kind, s)| (Style::from_kind(*kind), s.clone()))
                .collect();
            assert_eq!(*terminal.highlight_line(line, text), expected);
        }
    }
}
