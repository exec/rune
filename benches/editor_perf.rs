use criterion::{black_box, criterion_group, criterion_main, BenchmarkId, Criterion, Throughput};
use rune::editor::Editor;
use rune::fuzzy::fuzzy_filter;
use rune::search::SearchState;

fn make_doc(lines: usize, line_text: &str) -> String {
    let mut s = String::with_capacity(lines * (line_text.len() + 1));
    for _ in 0..lines {
        s.push_str(line_text);
        s.push('\n');
    }
    s
}

fn editor_with(content: &str) -> Editor {
    let mut ed = Editor::new_for_test();
    ed.rope = ropey::Rope::from_str(content);
    ed
}

fn bench_perform_replace(c: &mut Criterion) {
    let mut group = c.benchmark_group("perform_replace");
    for &lines in &[1_000usize, 10_000, 100_000] {
        let doc = make_doc(lines, "the quick brown fox jumps over the lazy dog");
        group.throughput(Throughput::Bytes(doc.len() as u64));
        group.bench_with_input(BenchmarkId::from_parameter(lines), &doc, |b, doc| {
            b.iter_batched(
                || editor_with(doc),
                |mut ed| {
                    let n = ed.perform_replace(black_box("fox"), black_box("cat"));
                    black_box(n);
                },
                criterion::BatchSize::LargeInput,
            );
        });
    }
    group.finish();
}

fn bench_perform_replace_interactive(c: &mut Criterion) {
    let mut group = c.benchmark_group("perform_replace_interactive");
    let doc = make_doc(10_000, "alpha beta gamma delta epsilon");
    group.throughput(Throughput::Bytes(doc.len() as u64));
    group.bench_function("10k_lines_single", |b| {
        b.iter_batched(
            || editor_with(&doc),
            |mut ed| {
                ed.perform_replace_interactive(black_box("gamma"), black_box("GAMMA"));
            },
            criterion::BatchSize::LargeInput,
        );
    });
    group.finish();
}

fn bench_toggle_hex_view(c: &mut Criterion) {
    let mut group = c.benchmark_group("toggle_hex_view");
    for &lines in &[1_000usize, 10_000, 100_000] {
        let doc = make_doc(lines, "0123456789abcdef0123456789abcdef");
        group.throughput(Throughput::Bytes(doc.len() as u64));
        group.bench_with_input(BenchmarkId::from_parameter(lines), &doc, |b, doc| {
            b.iter_batched(
                || editor_with(doc),
                |mut ed| {
                    ed.toggle_hex_view();
                    black_box(&ed);
                },
                criterion::BatchSize::LargeInput,
            );
        });
    }
    group.finish();
}

fn bench_word_complete_first(c: &mut Criterion) {
    let mut group = c.benchmark_group("word_complete_first_scan");
    let words: Vec<String> = (0..5_000)
        .map(|i| format!("identifier_{:04} variable_{:04} function_{:04}", i, i, i))
        .collect();
    let doc = words.join("\n") + "\n";
    group.throughput(Throughput::Bytes(doc.len() as u64));
    group.bench_function("5k_lines_15k_words", |b| {
        b.iter_batched(
            || {
                let mut ed = editor_with(&doc);
                let last = ed.rope.len_lines().saturating_sub(1);
                ed.rope.insert(ed.rope.line_to_char(last), "iden");
                ed.viewport.cursor_pos = (last, 4);
                ed.reset_word_complete();
                ed
            },
            |mut ed| {
                ed.word_complete();
                black_box(&ed);
            },
            criterion::BatchSize::LargeInput,
        );
    });
    group.finish();
}

fn bench_word_complete_cycle(c: &mut Criterion) {
    let mut group = c.benchmark_group("word_complete_cycle");
    let words: Vec<String> = (0..5_000)
        .map(|i| format!("identifier_{:04} variable_{:04} function_{:04}", i, i, i))
        .collect();
    let doc = words.join("\n") + "\n";
    group.bench_function("5k_lines_repeat_alt_backslash", |b| {
        b.iter_batched(
            || {
                let mut ed = editor_with(&doc);
                let last = ed.rope.len_lines().saturating_sub(1);
                ed.rope.insert(ed.rope.line_to_char(last), "iden");
                ed.viewport.cursor_pos = (last, 4);
                ed.reset_word_complete();
                ed.word_complete();
                ed
            },
            |mut ed| {
                for _ in 0..10 {
                    ed.word_complete();
                }
                black_box(&ed);
            },
            criterion::BatchSize::LargeInput,
        );
    });
    group.finish();
}

fn bench_search_literal(c: &mut Criterion) {
    let mut group = c.benchmark_group("search_literal");
    for &lines in &[1_000usize, 10_000, 50_000] {
        let doc = make_doc(lines, "the quick brown fox jumps over the lazy dog");
        let rope = ropey::Rope::from_str(&doc);
        group.throughput(Throughput::Bytes(doc.len() as u64));
        group.bench_with_input(BenchmarkId::from_parameter(lines), &rope, |b, rope| {
            b.iter(|| {
                let mut state = SearchState::default();
                state.search_buffer = "fox".to_string();
                let m = state.find_all_matches(black_box(rope));
                black_box(m);
            });
        });
    }
    group.finish();
}

fn bench_search_regex_recompile_per_keystroke(c: &mut Criterion) {
    let mut group = c.benchmark_group("search_regex_per_keystroke");
    let doc = make_doc(10_000, "user_42 admin_99 guest_12 root_01 service_77");
    let rope = ropey::Rope::from_str(&doc);
    let pattern = r"\w+_\d{2,}";
    group.throughput(Throughput::Bytes(doc.len() as u64));
    group.bench_function("compile_and_match_each_call", |b| {
        b.iter(|| {
            let mut state = SearchState::default();
            state.use_regex = true;
            state.search_buffer = pattern.to_string();
            let m = state.find_all_matches(black_box(&rope));
            black_box(m);
        });
    });
    group.finish();
}

fn bench_fuzzy_filter(c: &mut Criterion) {
    let mut group = c.benchmark_group("fuzzy_filter");
    for &n in &[10usize, 50, 200] {
        let candidates: Vec<(usize, String)> = (0..n)
            .map(|i| (i, format!("src/module_{}/component_{}.rs", i % 20, i)))
            .collect();
        group.throughput(Throughput::Elements(n as u64));
        group.bench_with_input(BenchmarkId::from_parameter(n), &candidates, |b, cands| {
            b.iter(|| {
                let r = fuzzy_filter(black_box("mdcmp"), black_box(cands));
                black_box(r);
            });
        });
    }
    group.finish();
}

fn bench_cursor_navigation(c: &mut Criterion) {
    let mut group = c.benchmark_group("cursor_navigation");
    let doc = make_doc(10_000, "fn example() { let x = 42; }");
    group.bench_function("right_500_in_long_line", |b| {
        b.iter_batched(
            || {
                let mut ed = editor_with(&doc);
                let long: String = "x".repeat(500);
                ed.rope.insert(0, &long);
                ed.viewport.cursor_pos = (0, 0);
                ed
            },
            |mut ed| {
                for _ in 0..500 {
                    ed.move_cursor_right();
                }
                black_box(&ed);
            },
            criterion::BatchSize::LargeInput,
        );
    });
    group.bench_function("down_500_across_lines", |b| {
        b.iter_batched(
            || {
                let mut ed = editor_with(&doc);
                ed.viewport.cursor_pos = (0, 0);
                ed
            },
            |mut ed| {
                for _ in 0..500 {
                    ed.move_cursor_down();
                }
                black_box(&ed);
            },
            criterion::BatchSize::LargeInput,
        );
    });
    group.finish();
}

fn bench_insert_char_typing(c: &mut Criterion) {
    let mut group = c.benchmark_group("insert_char_typing");
    let doc = make_doc(10_000, "fn example() { let x = 42; }");
    group.bench_function("100_chars_into_10k_line_doc", |b| {
        b.iter_batched(
            || editor_with(&doc),
            |mut ed| {
                ed.viewport.cursor_pos = (5_000, 0);
                for c in "the quick brown fox jumps over lazy dogs and runs fast across the meadow at noon today okay".chars() {
                    ed.insert_char(c);
                }
                black_box(&ed);
            },
            criterion::BatchSize::LargeInput,
        );
    });
    group.finish();
}

criterion_group!(
    benches,
    bench_perform_replace,
    bench_perform_replace_interactive,
    bench_toggle_hex_view,
    bench_word_complete_first,
    bench_word_complete_cycle,
    bench_search_literal,
    bench_search_regex_recompile_per_keystroke,
    bench_fuzzy_filter,
    bench_cursor_navigation,
    bench_insert_char_typing,
);
criterion_main!(benches);
