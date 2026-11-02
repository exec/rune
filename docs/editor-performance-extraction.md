# First rune-core extraction: correctness and performance

Rune v1.7.0 (`790b181`) and Etch v0.2.0 (`a6c7d39`) are the pushed,
pre-extraction checkpoints. This pass lives on `extract-rune-core` in each repo.
The core is a workspace crate, not a second copy of the engine or a process bridge.

## Scope

Documents, shared undo, file I/O, search, neutral syntax, and their tests moved
into `crates/rune-core`. Rune keeps document/search/file-I/O re-exports. Its terminal
style mapper and Crossterm adapter stay in Rune. Etch depends directly on core,
retaining its `rune` dependency alias and `etch::rune` public re-export.

The core has 22 packages in its normal dependency tree, with no Rune application,
Ratatui, Crossterm, GPUI, CLI, settings, directory-walker, or updater dependency.
CI checks this boundary. Highlight themes specialize at compile time; documents
always use neutral roles. No edit, search, save, or cache algorithm was rewritten.
Hot width helpers and small document accessors have inline annotations to retain
optimizer visibility across the new crate boundary.

Legacy column/viewport/hex buffer internals remain in hidden core modules, behind
the Document API. Separating those from document storage is the next extraction
step; new consumers should not adopt those legacy internals.

## Criterion

Apple M3 Pro, macOS, Rust 1.96.0. Frozen before/after release executables; builds
and tests finish before timing. 25 samples, 0.5 s warmup, 2 s measurement target
(extended automatically for expensive batched commands). These are Criterion
point estimates, not graphical input latency or cross-editor scores.

Core results compare the pre-extraction **headless** build to the final core,
matching Etch's previous feature configuration. Terminal workloads compare the
normal pre-extraction Rune build to the extracted Rune build.

| Core workload | Before | After |
| --- | ---: | ---: |
| Insert one | 831 ns | 826 ns |
| Replace in a million-character line | 1.491 µs | 1.495 µs |
| Undo + redo in that line | 185 ns | 196 ns |
| Indent 100,000 lines | 8.472 ms | 8.396 ms |
| Unindent 100,000 lines | 10.861 ms | 10.787 ms |
| Comment 100,000 lines | 8.463 ms | 8.665 ms |
| Auto-indent in a million-character line | 1.346 µs | 1.401 µs |

| Terminal workload | Before | After |
| --- | ---: | ---: |
| 100 insert/redraw frames | 9.614 ms | 9.438 ms |
| Six live-search changes, 100,000 lines | 3.291 ms | 3.375 ms |
| Highlight 50 Rust lines, cold | 46.041 µs | 48.126 µs |
| Highlight 50 Rust lines, warm | 8.994 µs | 9.129 µs |
| Invalidate and rehighlight 50 lines | 54.466 µs | 57.265 µs |

Initial captures showed larger apparent slowdowns, including document replacement
at 2.070 µs and auto-indent at 1.639 µs. Alternating frozen builds (after/before/
before/after, 30 samples and a 3 s target) also showed substantial host variation:
old replacement estimates ranged 1.565–1.789 µs and new ones 1.717–1.923 µs.
The final measurements above follow the inline annotations. Raw initial and
confirmation runs are retained; these results do not establish identical timings.

## Real terminal replays and native profiles

The existing PTY harness verifies exact saved bytes for every session. Final
captures use five tasks, one excluded warmup and seven measured runs per build
in seeded shuffled order: 80 verified sessions. Timers cover the editing task,
save, and exit; startup/settling are separate. Typing inserts a **256-key burst**
into a 1,000-line file, not one key and not 1,000 inserted characters.

The small first sample still varied considerably, so typing, long lines, and
wrapped Unicode received a second shuffled capture with one excluded warmup and
24 measured runs per build: 150 more verified sessions. Both sets are shown.

| Task | Initial before → after | Confirmation before → after | Confirmed change |
| --- | ---: | ---: | ---: |
| 256-key burst + save/exit | 6.223 → 6.946 ms | 6.140 → 6.315 ms | +2.8% |
| Million-character-line append + save/exit | 24.247 → 25.660 ms | 22.656 → 22.302 ms | −1.6% |
| Wrapped Unicode burst + save/exit | 8.532 → 9.985 ms | 7.586 → 7.875 ms | +3.8% |
| Save unchanged small file + exit | 5.649 → 5.677 ms | — | — |
| Find near EOF, 100,000 lines + save/exit | 24.858 → 24.273 ms | — | — |

Three-second native `sample` captures before/after each implementation stage also
exercise byte-verified terminal typing. Profiles include setup and are not used
as latency measurements. The final checked paths show no large repeatable
slowdown; some small overhead and variation remain. These are desktop smoke
measurements, not isolated hardware guarantees. No fresh GUI frame/input profile
was captured.

## Verification

- macOS: 416 default/workspace tests, 134 headless tests, 18 Etch tests; Clippy
  passes with/without terminal features and in Etch; formatting passes.
- Linux/Debian: 415 workspace tests plus the added terminal-adapter test, 134
  headless tests, all 98 core tests, and 18 Etch tests pass in release mode.
  The final core/adapter/Etch changes were rebuilt and retested there.
- `cargo package --locked --allow-dirty --offline -p rune-core` verifies that the
  standalone packaged engine compiles without the frontend source tree.
- A separate consumer depending only on Etch still compiles; the optimized macOS
  bundle builds and passes signature/plist/version checks (0.2.0).
- Engine tests retain save-conflict/overwrite/symlink protections, Unicode/CRLF
  selections and history, cache invalidation, randomized edits, and dense/sparse
  editing coverage. Terminal style mapping has an explicit native-color/cache test.

Artifacts: `target/core-extraction/` contains frozen executables, original and
confirmation Criterion logs, native replay JSON, stack profiles, package/build/
test logs, and Linux logs. GUI appearance, live IME, and Windows runtime behavior
were not retested in this engine-only pass. Core I/O remains synchronous.
