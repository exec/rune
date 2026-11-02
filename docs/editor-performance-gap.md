# Cursor-gap output pass

When the cursor is already on the requested row, Rune now uses column-only positioning (`MoveToColumn`) for gaps and backtracking. Contiguous cells still emit no move. Row changes, the first cell of a frame, and uncertain positions still use full row-and-column positioning.

This uses the standard [CHA cursor-horizontal-absolute sequence](https://xtermjs.org/docs/api/vtfeatures/). It saves the row number and separator: 2–6 bytes per eligible move, and one integer to format. No skipped text is repainted. The runtime change is four additional lines in the existing backend; no dependencies, save behavior, or cache policies change.

## Emitter measurements

These are output-construction benchmarks: **100 draws of 37 rows**, measured against an immutable executable built immediately before the change. They are not per-character or save-inclusive timings.

| Text with unchanged gaps | Before | After | Less emission time |
| --- | ---: | ---: | ---: |
| ASCII | 5.45 ms | 5.00 ms | About 8% |
| CJK + combining characters | 6.50 ms | 5.39 ms | About 17% |

A contiguous-ASCII control initially measured 3.98 → 4.17 ms, but a reverse-ordered repeat measured 4.07 ms before and 3.92 ms after. The apparent slowdown did not repeat; variation prevented a reliable timing change estimate for that control. Its emitted bytes and the already-contiguous runtime path remain unchanged.

The Criterion cases are `terminal_output/ascii_100_draws`, `unicode_100_draws`, and `contiguous_ascii_100_draws`. Logs are retained under `target/editor-compare/cursor-gap-pass/`.

## Real terminal before/after

The paired run used 21 measured rounds and three excluded warmups, in seeded shuffled order. All **144 sessions** passed exact save verification. The frozen before binary matches the after binary from the preceding Unicode output pass.

| Task, including save and exit | Before median | After median | Before bytes | After bytes | Less output |
| --- | ---: | ---: | ---: | ---: | ---: |
| 256 ASCII characters; 1,000 lines | 9.51 ms | 8.61 ms | 11,661 | 10,007 | 14.2% |
| Save unmodified; 1,000 lines | 6.98 ms | 6.83 ms | 4,737 | 4,000 | 15.6% |
| 256 CJK characters; wrapped Unicode | 12.73 ms | 12.96 ms | 32,569 | 26,444 | 18.8% |

The paired bootstrap 95% intervals for median after/before workflow-time ratios are 0.820–1.129 (ASCII), 0.851–1.065 (save), and 0.941–1.125 (Unicode). All include 1: this run does not establish a save-inclusive latency improvement. The lower byte counts and emitter savings are the supported wins.

Together with the preceding pass, wrapped Unicode output is down from the original 46,834 bytes to 26,444 bytes, a **43.5% cumulative reduction**. The present pass alone accounts for 18.8%.

## Refreshed Rune/nano/Vim comparison

Captured 2026-11-02T02:18:00.674830+00:00 on Apple M3 Pro, terminal 120×40, using the current unreleased Rune build, GNU nano 9.3, and Apple Vim 9.1. Seven measured sessions per cell follow one excluded warmup; all **240 sessions** passed byte-for-byte verification. These are same-run comparisons; do not attribute differences from earlier runs solely to code changes.

Milliseconds are medians for **task + save + exit**. Startup and the fixed settling period are excluded. Native save semantics remain different; Rune retains external-change checks and durable atomic writes.

| Task | Rune | nano | Vim |
| --- | ---: | ---: | ---: |
| Type 256 characters; 1,000 lines | 6.10 | 2.01 | 5.40 |
| Move down 256 lines + insert; 1,000 lines | 6.99 | 2.40 | 6.79 |
| Find unique match near EOF + insert; 1,000 lines | 6.45 | 1.74 | 5.46 |
| Save unmodified; 1,000 lines | 5.56 | 1.13 | 4.22 |
| Type 256 characters; 100,000 lines | 15.07 | 23.34 | 15.39 |
| Move down 256 lines + insert; 100,000 lines | 14.03 | 22.50 | 17.99 |
| Find unique match near EOF + insert; 100,000 lines | 23.41 | 47.22 | 27.18 |
| Save unmodified; 100,000 lines | 12.50 | 22.42 | 16.46 |
| Append to million-character line | 30.07 | 412.97 | 30.76 |
| Wrapped Unicode typing | 9.01 | 3.49 | 11.41 |

The broad priorities remain: short-file saves take more time in Rune, while Rune is competitive on these large-file workflows. In the 100,000-line typing case, peak RSS remains about **16.8 MiB for Rune versus 11.8 MiB for Vim**. That loading-memory gap remains a separate optimization target.

## Verification and artifacts

- 375 Rust tests pass in debug and release; two explicit live-network tests remain ignored. Clippy, formatting, and whitespace checks pass.
- 32 old/new real-PTY screen pairs match cells, colors, attributes, cursor positions, and saved bytes, across plain/highlighted text, selection, wrapping, and four terminal sizes.
- Unit checks cover forward gaps, backtracking, overlap, column zero, near-limit coordinates, unchanged contiguous output, uncertain emoji widths, and cursor-command write failures.
- Raw samples are in `target/editor-compare/cursor-gap-before-after.json` and `cursor-gap-current.json`. Frozen binaries and replay helpers are in `target/editor-compare/cursor-gap-pass/`.

Physical key-to-screen latency, other operating systems, and cold-storage behavior remain unmeasured. No release or installation was performed.
