# Buffered file loading

Rune previously read a complete file into a `String`, then built a rope while
that string remained allocated. Opening now builds the rope through a 64 KiB
`BufReader`, avoiding a file-sized temporary. A small reader adapter preserves
retries on interrupted reads. Ropey still validates UTF-8, and the editor adopts
the new buffer only after loading succeeds.

CRLF detection traverses rope chunks and stops at the first match. Ropey
[keeps CRLF pairs within a chunk](https://docs.rs/ropey/1.6.1/ropey/iter/struct.Chunks.html),
including pairs crossing a read boundary. Existing newline policy, syntax
selection, and failure messages are retained. No dependencies or save behavior
change.

## Real-app opening memory

The frozen before binary is the result of the preceding cursor-gap pass. Both
builds open the same fixtures in real 120×40 pseudo-terminals and quit without
editing or saving. Jobs run serially in seeded shuffled order: 15 measured
rounds after three excluded warmups, **144 verified sessions**. Configurations
are disposable and update checks are disabled. Fixture bytes and modification
times remain unchanged.

Peak RSS covers the entire child process, including startup, rendering, and
cleanup. It is not an exclusive heap measurement. Files have warm OS caches.
The machine is an Apple M3 Pro running macOS.

| Fixture | Before peak MiB | After peak MiB | Reduction |
| --- | ---: | ---: | ---: |
| 1,000 records; 59 KB | 4.12 | 4.12 | No measured increase |
| 100,000 records; 5.9 MB | 16.12 | 10.45 | 35.2% |
| Approximately 50 MB of records | 106.11 | 58.42 | 44.9% |
| Unicode + CRLF; 8 MB | 20.44 | 12.89 | 36.9% |

The 50 MB case removes **47.69 MiB** of peak RSS, close to the size of its former
temporary string. The fixed buffer adds bounded overhead on small files.

| Fixture | Before first fixture ms | After first fixture ms | Before child CPU ms | After child CPU ms |
| --- | ---: | ---: | ---: | ---: |
| 1,000 records | 6.41 | 6.38 | 4.63 | 4.54 |
| 100,000 records | 10.60 | 10.23 | 9.16 | 8.73 |
| Approximately 50 MB of records | 35.63 | 36.87 | 38.23 | 39.56 |
| Unicode + CRLF | 15.33 | 15.24 | 13.99 | 13.93 |

First fixture time is launch through receipt of the fixture marker, a lower
bound on startup rendering. These medians do not establish reliable tail
latencies. This is a memory improvement with mixed timing results: the 50 MB
case takes about 1.24 ms longer to first fixture in this run, while the
100,000-record case takes about 0.37 ms less.

## Loader benchmarks

Criterion measures `Editor::load_file`, including disposing of the loaded
editor, separately from terminal output and saving. Fixtures cover 1, 10, and
50 MB ASCII lines, 100,000 short records, Unicode CRLF, and a small-file control.
The fixtures stay in warm OS caches. Retained fixture strings make Criterion's
process RSS unsuitable for measuring load-only memory.

The confirmation run uses 20 samples per case, a 0.3-second warmup and a
1-second measurement target (extended by Criterion when necessary). Before and
after executables run serially, with builds, tests, and profiling excluded.
The table gives Criterion's central time estimates, rather than PTY medians.

| Fixture | Before | After | Difference |
| --- | ---: | ---: | ---: |
| 1 MB ASCII line | 315.27 µs | 349.43 µs | 34.16 µs more |
| 10 MB ASCII line | 3.922 ms | 3.513 ms | 0.409 ms less |
| 50 MB ASCII line | 20.357 ms | 19.252 ms | 1.105 ms less |
| 100,000 records | 1.887 ms | 2.121 ms | 0.234 ms more |
| Unicode + CRLF; 8 MB | 5.190 ms | 4.959 ms | 0.231 ms less |
| 1,000 records; 59 KB | 29.26 µs | 33.08 µs | 3.82 µs more |

Buffered streaming is not uniformly faster than building from a complete
string. It trades small-file and record-loading overhead for substantially
lower peak memory; large ASCII throughput improves in these warm-cache tests.
These differences are loader timings, not per-keystroke latency.

## Focused Rune/nano/Vim comparison

The existing typing comparison runs 256 ordinary character insertions followed
by save and exit, with identical fixtures and isolated editor configurations.
Fifteen measured rounds follow one excluded warmup: **96 verified sessions**.
These are same-run medians, with each editor retaining its normal save semantics.

| Task | Editor | First fixture ms | Task + save + exit ms | Peak MiB |
| --- | --- | ---: | ---: | ---: |
| Type; 1,000 records | Rune | 5.64 | 5.39 | 4.66 |
| Type; 1,000 records | GNU nano 9.3 | 8.27 | 1.47 | 7.41 |
| Type; 1,000 records | Apple Vim 9.1 | 12.10 | 5.08 | 5.53 |
| Type; 100,000 records | Rune | 9.60 | 14.62 | 11.20 |
| Type; 100,000 records | GNU nano 9.3 | 28.67 | 21.55 | 18.03 |
| Type; 100,000 records | Apple Vim 9.1 | 21.67 | 16.56 | 11.84 |

Rune's large-file typing session now peaks slightly below Vim in this run,
closing the memory gap that motivated the pass. Small-file save-inclusive time
remains higher than nano's. The historical matrix and this focused comparison
were captured at different times; timing changes between them are not a
controlled before/after result.

## Verification and artifacts

- 381 Rust tests pass in both debug and release; two explicit live-network
  checks remain ignored. Clippy with all targets/features and denied warnings,
  formatting, and whitespace checks pass.
- 32 before/after real-PTY screen pairs match visible cells, colors, attributes,
  cursor positions, and saved bytes, across wrapping, selections, plain and
  highlighted files, and four terminal sizes.

Loading tests cover UTF-8 and CRLF around Ropey's and the read buffer's
boundaries, empty files, BOM/NUL, mixed newline styles, lone CR, Unicode line
separators, interrupted and short reads, hard I/O errors, and malformed UTF-8
after a valid prefix. Failed loads preserve unsaved text, saved snapshot, buffer
identity, cursor, viewport, undo history, search, selection, syntax, and hex
state. Successful loads still reset the old state and select syntax by filename.

Raw opening samples and executable hashes are in
`target/editor-compare/load-memory-before-after.json`. Frozen executables and
the opening-memory replay script are under
`target/editor-compare/load-memory-pass/`.
The focused editor comparison is in
`target/editor-compare/load-memory-current.json`.
Loader benchmark executables, sources, logs, and raw Criterion data are retained
under `target/editor-compare/load-memory-pass/`.

```sh
cargo build --locked --release
python3 tools/editor_compare.py --nano /opt/homebrew/bin/nano \
  --case type_1000 --case type_100000 --runs 15 \
  --output target/editor-compare/load-memory-current.json
cargo bench --locked --bench editor_perf -- load_file \
  --sample-size 20 --warm-up-time 0.3 --measurement-time 1
```

Cold storage, other operating systems, and physical key-to-screen latency remain
unmeasured. This is local unreleased work.
