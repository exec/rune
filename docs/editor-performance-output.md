# Unicode terminal output pass

This pass changes terminal cursor tracking only. Save verification, backup handling, writes, synchronization, and atomic replacement are unchanged. Cargo dependencies and the lockfile are unchanged.

## Why this target

A sustained small-file save profile found `File::sync_all` under 1,505 of 2,065 stack samples (about 73%); rename accounted for another 177 samples. These are wall-time stack samples, including waiting, rather than CPU percentages. The flush preserves the existing save guarantees and was retained. The profile is in `target/editor-compare/save-before-profile.rune-save_1000.sample.txt`.

The installed Ratatui 0.29 backend assumes each printed cell advances one terminal column. A CJK glyph advances two, making its next absolute cursor move redundant. [Upstream cursor tracking](https://github.com/ratatui/ratatui/blob/main/ratatui-crossterm/src/lib.rs) accounts for printed width, but the 0.30.2 release checked during this pass still emitted the same redundant moves. Rune retains its existing dependencies and uses a narrow output backend with the existing serializer’s style behavior. The adapted MIT-licensed portions retain their notice in `LICENSE-RATATUI`.

ASCII retains the simple one-column path. Unicode advances by display width. Cells with VS16 emoji presentation use explicit positioning because terminal widths can vary. Gaps, row changes, overlapping updates, and arithmetic limits still force positioning as needed.

## Measurements

The isolated emitter benchmark draws 37 rows 100 times into a byte buffer. This measures output construction, not a complete editor workflow.

| Output | Before | After | Result |
| --- | ---: | ---: | --- |
| ASCII | 5.47 ms | 5.57 ms | No significant change detected by Criterion |
| CJK + combining characters | 7.70 ms | 6.54 ms | About 15% less emission time |

The real-PTY before/after comparison used the frozen original executable and current release build, 21 measured rounds after three excluded warmups, in seeded shuffled order. All 144 sessions passed exact save verification. Each task includes save and exit. Conditions differed from the initial baseline, so comparisons below use the contemporaneous original binary rather than its earlier timings.

| Task | Before median | After median | Before bytes | After bytes |
| --- | ---: | ---: | ---: | ---: |
| 256 ASCII characters; 1,000 lines | 10.09 ms | 9.79 ms | 11,661 | 11,661 |
| Save unmodified; 1,000 lines | 6.85 ms | 7.44 ms | 4,737 | 4,737 |
| 256 CJK characters; wrapped Unicode | 14.56 ms | 13.59 ms | 46,834 | 32,569 |

**Unicode terminal traffic fell 30.5%: 46,834 to 32,569 bytes per session.** ASCII output is byte-for-byte unchanged. The paired bootstrap intervals for median workflow-time ratios include 1 for all three cases; this run does not establish an end-to-end latency improvement. Saving and other editor work still dominate those short sessions.

Raw workflow data, binary fingerprints, and every sample are in `target/editor-compare/output-before-after.json`. The original executable matches the initial baseline’s SHA-256. The frozen executables and the paired-run helper are retained under `target/editor-compare/output-pass/`. Criterion covers `terminal_output/ascii_100_draws` and `terminal_output/unicode_100_draws` in `benches/editor_perf.rs`.

## Validation

- 373 Rust tests pass in both debug and release; two explicit live-network tests remain ignored.
- Clippy with all targets/features and denied warnings, formatting, and diff whitespace checks pass.
- Backend tests compare emitted style sequences with Ratatui’s serializer across modifier transitions, colors, underline colors, CJK, combining characters, emoji, gaps, row changes, overlap, and multiple frames. Write errors still propagate.
- 32 pairs of real-PTY runs matched visible cells, colors, attributes, cursor positions, and saved bytes in a terminal emulator. Cases include plain and highlighted text, selections, wrapping on/off, and widths of 20, 21, 80, and 120 columns. The local check used pyte as a temporary validation tool; it is not a project dependency.

The source backend can be removed when a compatible released upstream backend supplies correct width-aware cursor tracking. Other operating systems and physical key-to-screen latency were not tested in this pass.
