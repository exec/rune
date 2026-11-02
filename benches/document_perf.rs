use criterion::{black_box, criterion_group, criterion_main, BatchSize, Criterion};
use rune::{Document, Selection};

fn document(text: &str) -> Document {
    let mut doc = Document::new();
    doc.insert_text(text).unwrap();
    doc
}

fn bench_document(c: &mut Criterion) {
    let mut group = c.benchmark_group("document_api");
    let text = "record alpha beta gamma\n".repeat(1000);
    group.bench_function("insert_one", |b| {
        b.iter_batched(
            || document(&text),
            |mut doc| {
                doc.insert_text(black_box("x")).unwrap();
                black_box(doc)
            },
            BatchSize::SmallInput,
        )
    });
    let line = "a".repeat(1_000_000);
    group.bench_function("replace_in_million_character_line", |b| {
        b.iter_batched(
            || document(&line),
            |mut doc| {
                doc.replace_range(500_000..500_008, black_box("é界🙂"))
                    .unwrap();
                black_box(doc)
            },
            BatchSize::SmallInput,
        )
    });
    group.bench_function("undo_redo_million_character_line", |b| {
        b.iter_batched(
            || {
                let mut doc = document(&line);
                doc.replace_range(500_000..500_008, "é界🙂").unwrap();
                doc
            },
            |mut doc| {
                assert!(doc.undo());
                assert!(doc.redo());
                black_box(doc)
            },
            BatchSize::SmallInput,
        )
    });
    group.finish();
}

fn bench_document_commands(c: &mut Criterion) {
    let mut group = c.benchmark_group("document_commands");
    let lines = "    café 界🙂\n".repeat(100_000);
    for command in [
        "indent_100k_lines",
        "unindent_100k_lines",
        "comment_100k_lines",
    ] {
        group.bench_function(command, |b| {
            b.iter_batched(
                || {
                    let mut doc = document(&lines);
                    doc.set_selection(Selection {
                        anchor: doc.text().len_chars(),
                        caret: 0,
                    })
                    .unwrap();
                    doc
                },
                |mut doc| {
                    match command {
                        "indent_100k_lines" => assert!(doc.indent_lines(4, false)),
                        "unindent_100k_lines" => assert!(doc.indent_lines(4, true)),
                        _ => assert!(doc.toggle_line_comment()),
                    }
                    black_box(doc)
                },
                BatchSize::LargeInput,
            )
        });
    }
    let long_line = format!("    {}", "x".repeat(1_000_000));
    group.bench_function("autoindent_in_million_character_line", |b| {
        b.iter_batched(
            || {
                let mut doc = document(&long_line);
                doc.set_selection(Selection {
                    anchor: 500_000,
                    caret: 500_000,
                })
                .unwrap();
                doc
            },
            |mut doc| {
                doc.insert_newline(true);
                black_box(doc)
            },
            BatchSize::LargeInput,
        )
    });
    group.finish();
}

criterion_group!(benches, bench_document, bench_document_commands);
criterion_main!(benches);
