# First editor comparison baseline

Captured 2026-11-02T01:42:57.142741+00:00 on **Apple M3 Pro**, macOS-27.0.1-arm64-arm-64bit-Mach-O, terminal 120×40.

Rune is the current unreleased workspace build; GNU nano is 9.3, Vim is Apple’s 9.1 (patches 1–1752). Binary hashes and launch commands are retained in `target/editor-compare/baseline.json`.

Seven measured sessions per cell, after one excluded warmup. All **240 sessions** (including warmups) passed byte-for-byte save verification. Jobs ran serially in shuffled rounds, after Cargo checks and profiling had finished. See [methodology and commands](editor-performance.md) before interpreting these numbers.

## Task, save, and exit

Milliseconds, shown as **median [minimum–maximum]**. Startup and the fixed settling period are excluded from this wall-time column. Native saving and process cleanup remain included.

| Task | Rune | GNU nano | Vim |
| --- | ---: | ---: | ---: |
| Type 256 chars; 1,000 lines | 5.99 [4.19–7.41] | 1.56 [1.26–2.11] | 5.18 [4.09–6.79] |
| Move down 256 lines + insert; 1,000 lines | 6.35 [4.00–8.05] | 2.10 [1.78–3.76] | 6.31 [5.18–8.59] |
| Find unique `@` at EOF + insert; 1,000 lines | 6.36 [5.49–7.79] | 1.58 [1.34–2.05] | 4.62 [2.98–5.39] |
| Save unmodified; 1,000 lines | 5.09 [3.94–5.57] | 1.10 [0.91–1.31] | 4.09 [2.81–5.15] |
| Type 256 chars; 100,000 lines | 15.77 [12.83–17.78] | 19.85 [19.08–24.45] | 15.41 [12.26–17.63] |
| Move down 256 lines + insert; 100,000 lines | 13.10 [10.69–20.37] | 21.08 [19.78–23.79] | 16.70 [15.80–22.69] |
| Find unique `@` at EOF + insert; 100,000 lines | 20.83 [17.72–26.81] | 43.58 [40.61–44.90] | 24.27 [22.40–27.48] |
| Save unmodified; 100,000 lines | 11.81 [10.03–17.78] | 19.47 [18.55–24.89] | 14.58 [12.97–16.89] |
| Append 256 chars to a million-character line | 25.90 [20.84–35.62] | 406.08 [401.04–441.48] | 29.11 [24.54–45.18] |
| Type 256 CJK chars; wrapped Unicode | 8.55 [7.49–12.14] | 2.99 [2.71–4.27] | 9.00 [8.77–11.27] |

## Startup and resources

These rows use the typing task’s sessions. First-fixture time is a first-paint lower bound. CPU and peak RSS cover the entire child lifetime, including startup. The 100,000-line fixture is 5,900,035 bytes (5.63 MiB).

| Fixture | Editor | First fixture, ms | Child CPU, ms | Peak RSS, MiB |
| --- | --- | ---: | ---: | ---: |
| 1000 lines | rune | 6.29 | 6.21 | 4.62 |
| 1000 lines | nano | 9.75 | 8.61 | 7.41 |
| 1000 lines | vim | 11.04 | 8.86 | 5.53 |
| 100000 lines | rune | 10.30 | 18.44 | 16.81 |
| 100000 lines | nano | 28.57 | 44.19 | 18.00 |
| 100000 lines | vim | 19.97 | 25.94 | 11.84 |

## Targets suggested by this run

- **Investigate short-file save overhead.** The small typing workflow takes 5.99 ms in Rune versus 1.56 ms in nano. The save-only control is also slower: 5.09 versus 1.10 ms. This suggests prioritizing save behavior and I/O before attributing the gap to typing. Atomic replacement, external-change checks, and durability must be preserved; the editors’ save guarantees differ.
- **Reduce terminal output for wrapped Unicode.** Median output across the entire session is 46,834 bytes for Rune, 8,182 for nano, and 8,297 for Vim. Rune emits roughly 5.7× nano’s bytes. Rune’s sustained editing stack profile also spends substantial time in Ratatui’s buffer-diff path. Investigate redraws, cell changes, and terminal writes with a focused follow-up; this run alone does not establish their individual costs.
- **Investigate large-file loading peak memory.** On the 5.63 MiB fixture, Rune peaks around 16.8 MiB, compared with Vim’s 11.8 MiB. Distinguish temporary loading copies from steady retained memory before changing storage.
- **Keep the long-line case as a regression guard.** Rune’s 25.9 ms median is in the same range as Vim’s 29.1 ms; nano takes 406.1 ms here. This workload does not support a blanket claim about overall editor speed.

These are initial local goals, not universal rankings. The Unicode and long-line cases have different file sizes; timings should only be compared across editors within the same row. Individual key-to-screen latency, highlighted source files, long incremental queries, cold disk, and other operating systems remain separate passes.
