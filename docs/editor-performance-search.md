# Live-search responsiveness

Ordinary literal search previously invoked the matcher separately for every
rope line. Each incremental query change repeated that work. In a 59 MB log,
entering a six-character query spent about 355 ms in the search routines.

Literal search now scans roughly 64 KiB text blocks, retaining a small overlap
so matches spanning blocks are found exactly once. It uses the same cached
regex engine, Unicode simple case folding, overlapping-literal semantics,
source-character offsets, and result limits. Result mapping jumps to the first
match in a block and walks subsequent lines, avoiding repeated tree lookups for
dense results. Scratch storage depends on the block and query sizes, not the
file size; it does not retain a second full-file copy.

Regex searches, queries containing rope line-break characters, and literal
queries longer than 16 KiB retain the existing line scanner. The query-length
condition bounds block scratch storage; it does not reject longer queries.
Interactive replacement's first-match-only lookup retains its existing path.

## Actual Find UI

The frozen before build contains the preceding terminal-paste optimization.
Both builds run in real 120×40 pseudo-terminals on Apple M3 Pro/macOS, using
isolated configurations with updates, wrapping, and automatic indentation off.
The plain `.txt` fixtures contain ordinary 59-byte log records and a unique
`quartz` target near EOF. Files have warm OS caches.

The timed task enters Find, types `quartz` as six ordinary key events, confirms,
and inserts a unique marker at the result. Timing ends when the marker and
completed synchronized screen update arrive. Saving follows outside the timer;
every run must save the exact expected file bytes at the expected match
position and must actually write the destination. These times include UI and
PTY handling, but not graphical terminal/font rendering.

Jobs run serially in seeded shuffled order: 15 measured rounds after two
excluded warmups, **170 verified sessions** including dense-result and regex
controls.

| Fixture | Before median | After median | Speedup |
| --- | ---: | ---: | ---: |
| 1,000 records; approximately 59 KB | 2.01 ms | 1.65 ms | 1.2× |
| 100,000 records; approximately 5.9 MB | 43.27 ms | 5.73 ms | 7.5× |
| 1,000,000 records; approximately 59 MB | 376.15 ms | 35.59 ms | 10.6× |

The largest case removes **341 ms of waiting** from this interaction. The
paired-bootstrap 95% after/before ratio intervals are 0.736–0.864, 0.127–0.142,
and 0.092–0.096, respectively.

| Control | Before median | After median | Ratio interval |
| --- | ---: | ---: | --- |
| Dense `alpha` query; 100,000 records | 6.59 ms | 5.60 ms | 0.734–0.930 |
| Regex-mode `quartz`; 100,000 records | 44.21 ms | 41.50 ms | 0.919–1.027 |

The regex interval includes 1; this pass does not establish a regex speedup.
Terminal output byte-count medians are identical within each case. Every query
change still runs a search; no debounce delay or query skipping is introduced.

For the largest fixture, whole-child peak RSS is 69.55 → 69.67 MiB. Search and
rendering add modest scratch memory rather than a 59 MB document copy. The
child CPU metric includes startup and saving, so it is not editing-only CPU.

## In-process measurements

Criterion measures six successive search-buffer updates without terminal
rendering or saving. Before uses 10 samples and a 0.2-second warmup; after uses
20 samples and a 0.3-second warmup, each with a 1-second measurement target
extended by Criterion as needed. Both use the identical fixture/assertions,
frozen executables, and no concurrent builds, tests, or profilers.

| Workload | Before | After |
| --- | ---: | ---: |
| Six query changes; 100,000 records | 36.00 ms | 3.15 ms |
| Six query changes; 1,000,000 records | 355.15 ms | 32.16 ms |
| Dense ASCII literal, capped results | 207.48 µs | 214.53 µs |
| Regex control, capped results | 662.32 µs | 674.18 µs |
| Unicode-folded literal, capped results | 1.324 ms | 1.054 ms |

The dense ASCII and regex microbenchmarks cost about 7 and 12 µs more in this
capture. The substantial gain is avoiding repeated per-line work across large
sparse searches, while keeping dense-result latency low.

## Verification and artifacts

- 395 Rust tests pass in debug and release; two explicit live-network checks
  remain ignored. Clippy with all targets/features and denied warnings,
  formatting, and whitespace checks pass.
- An independent line-at-a-time reference checks literal results and truncation
  over Unicode folding, combining/astral characters, literal metacharacters,
  source offsets, resume positions, line breaks, overlapping matches, empty
  documents, dense caps, and edits between searches.
- Boundary fixtures exercise matches near block edges and spanning UTF-8
  chunks. A direct scanner test verifies progress when overlap exceeds a block.
- Existing tests cover first-result lookup, result navigation, regex behavior,
  interactive replacement, and replacing beyond the UI result limit.

Raw UI samples and executable hashes are in
`target/editor-compare/live-search-before-after.json`, with paired intervals in
`live-search-statistics.json`. Frozen app/benchmark builds, source snapshots,
replay driver, test logs, and Criterion data are under
`target/editor-compare/live-search-pass/`.

Cold storage, other operating systems, and physical key-to-screen latency remain
unmeasured. This is local unreleased work.
