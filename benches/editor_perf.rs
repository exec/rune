use criterion::{black_box, criterion_group, criterion_main, BenchmarkId, Criterion, Throughput};
use ratatui::{backend::TestBackend, Terminal};
use ropey::Rope;
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
                    assert_eq!(n, lines);
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
                assert!(!ed.word_complete_candidates.is_empty());
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
                assert!(!ed.word_complete_candidates.is_empty());
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
    for (name, body) in [
        ("1mb_ascii", "x".repeat(1_000_000)),
        ("10mb_ascii", "x".repeat(10_000_000)),
        ("50mb_ascii", "x".repeat(50_000_000)),
        (
            "records_100k",
            make_doc(
                100_000,
                "record 00000000  alpha beta gamma delta epsilon 0123456789",
            ),
        ),
        (
            "unicode_crlf",
            "let message = \"界e\u{301}🙂\"; // comment\r\n".repeat(200_000),
        ),
        (
            "small_ascii",
            make_doc(
                1_000,
                "record 00000000  alpha beta gamma delta epsilon 0123456789",
            ),
        ),
    ] {
        let mut tmp = tempfile::NamedTempFile::new().expect("tempfile");
        tmp.write_all(body.as_bytes()).expect("write");
        let path = tmp.path().to_path_buf();
        let _guard = tmp; // keep file alive across iterations
        group.throughput(Throughput::Bytes(body.len() as u64));
        group.bench_with_input(BenchmarkId::from_parameter(name), &path, |b, path| {
            b.iter_batched(
                Editor::new_for_test,
                |mut ed| {
                    ed.load_file(black_box(path.clone())).expect("load");
                    black_box(&ed);
                },
                criterion::BatchSize::LargeInput,
            );
        });
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

fn bench_efficiency_edges(c: &mut Criterion) {
    let mut group = c.benchmark_group("efficiency_edges");
    let long = "x".repeat(100_000);
    for name in ["left_100_at_end", "backspace_100_at_end"] {
        group.bench_function(name, |b| {
            b.iter_batched(
                || {
                    let mut ed = editor_with(&long);
                    ed.viewport.cursor_pos = (0, long.len());
                    ed
                },
                |mut ed| {
                    for _ in 0..100 {
                        if name.starts_with("left") {
                            ed.move_cursor_left();
                        } else {
                            ed.delete_char();
                        }
                    }
                    black_box(ed);
                },
                criterion::BatchSize::LargeInput,
            )
        });
    }
    let rope = ropey::Rope::from_str(&long);
    group.bench_function("search_capped_long_line", |b| {
        b.iter(|| {
            let mut search = SearchState::default();
            search.search_buffer = "x".into();
            black_box(search.match_spans(&rope, 0, 10));
        })
    });
    let doc = make_doc(100_000, "short line");
    group.bench_function("wrapped_jump_to_end", |b| {
        b.iter_batched(
            || editor_with(&doc),
            |mut ed| {
                ed.goto_end();
                ed.update_viewport_for_size(40, 120, 0, true);
                black_box(ed);
            },
            criterion::BatchSize::LargeInput,
        )
    });
    group.bench_function("wrapped_100_frames", |b| {
        b.iter_batched(
            || {
                let mut tabs = setup_render_tabs(10_000);
                tabs.config.word_wrap = true;
                let terminal = Terminal::new(TestBackend::new(120, 40)).unwrap();
                (tabs, terminal)
            },
            |(mut tabs, mut terminal)| {
                for _ in 0..100 {
                    tabs.active_editor_mut().move_cursor_right();
                    terminal.draw(|f| draw_ui(f, &mut tabs)).unwrap();
                }
                black_box(tabs);
            },
            criterion::BatchSize::SmallInput,
        )
    });
    group.bench_function("fuzzy_100_frames_200_tabs", |b| {
        b.iter_batched(
            || {
                let mut tabs = TabManager::new_for_test();
                for i in 0..200 {
                    tabs.new_tab();
                    tabs.active_editor_mut().display_name = format!("src/component_{i}/module.rs");
                }
                tabs.input_mode = rune::editor::InputMode::FuzzyFinder;
                tabs.fuzzy_query = "cmp".into();
                tabs.rebuild_fuzzy_candidates();
                (tabs, Terminal::new(TestBackend::new(120, 40)).unwrap())
            },
            |(mut tabs, mut terminal)| {
                for _ in 0..100 {
                    terminal.draw(|f| draw_ui(f, &mut tabs)).unwrap();
                }
            },
            criterion::BatchSize::SmallInput,
        )
    });
    group.bench_function("resolve_names_10000_tabs", |b| {
        b.iter_batched(
            || {
                let mut tabs = TabManager::new_for_test();
                tabs.tabs = (0..10_000)
                    .map(|i| {
                        let mut editor = Editor::new_buffer();
                        editor.file_path = Some(format!("src/{i}/module.rs").into());
                        editor
                    })
                    .collect();
                tabs
            },
            |mut tabs| {
                tabs.resolve_display_names();
                black_box(tabs);
            },
            criterion::BatchSize::LargeInput,
        )
    });
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("save.txt");
    let content = "x".repeat(10_000_000);
    std::fs::write(&path, &content).unwrap();
    group.bench_function("save_10mb", |b| {
        b.iter_batched(
            || {
                let mut tabs = TabManager::new_for_test();
                tabs.active_editor_mut().load_file(path.clone()).unwrap();
                tabs
            },
            |mut tabs| {
                tabs.perform_save(path.clone()).unwrap();
                black_box(tabs);
            },
            criterion::BatchSize::LargeInput,
        )
    });
    group.finish();
}

fn bench_second_efficiency_pass(c: &mut Criterion) {
    let mut group = c.benchmark_group("efficiency_pass2");
    group.bench_function("new_tab_5000_existing", |b| {
        b.iter_batched(
            || {
                let mut tabs = TabManager::new_for_test();
                tabs.tabs = (1..=5_000)
                    .map(|i| {
                        let mut e = Editor::new_buffer();
                        e.display_name = if i == 1 {
                            "[untitled]".into()
                        } else {
                            format!("[untitled-{i}]")
                        };
                        e
                    })
                    .collect();
                tabs
            },
            |mut tabs| {
                tabs.new_tab();
                assert_eq!(tabs.active_editor().display_name, "[untitled-5001]");
                black_box(tabs);
            },
            criterion::BatchSize::LargeInput,
        )
    });
    let brackets = "(\n".to_string() + &make_doc(1_000, &"x".repeat(1_000)) + ")";
    for forward in [true, false] {
        group.bench_function(
            if forward {
                "bracket_forward_1mb"
            } else {
                "bracket_backward_1mb"
            },
            |b| {
                b.iter_batched(
                    || {
                        let mut e = editor_with(&brackets);
                        if !forward {
                            e.viewport.cursor_pos = (e.rope.len_lines() - 1, 0);
                        }
                        e
                    },
                    |mut e| {
                        e.match_bracket();
                        assert_eq!(
                            e.viewport.cursor_pos,
                            if forward {
                                (e.rope.len_lines() - 1, 0)
                            } else {
                                (0, 0)
                            }
                        );
                        black_box(e);
                    },
                    criterion::BatchSize::LargeInput,
                )
            },
        );
    }
    let words = "word ".repeat(20_000);
    for forward in [true, false] {
        group.bench_function(
            if forward {
                "word_right_100_long_line"
            } else {
                "word_left_100_long_line"
            },
            |b| {
                b.iter_batched(
                    || {
                        let mut e = editor_with(&words);
                        if !forward {
                            e.viewport.cursor_pos = (0, words.len());
                        }
                        e
                    },
                    |mut e| {
                        for _ in 0..100 {
                            if forward {
                                e.move_word_right();
                            } else {
                                e.move_word_left();
                            }
                        }
                        assert_eq!(
                            e.viewport.cursor_pos.1,
                            if forward { 500 } else { words.len() - 500 }
                        );
                        black_box(e);
                    },
                    criterion::BatchSize::LargeInput,
                )
            },
        );
    }
    group.bench_function("tab_bar_100_frames_10000_tabs", |b| {
        b.iter_batched(
            || {
                let mut tabs = TabManager::new_for_test();
                tabs.tabs = (0..10_000)
                    .map(|i| {
                        let mut e = Editor::new_buffer();
                        e.display_name = format!("module_{i}.rs");
                        e
                    })
                    .collect();
                tabs.active_tab = 9_999;
                let mut terminal = Terminal::new(TestBackend::new(120, 40)).unwrap();
                terminal.draw(|f| draw_ui(f, &mut tabs)).unwrap();
                (tabs, terminal)
            },
            |(mut tabs, mut terminal)| {
                for _ in 0..100 {
                    terminal.draw(|f| draw_ui(f, &mut tabs)).unwrap();
                }
                black_box(tabs);
            },
            criterion::BatchSize::SmallInput,
        )
    });
    let line = "prefix ".to_string() + &"x".repeat(200_000) + " needle suffix";
    for search in [true, false] {
        group.bench_function(
            if search {
                "search_highlight_20_frames_long_line"
            } else {
                "selection_highlight_20_frames_long_line"
            },
            |b| {
                b.iter_batched(
                    || {
                        let mut tabs = TabManager::new_for_test();
                        tabs.active_editor_mut().rope = Rope::from_str(&line);
                        if search {
                            assert!(tabs.active_editor_mut().perform_find("needle"));
                        } else {
                            let e = tabs.active_editor_mut();
                            e.mark_anchor = Some((0, 1));
                            e.viewport.cursor_pos = (0, line.len() - 1);
                        }
                        let mut terminal = Terminal::new(TestBackend::new(120, 40)).unwrap();
                        terminal.draw(|f| draw_ui(f, &mut tabs)).unwrap();
                        (tabs, terminal)
                    },
                    |(mut tabs, mut terminal)| {
                        for _ in 0..20 {
                            terminal.draw(|f| draw_ui(f, &mut tabs)).unwrap();
                        }
                        black_box(tabs);
                    },
                    criterion::BatchSize::SmallInput,
                )
            },
        );
    }
    group.finish();
}

fn bench_rare_efficiency(c: &mut Criterion) {
    let mut group = c.benchmark_group("rare_efficiency");
    let unicode = "界e\u{301} xyz ".repeat(40_000);
    group.bench_function("unicode_wrap_end_20_frames", |b| {
        b.iter_batched(
            || {
                let mut tabs = TabManager::new_for_test();
                tabs.config.word_wrap = true;
                let e = tabs.active_editor_mut();
                e.rope = Rope::from_str(&unicode);
                e.viewport.cursor_pos = (0, rune::editor::line_display_width(&e.rope, 0));
                let mut terminal = Terminal::new(TestBackend::new(120, 40)).unwrap();
                terminal.draw(|f| draw_ui(f, &mut tabs)).unwrap();
                (tabs, terminal)
            },
            |(mut tabs, mut terminal)| {
                for _ in 0..20 {
                    terminal.draw(|f| draw_ui(f, &mut tabs)).unwrap();
                }
                black_box(tabs);
            },
            criterion::BatchSize::SmallInput,
        )
    });
    let commented = "\u{3000}  // ".to_owned() + &"x".repeat(10_000_000);
    group.bench_function("uncomment_10mb_line", |b| {
        b.iter_batched(
            || editor_with(&commented),
            |mut e| {
                e.toggle_comment();
                assert_eq!(e.rope.len_bytes(), commented.len() - 3);
                assert!(e.rope.slice(..4).to_string().starts_with("\u{3000}  x"));
                black_box(e);
            },
            criterion::BatchSize::LargeInput,
        )
    });
    group.finish();
}

fn bench_common_efficiency(c: &mut Criterion) {
    let mut group = c.benchmark_group("common_efficiency");
    let long_lines = ("x".repeat(10_000) + "\n").repeat(501);
    group.bench_function("down_500_at_column_4", |b| {
        b.iter_batched(
            || {
                let mut e = editor_with(&long_lines);
                e.viewport.cursor_pos = (0, 4);
                e
            },
            |mut e| {
                for _ in 0..500 {
                    e.move_cursor_down();
                }
                assert_eq!(e.viewport.cursor_pos, (500, 4));
                black_box(e);
            },
            criterion::BatchSize::LargeInput,
        )
    });
    let document = "needle\n".to_owned() + &("ordinary ".repeat(250) + "\n").repeat(60);
    for search in [true, false] {
        group.bench_function(
            if search {
                "search_100_frames_one_matching_line"
            } else {
                "selection_100_frames_one_selected_line"
            },
            |b| {
                b.iter_batched(
                    || {
                        let mut tabs = TabManager::new_for_test();
                        let e = tabs.active_editor_mut();
                        e.rope = Rope::from_str(&document);
                        if search {
                            assert!(e.perform_find("needle"));
                        } else {
                            e.mark_anchor = Some((0, 0));
                            e.viewport.cursor_pos = (0, 3);
                        }
                        let mut terminal = Terminal::new(TestBackend::new(120, 40)).unwrap();
                        terminal.draw(|f| draw_ui(f, &mut tabs)).unwrap();
                        (tabs, terminal)
                    },
                    |(mut tabs, mut terminal)| {
                        for _ in 0..100 {
                            terminal.draw(|f| draw_ui(f, &mut tabs)).unwrap();
                        }
                        black_box(tabs);
                    },
                    criterion::BatchSize::SmallInput,
                )
            },
        );
    }
    group.finish();
}

fn bench_remaining_efficiency(c: &mut Criterion) {
    let mut group = c.benchmark_group("remaining_efficiency");
    let text = "a".repeat(200_000);
    group.bench_function("type_then_right_100_long_line", |b| {
        b.iter_batched(
            || {
                let mut e = editor_with(&text);
                e.viewport.cursor_pos = (0, 4);
                e
            },
            |mut e| {
                for _ in 0..100 {
                    e.insert_char('x');
                    e.move_cursor_right();
                }
                assert_eq!(e.viewport.cursor_pos, (0, 204));
                assert_eq!(e.rope.len_chars(), 200_100);
                black_box(e);
            },
            criterion::BatchSize::LargeInput,
        )
    });
    let candidates: Vec<_> = (0..2000)
        .map(|i| {
            rune::fuzzy::FuzzyCandidate::new(
                i,
                format!("module_{i}_{}", "directory_segment/".repeat(60)),
            )
        })
        .collect();
    group.bench_function("fuzzy_short_query_2000_long_names", |b| {
        b.iter(|| {
            let matches =
                rune::fuzzy::fuzzy_filter_prepared(black_box("mod"), black_box(&candidates));
            assert_eq!(matches.len(), 2000);
            black_box(matches);
        })
    });
    let comment = "// ".to_owned() + &"long comment text ".repeat(180_000);
    group.bench_function("highlight_3mb_line_comment", |b| {
        b.iter_batched(
            || {
                let mut h = SyntaxHighlighter::new();
                h.set_syntax(Some("Rust"));
                h
            },
            |mut h| {
                let spans = h.highlight_line(0, black_box(&comment));
                assert_eq!(
                    spans.iter().map(|(_, s)| s.len()).sum::<usize>(),
                    comment.len()
                );
                black_box(spans);
            },
            criterion::BatchSize::SmallInput,
        )
    });
    group.finish();
}

fn bench_fuzzy_pruning(c: &mut Criterion) {
    let mut group = c.benchmark_group("fuzzy_pruning");
    for long in [false, true] {
        let names: Vec<_> = (0..2000)
            .map(|i| {
                format!(
                    "module_{i}_{}tail.rs",
                    if long {
                        "directory_segment/".repeat(60)
                    } else {
                        "src/".into()
                    }
                )
            })
            .collect();
        let prepared: Vec<_> = names
            .iter()
            .enumerate()
            .map(|(i, s)| rune::fuzzy::FuzzyCandidate::new(i, s.clone()))
            .collect();
        for (label, query) in [
            ("early", "mod"),
            ("late", "tail"),
            ("absent", "z"),
            ("wrong_order", "tailmod"),
            ("unicode_absent", "界"),
        ] {
            group.bench_function(
                format!("{}_{}", if long { "long" } else { "short" }, label),
                |b| {
                    b.iter(|| {
                        let matches = rune::fuzzy::fuzzy_filter_prepared(
                            black_box(query),
                            black_box(&prepared),
                        );
                        assert_eq!(
                            matches.len(),
                            if ["mod", "tail"].contains(&query) {
                                2000
                            } else {
                                0
                            }
                        );
                        black_box(matches);
                    })
                },
            );
        }
        group.bench_function(
            if long {
                "prepare_long"
            } else {
                "prepare_short"
            },
            |b| {
                b.iter(|| {
                    black_box(
                        names
                            .iter()
                            .enumerate()
                            .map(|(i, s)| rune::fuzzy::FuzzyCandidate::new(i, s.clone()))
                            .collect::<Vec<_>>(),
                    )
                })
            },
        );
    }
    group.finish();
}

fn bench_search_interactions(c: &mut Criterion) {
    let mut group = c.benchmark_group("search_interactions");
    let sparse = "needle\n".to_owned() + &("x".repeat(1000) + "\n").repeat(10_000);
    group.bench_function("interactive_first_then_10mb_without_matches", |b| {
        b.iter_batched(
            || {
                let mut e = editor_with(&sparse);
                assert!(e.perform_find("needle"));
                e
            },
            |mut e| {
                assert_eq!(e.perform_replace_interactive("needle", "result"), 1);
                assert_eq!(e.rope.line(0).to_string(), "result\n");
                black_box(e);
            },
            criterion::BatchSize::LargeInput,
        )
    });
    let dense = make_doc(20_000, "needle in this line");
    group.bench_function("find_10k_results_from_document_end", |b| {
        b.iter_batched(
            || {
                let mut e = editor_with(&dense);
                e.search.search_buffer = "needle".into();
                e.search.find_all_matches(&e.rope);
                e.viewport.cursor_pos = (19_999, 0);
                e
            },
            |mut e| {
                assert!(e.perform_find("needle"));
                assert_eq!(e.search.current_match_index, Some(0));
                assert_eq!(e.viewport.cursor_pos, (0, 0));
                black_box(e);
            },
            criterion::BatchSize::LargeInput,
        )
    });
    group.finish();
}

fn bench_clipboard_efficiency(c: &mut Criterion) {
    let mut group = c.benchmark_group("clipboard_efficiency");
    let text = "x".repeat(10_000_000) + "\n";
    for cut in [false, true] {
        group.bench_function(
            if cut {
                "cut_10mb_line"
            } else {
                "copy_10mb_line"
            },
            |b| {
                b.iter_batched(
                    || {
                        let mut tabs = TabManager::new_for_test();
                        tabs.active_editor_mut().rope = Rope::from_str(&text);
                        tabs
                    },
                    |mut tabs| {
                        if cut {
                            tabs.cut();
                            assert_eq!(tabs.active_editor().rope.len_chars(), 0);
                        } else {
                            tabs.copy();
                        }
                        assert_eq!(tabs.clipboard.len(), 1);
                        assert_eq!(tabs.clipboard[0].len(), text.len());
                        black_box(tabs);
                    },
                    criterion::BatchSize::LargeInput,
                )
            },
        );
    }
    let payload = "x".repeat(10_000_000);
    for inline in [false, true] {
        group.bench_function(
            if inline {
                "paste_inline_10mb_ascii"
            } else {
                "paste_above_10mb_ascii"
            },
            |b| {
                b.iter_batched(
                    || {
                        let mut tabs = TabManager::new_for_test();
                        tabs.active_editor_mut().rope = Rope::from_str("original\n");
                        tabs.active_editor_mut().viewport.cursor_pos = (0, 3);
                        tabs.clipboard = vec![payload.clone()];
                        tabs
                    },
                    |mut tabs| {
                        if inline {
                            tabs.paste_inline();
                            assert_eq!(
                                tabs.active_editor().viewport.cursor_pos,
                                (0, payload.len() + 3)
                            );
                        } else {
                            tabs.paste();
                            assert_eq!(tabs.active_editor().viewport.cursor_pos, (0, 0));
                        }
                        assert_eq!(tabs.active_editor().rope.len_chars(), payload.len() + 9);
                        assert_eq!(tabs.clipboard[0].len(), payload.len());
                        black_box(tabs);
                    },
                    criterion::BatchSize::LargeInput,
                )
            },
        );
    }
    group.finish();
}

fn bench_syntax_efficiency(c: &mut Criterion) {
    let mut group = c.benchmark_group("syntax_efficiency");
    for (name, text) in [
        (
            "quoted_3mb",
            format!("\"{}\" let after = true;", "quoted text ".repeat(250_000)),
        ),
        (
            "block_comment_3mb",
            format!("/*{}*/ let after = true;", "comment text ".repeat(250_000)),
        ),
    ] {
        group.bench_function(name, |b| {
            b.iter_batched(
                || {
                    let mut h = SyntaxHighlighter::new();
                    h.set_syntax(Some("Rust"));
                    h
                },
                |mut h| {
                    let spans = h.highlight_line(0, &text);
                    assert_eq!(
                        spans.iter().map(|(_, s)| s.len()).sum::<usize>(),
                        text.len()
                    );
                    black_box(spans);
                },
                criterion::BatchSize::SmallInput,
            )
        });
    }
    group.bench_function("scroll_20000_short_lines", |b| {
        b.iter_batched(
            || {
                let mut h = SyntaxHighlighter::new();
                h.set_syntax(Some("Rust"));
                h
            },
            |mut h| {
                for line in 0..20000 {
                    black_box(h.highlight_line(line, "// comment"));
                }
                black_box(h);
            },
            criterion::BatchSize::SmallInput,
        )
    });
    group.finish();
}

fn bench_bulk_indentation(c: &mut Criterion) {
    let mut group = c.benchmark_group("bulk_indentation");
    for remove in [false, true] {
        let text = make_doc(
            100_000,
            if remove {
                "    short line"
            } else {
                "short line"
            },
        );
        group.bench_function(
            if remove {
                "unindent_100k_lines"
            } else {
                "indent_100k_lines"
            },
            |b| {
                b.iter_batched(
                    || {
                        let mut tabs = TabManager::new_for_test();
                        let e = tabs.active_editor_mut();
                        e.rope = Rope::from_str(&text);
                        e.mark_anchor = Some((0, 0));
                        e.viewport.cursor_pos = (99_999, 0);
                        tabs.config.tab_width = 4;
                        tabs
                    },
                    |mut tabs| {
                        if remove {
                            tabs.unindent_lines();
                        } else {
                            tabs.indent_lines();
                        }
                        assert_eq!(
                            tabs.active_editor().rope.len_bytes(),
                            if remove { 1_100_000 } else { 1_500_000 }
                        );
                        black_box(tabs);
                    },
                    criterion::BatchSize::LargeInput,
                )
            },
        );
    }
    group.finish();
}

fn bench_hex_rendering(c: &mut Criterion) {
    let mut group = c.benchmark_group("hex_rendering");
    for (name, width, moving) in [
        ("steady_100_frames", 100, false),
        ("cursor_100_frames", 100, true),
        ("narrow_100_frames", 20, true),
    ] {
        group.bench_function(name, |b| {
            b.iter_batched(
                || {
                    let state = rune::hex::HexViewState::new((0..16384).map(|i| i as u8).collect());
                    let terminal = Terminal::new(TestBackend::new(width, 40)).unwrap();
                    (state, terminal)
                },
                |(mut state, mut terminal)| {
                    for frame in 0..100 {
                        if moving {
                            state.cursor = frame;
                        }
                        terminal
                            .draw(|f| rune::hex::draw_hex_view(f, f.area(), &mut state))
                            .unwrap();
                    }
                    assert_eq!(state.raw_bytes.len(), 16384);
                    black_box(state);
                },
                criterion::BatchSize::SmallInput,
            )
        });
    }
    group.finish();
}

fn bench_typing_cache(c: &mut Criterion) {
    let mut group = c.benchmark_group("typing_cache");
    for name in [
        "insert_100_frames",
        "backspace_100_frames",
        "delete_100_frames",
        "tab_100_frames",
        "unicode_wrap_100_frames",
        "newline_100_frames",
    ] {
        group.bench_function(name, |b| {
            b.iter_batched(
                || {
                    let mut tabs = TabManager::new_for_test();
                    tabs.config.word_wrap = name == "unicode_wrap_100_frames";
                    let text = if tabs.config.word_wrap {
                        ("界e\u{301} ".repeat(25) + "\n").repeat(500)
                    } else {
                        ("fn item() { let value = true; } ".repeat(6) + "\n").repeat(500)
                    };
                    let e = tabs.active_editor_mut();
                    e.rope = Rope::from_str(&text);
                    e.syntax_name = Some("Rust".into());
                    e.highlighter.set_syntax(Some("Rust"));
                    if name == "backspace_100_frames" {
                        e.viewport.cursor_pos = (0, 100);
                    }
                    let mut terminal = Terminal::new(TestBackend::new(120, 40)).unwrap();
                    terminal.draw(|f| draw_ui(f, &mut tabs)).unwrap();
                    (tabs, terminal)
                },
                |(mut tabs, mut terminal)| {
                    for _ in 0..100 {
                        match name {
                            "backspace_100_frames" => tabs.active_editor_mut().delete_char(),
                            "delete_100_frames" => tabs.active_editor_mut().delete_char_forward(),
                            "tab_100_frames" => tabs.handle_tab_insertion(),
                            "newline_100_frames" => tabs.active_editor_mut().insert_newline(false),
                            _ => tabs.active_editor_mut().insert_char('x'),
                        }
                        terminal.draw(|f| draw_ui(f, &mut tabs)).unwrap();
                    }
                    black_box(tabs);
                },
                criterion::BatchSize::SmallInput,
            )
        });
    }
    group.finish();
}

fn bench_input_bursts(c: &mut Criterion) {
    use crossterm::event::{Event, KeyCode, KeyEvent, KeyModifiers};
    let mut group = c.benchmark_group("input_bursts");
    for name in [
        "typing_256",
        "plain_typing_256",
        "navigation_256",
        "unicode_wrap_256",
        "isolated_key",
    ] {
        group.bench_function(name, |b| {
            b.iter_batched(
                || {
                    let mut tabs = TabManager::new_for_test();
                    tabs.config.word_wrap = name == "unicode_wrap_256";
                    let text = if tabs.config.word_wrap {
                        ("界e\u{301} ".repeat(25) + "\n").repeat(500)
                    } else if name == "plain_typing_256" {
                        make_doc(
                            1_000,
                            "record 00000000  alpha beta gamma delta epsilon 0123456789",
                        )
                    } else {
                        ("fn item() { let value = true; } ".repeat(6) + "\n").repeat(500)
                    };
                    let e = tabs.active_editor_mut();
                    e.rope = Rope::from_str(&text);
                    if name != "plain_typing_256" {
                        e.highlighter.set_syntax(Some("Rust"));
                    }
                    let mut terminal = Terminal::new(TestBackend::new(120, 40)).unwrap();
                    terminal.draw(|f| draw_ui(f, &mut tabs)).unwrap();
                    (tabs, terminal)
                },
                |(mut tabs, mut terminal)| {
                    let code = if name == "navigation_256" {
                        KeyCode::Down
                    } else {
                        KeyCode::Char('x')
                    };
                    let key = KeyEvent::new(code, KeyModifiers::NONE);
                    let mut events = std::iter::repeat_n(
                        Event::Key(key),
                        if name == "isolated_key" { 1 } else { 256 },
                    );
                    let mut pending = None;
                    while let Some(event) = pending.take().or_else(|| events.next()) {
                        if let Event::Key(key) = event {
                            let (quit, next) =
                                rune::input::handle_key_batch(&mut tabs, key, (120, 40), || {
                                    Ok(events.next())
                                })
                                .unwrap();
                            assert!(!quit);
                            pending = next;
                        }
                        terminal.draw(|f| draw_ui(f, &mut tabs)).unwrap();
                    }
                    black_box(tabs);
                },
                criterion::BatchSize::SmallInput,
            )
        });
    }
    group.finish();
}

fn bench_terminal_output(c: &mut Criterion) {
    use ratatui::{backend::Backend, buffer::Buffer, layout::Rect};
    let mut group = c.benchmark_group("terminal_output");
    for (name, text) in [
        ("ascii_100_draws", "abcdef ".repeat(16)),
        ("unicode_100_draws", "界e\u{301} ".repeat(28)),
        ("contiguous_ascii_100_draws", "a".repeat(112)),
    ] {
        let buffer = Buffer::with_lines(std::iter::repeat_n(text, 37));
        let empty = Buffer::empty(Rect::new(0, 0, buffer.area.width, buffer.area.height));
        let updates = empty.diff(&buffer);
        group.bench_function(name, |b| {
            b.iter(|| {
                let mut bytes = Vec::new();
                let mut backend = rune::backend::TerminalBackend::new(&mut bytes);
                for _ in 0..100 {
                    backend.draw(updates.clone().into_iter()).unwrap();
                }
                black_box(bytes);
            })
        });
    }
    group.finish();
}

fn bench_live_search_latency(c: &mut Criterion) {
    let mut group = c.benchmark_group("live_search_latency");
    for lines in [100_000, 1_000_000] {
        let text = make_doc(
            lines,
            "record 00000000  alpha beta gamma delta epsilon 0123456789",
        ) + "quartz target at EOF\n";
        let rope = Rope::from_str(&text);
        group.bench_function(format!("type_6_chars_{lines}_lines"), |b| {
            b.iter(|| {
                let mut search = SearchState::default();
                for query in ["q", "qu", "qua", "quar", "quart", "quartz"] {
                    search.search_buffer = query.into();
                    let result = search.find_all_match_spans(black_box(&rope));
                    assert_eq!(result, [(lines, 0, query.len())]);
                    black_box(result);
                }
            });
        });
    }
    for (name, text, query, regex, count) in [
        (
            "dense_literal",
            make_doc(50_000, "alpha beta gamma"),
            "a",
            false,
            10_000,
        ),
        (
            "regex_control",
            make_doc(50_000, "alpha beta gamma"),
            "a[a-z]+",
            true,
            10_000,
        ),
        (
            "unicode_literal",
            "界Kelvin Σσς word\r\n".repeat(50_000),
            "kelvin",
            false,
            10_000,
        ),
    ] {
        let rope = Rope::from_str(&text);
        group.bench_function(name, |b| {
            b.iter(|| {
                let mut search = SearchState::default();
                search.search_buffer = query.into();
                search.use_regex = regex;
                let result = search.find_all_match_spans(black_box(&rope));
                assert_eq!(result.len(), count);
                black_box(result);
            });
        });
    }
    group.finish();
}

criterion_group!(
    benches,
    bench_live_search_latency,
    bench_terminal_output,
    bench_input_bursts,
    bench_typing_cache,
    bench_hex_rendering,
    bench_bulk_indentation,
    bench_syntax_efficiency,
    bench_clipboard_efficiency,
    bench_search_interactions,
    bench_fuzzy_pruning,
    bench_remaining_efficiency,
    bench_common_efficiency,
    bench_rare_efficiency,
    bench_second_efficiency_pass,
    bench_efficiency_edges,
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
