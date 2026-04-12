use criterion::{black_box, criterion_group, criterion_main, BenchmarkId, Criterion, Throughput};
use ratatui::{backend::TestBackend, Terminal};
use rune::editor::Editor;
use rune::fuzzy::fuzzy_filter;
use rune::search::SearchState;
use rune::syntax::SyntaxHighlighter;
use rune::tabs::TabManager;
use rune::ui::draw_ui;
use std::io::Write;

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

// ---- Second-pass (perf-deep) benches ----

fn rust_like_line(i: usize) -> String {
    format!(
        "fn item_{}(x: i32, y: &str) -> Option<String> {{ let v = vec![1, 2, 3]; Some(format!(\"{{}}-{{}}\", x, y)) }}",
        i
    )
}

fn make_rust_doc(lines: usize) -> String {
    let mut s = String::with_capacity(lines * 128);
    for i in 0..lines {
        s.push_str(&rust_like_line(i));
        s.push('\n');
    }
    s
}

fn bench_load_file(c: &mut Criterion) {
    let mut group = c.benchmark_group("load_file");
    for &mb in &[1usize, 10, 50] {
        let body = "x".repeat(mb * 1_000_000);
        let mut tmp = tempfile::NamedTempFile::new().expect("tempfile");
        tmp.write_all(body.as_bytes()).expect("write");
        let path = tmp.path().to_path_buf();
        let _guard = tmp; // keep file alive across iterations
        group.throughput(Throughput::Bytes((mb * 1_000_000) as u64));
        group.bench_with_input(
            BenchmarkId::from_parameter(format!("{}mb_ascii", mb)),
            &path,
            |b, path| {
                b.iter_batched(
                    Editor::new_for_test,
                    |mut ed| {
                        ed.load_file(black_box(path.clone())).expect("load");
                        black_box(&ed);
                    },
                    criterion::BatchSize::LargeInput,
                );
            },
        );
    }
    group.finish();
}

fn bench_syntax_highlight(c: &mut Criterion) {
    let mut group = c.benchmark_group("syntax_highlight");
    let lines: Vec<String> = (0..50).map(rust_like_line).collect();

    group.bench_function("50_rust_lines_cold", |b| {
        b.iter_batched(
            || {
                let mut h = SyntaxHighlighter::new();
                h.set_syntax(Some("Rust"));
                h
            },
            |mut h| {
                for (i, line) in lines.iter().enumerate() {
                    let _ = h.highlight_line(i, black_box(line));
                }
                black_box(&h);
            },
            criterion::BatchSize::SmallInput,
        );
    });

    group.bench_function("50_rust_lines_warm", |b| {
        b.iter_batched(
            || {
                let mut h = SyntaxHighlighter::new();
                h.set_syntax(Some("Rust"));
                for (i, line) in lines.iter().enumerate() {
                    let _ = h.highlight_line(i, line);
                }
                h
            },
            |mut h| {
                for (i, line) in lines.iter().enumerate() {
                    let _ = h.highlight_line(i, black_box(line));
                }
                black_box(&h);
            },
            criterion::BatchSize::SmallInput,
        );
    });

    group.bench_function("invalidate_then_rehighlight_50_lines", |b| {
        b.iter_batched(
            || {
                let mut h = SyntaxHighlighter::new();
                h.set_syntax(Some("Rust"));
                for (i, line) in lines.iter().enumerate() {
                    let _ = h.highlight_line(i, line);
                }
                h
            },
            |mut h| {
                // Simulate a single-char edit at line 25; currently callers
                // pass 0 which nukes the whole cache.
                h.invalidate_cache_from_line(0);
                for (i, line) in lines.iter().enumerate() {
                    let _ = h.highlight_line(i, black_box(line));
                }
                black_box(&h);
            },
            criterion::BatchSize::SmallInput,
        );
    });

    group.finish();
}

fn setup_render_tabs(lines: usize) -> TabManager {
    let mut tabs = TabManager::new_for_test();
    let doc = make_rust_doc(lines);
    let editor = tabs.active_editor_mut();
    editor.rope = ropey::Rope::from_str(&doc);
    editor.viewport.cursor_pos = (lines / 2, 0);
    tabs
}

fn bench_render_frame(c: &mut Criterion) {
    let mut group = c.benchmark_group("render_frame");
    group.bench_function("10k_line_doc_single_draw", |b| {
        b.iter_batched(
            || {
                let tabs = setup_render_tabs(10_000);
                let backend = TestBackend::new(120, 40);
                let terminal = Terminal::new(backend).expect("terminal");
                (tabs, terminal)
            },
            |(mut tabs, mut terminal)| {
                terminal.draw(|f| draw_ui(f, &mut tabs)).expect("draw");
                black_box(&tabs);
            },
            criterion::BatchSize::SmallInput,
        );
    });

    group.bench_function("10k_line_doc_cursor_move_100_frames", |b| {
        b.iter_batched(
            || {
                let tabs = setup_render_tabs(10_000);
                let backend = TestBackend::new(120, 40);
                let terminal = Terminal::new(backend).expect("terminal");
                (tabs, terminal)
            },
            |(mut tabs, mut terminal)| {
                for _ in 0..100 {
                    tabs.active_editor_mut().move_cursor_right();
                    terminal.draw(|f| draw_ui(f, &mut tabs)).expect("draw");
                }
                black_box(&tabs);
            },
            criterion::BatchSize::SmallInput,
        );
    });

    group.bench_function("10k_line_doc_edit_then_draw_100_frames", |b| {
        b.iter_batched(
            || {
                let tabs = setup_render_tabs(10_000);
                let backend = TestBackend::new(120, 40);
                let terminal = Terminal::new(backend).expect("terminal");
                (tabs, terminal)
            },
            |(mut tabs, mut terminal)| {
                for _ in 0..100 {
                    tabs.active_editor_mut().insert_char('x');
                    terminal.draw(|f| draw_ui(f, &mut tabs)).expect("draw");
                }
                black_box(&tabs);
            },
            criterion::BatchSize::SmallInput,
        );
    });

    group.finish();
}

fn bench_search_many_matches(c: &mut Criterion) {
    let mut group = c.benchmark_group("search_many_matches");
    // 50k lines of common text — searching for " " produces ~400k matches.
    let doc = make_doc(50_000, "the quick brown fox jumps over the lazy dog");
    let rope = ropey::Rope::from_str(&doc);
    group.throughput(Throughput::Bytes(doc.len() as u64));
    group.bench_function("space_in_50k_line_doc", |b| {
        b.iter(|| {
            let mut state = SearchState::default();
            state.search_buffer = " ".to_string();
            let m = state.find_all_matches(black_box(&rope));
            black_box(m);
        });
    });
    group.finish();
}

fn bench_tabmanager_cold_start(c: &mut Criterion) {
    c.bench_function("tabmanager_new_for_test_cold", |b| {
        b.iter(|| {
            let t = TabManager::new_for_test();
            black_box(t);
        });
    });
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
    bench_load_file,
    bench_syntax_highlight,
    bench_render_frame,
    bench_search_many_matches,
    bench_tabmanager_cold_start,
);
criterion_main!(benches);
