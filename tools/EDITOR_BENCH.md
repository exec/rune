# Editor Bench v1

A source-only companion to Rune. One command downloads checksum-pinned
competitors, builds them and Rune, drives real PTYs, verifies every saved byte,
and prints timings and an opinionated score. Python's standard library only;
no benchmark code is linked into Rune.

```sh
python3 tools/editor_bench.py
# Quick end-to-end check; one sample is not a reliable ranking:
python3 tools/editor_bench.py --runs 1 --warmups 0
# Select an existing Rune build; competitors still use the pinned source builds:
python3 tools/editor_bench.py --rune /absolute/path/to/rune --output target/editor-bench/recheck.json
```

Requires macOS or Linux, Python 3.12+, Rust/Cargo, a C compiler, Make, and
ncurses development headers/libraries. On macOS, Xcode Command Line Tools supply
the C build tools; Linux commonly packages the headers as `libncurses-dev` or
`ncurses`. Missing dependencies fail with a build log; the tool never installs
system packages. Run on an otherwise idle machine. Builds finish before timing.

## What earns points

Responsiveness gets 70%: typing 40%, navigation 15%, search 15%. Opening and
saving get 10% each, paste 5%, and pathological long lines 5%. A routine keystroke
matters more than a rare giant line. Small files get 60% of each size-dependent
metric's weight, medium files 35%, large files 5%.

| Metric | Weight | Small budget | Medium budget | Large budget |
| --- | ---: | ---: | ---: | ---: |
| One ASCII key | 20% | 0.5 ms | 0.5 ms | 0.5 ms |
| 256 ASCII characters in a burst | 10% | 2 ms | 2 ms | 2 ms |
| Wrapped Unicode burst | 10% | 4 ms, fixed fixture | — | — |
| Move down 256 lines and insert sentinel | 15% | 4 ms | 4 ms | 4 ms |
| Search for `quartz` near EOF and insert sentinel | 15% | 4 ms | 12 ms | 50 ms |
| Open through first fixture output | 10% | 10 ms | 20 ms | 100 ms |
| Save unchanged and exit | 10% | 5 ms | 15 ms | 75 ms |
| Paste 1 MiB of multiline text | 5% | 25 ms | 25 ms | 25 ms |
| Append 256 characters to a million-character line | 5% | 10 ms, fixed fixture | — | — |

Small/medium/large fixtures have 1,000/100,000/1,000,000 indexed ASCII records,
approximately 59 KB/5.9 MB/59 MB. Unicode uses 500 wrapped CJK/combining-character
lines. Its 256-character insertion contains 247 CJK characters and nine ASCII
sentinel letters. ASCII and long-line bursts also end with that nine-letter
sentinel. These are plain-text workloads; syntax highlighting is off.

Budgets are deliberately demanding engineering targets with room left for
graphical terminal rendering, not claimed thresholds of human perception.
For each component, `ratio = min(1, budget_ms / median_ms)`. The score is
`100 × exp(sum(component_weight / 100 × log(ratio)))`. It ranges from 0 to 100;
100 means every median meets its budget. Faster-than-budget results get no
extra credit. Normalization uses fixed budgets, not whichever rival happens to
win. Weights sum to 100 and never redistribute after a missing test.

Keep the raw numbers: a score cannot replace them. The JSON contains every
sample plus component medians, nearest-rank observed p95s, budgets, and weights.
The p95 is descriptive, not an established tail-latency guarantee. The default
has two excluded warmups and fifteen measured rounds: **1,020 verified sessions**.
Jobs run serially in seeded shuffled order. A crash, timeout, wrong save, missing
save, or incomplete sample set fails the run and produces no score. Interrupted
runs retain partial samples with a failed/running status rather than a ranking.

Changing the fixtures, protocol, budgets, or weights requires a new suite version.
Compare scores using the same suite and pinned builds. Hardware/OS/storage
differences affect the score; it does not isolate CPU or RAM performance.

## Timing contract

The driver reuses [`editor_compare.py`](editor_compare.py). It sends ordinary
UTF-8 input, except the paste workload which uses bracketed paste. Typing,
navigation, search, and paste end when the inserted marker arrives in PTY output
and any active synchronized update has ended. Synchronization state survives
split escape sequences and bounded-output-buffer eviction. Saving follows
outside those timers. Navigation and search include inserting the nine-character
sentinel; there is no claim to observe an internal cursor move in isolation.

Search enters the actual search UI, types six characters, confirms, and inserts
the sentinel. Rune performs incremental searches; nano/Vim retain their isolated
default behavior. This intentionally measures the same user task rather than
equalizing internal search counts. Opening is measured during save-only sessions
and is a first-fixture-output lower bound, not proof all startup work finished.
Save/exit has a process-completion boundary. Each session separately records
opening, task, save/exit, child CPU, output bytes, and OS peak RSS.

The PTY drains continuously; it is not a graphical terminal emulator. These are
receipt times, not physical key-to-screen latency. The fixtures have warm caches.
Resource counters include the entire child lifetime and pre-exec Python work;
Linux peak RSS can be dominated by inherited Python memory. CPU, RSS, output
volume, and build time have **zero score weight**. Rune retains atomic saving
and external-change checks; editors' durability and undo policies differ.

## Competitors and observability

The tool fetches upstream [GNU nano 9.3](https://www.nano-editor.org/download.php)
and [Vim 9.2.1178](https://github.com/vim/vim/releases/tag/v9.2.1178), with SHA-256
pins in `SOURCES`. Downloads are checked before safe tar extraction. Both use
`-O2 -g`, disabled message translation, and private installation prefixes; Vim
uses its huge terminal feature set with GUI/X integration excluded. Existing
editor installations and shell settings are untouched. Personal configuration,
plugins, backups, and network update checks are excluded from timed sessions.

No source patch is needed for v1: external markers and process exit provide
observability. Vim's unanswered terminal queries are disabled through its command
line configuration, avoiding the artificial exit wait found in the Linux run.
If instrumentation becomes necessary, an optional
`tools/editor-bench-patches/nano.patch` or `vim.patch` is applied to a freshly
extracted tree before configuration. Patches must add observability only, never
speed up/disable the measured work. Their hashes are recorded and change the
build-cache key. No patch files ship until one is needed.

Sources, private installations, logs, and JSON live only under ignored
`target/editor-bench/`, excluded from source packages and release binaries.
Build manifests record source URL/checksum, compiler, configuration, patch hash,
build duration, and binary hash. Results additionally record the driver/PTY
hashes, Rune source hashes, versions, CPU/OS, and sample counts. The suite fixes
the terminal at 120×40. Cached binaries are checksum-checked before reuse.

For stack profiling, use the existing comparison driver's `--profile` mode with
the private binaries listed in the JSON; profiling runs are separate from scores.
Windows, graphical rendering, cold storage, syntax-heavy workloads, and full
editor feature coverage are outside v1.

## Initial validation

The November 2, 2026 UTC captures ran the defaults on Apple M3 Pro and Ryzen 9
7900X: 1,020 verified sessions per host, with the same driver/PTY hashes and
source-built competitor versions. Fifteen self-checks pass on both hosts; the
legacy comparison command still passes its editing/save check. Source-package
inspection confirms downloaded competitor code and builds are excluded.

| Host | Rune | GNU nano | Vim |
| --- | ---: | ---: | ---: |
| M3 Pro / macOS | 95.02 | 69.72 | 79.56 |
| 7900X / CachyOS Linux | 100.00 | 66.78 | 79.52 |

These are budget-compliance scores, not relative speed multipliers. For example,
Rune's small-file one-key median is 0.254 ms on Mac and 0.203 ms on Linux, while
nano's is 0.083/0.081 ms; both meet the 0.5 ms target. Rune's Mac save median
misses the 5 ms small-file budget at 5.453 ms, and its 1 MiB paste medians miss
25 ms at roughly 63 ms. Linux meets both. Weights/budgets were not changed after
these captures.

Raw captures, provenance, and printed timing tables are in
`target/editor-bench/v1-mac.json`, `v1-linux.json`, and their `.log` files.
The Linux setup remains at `~/rune-bench/editor-bench-v1` on `dylan@10.0.0.18`;
`./run.sh` invokes the tool using that host's existing private Rust toolchain.

```sh
python3 -m unittest discover -s tools -p 'test_editor*.py' -v
```
