use rune::{search::SearchState, Document, Selection};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut document = Document::new();
    document.insert_text("Hello, 界!\nquartz\n")?;
    document.set_selection(Selection {
        anchor: 7,
        caret: 8,
    })?;
    document.insert_text("Etch")?;
    let mut search = SearchState::default();
    search.search_buffer = "quartz".into();
    assert_eq!(search.find_all_match_spans(document.text()), [(1, 0, 6)]);
    assert!(document.undo());
    assert_eq!(document.text().line(0).to_string(), "Hello, 界!\n");
    println!(
        "Headless document: {} characters, revision {}",
        document.text().len_chars(),
        document.revision()
    );
    Ok(())
}
