# Comparing Rune, GNU nano, and Vim

For the source-built competitors and weighted latency score, use
[Editor Bench v1](../tools/EDITOR_BENCH.md): `python3 tools/editor_bench.py`.
The driver below remains available for focused comparisons and stack profiles.

Run the same logical tasks through real pseudo-terminals, verify every saved
file, and retain raw measurements before choosing optimization targets.
The harness uses Python 3.11+ and its standard library. It does not change your
home directory, normal editor settings, or existing documents.

The [first measured baseline](editor-performance-baseline.md) records the initial
results and the optimization targets they suggest.
The [first output optimization](editor-performance-output.md) records a follow-up
against the original binary, including unchanged timing controls.
The [cursor-gap pass and refreshed comparison](editor-performance-gap.md) record
the next output change and a new Rune/nano/Vim baseline.
The [buffered loading pass](editor-performance-load.md) records reduced large-file
opening memory, load-time tradeoffs, and a focused comparison with nano and Vim.
The [ASCII typing pass](editor-performance-typing.md) isolates typing from saving
and removes Unicode parsing from printable ASCII screen updates.
The [terminal paste pass](editor-performance-paste.md) targets visible pauses
when pasting larger blocks of text.
The [live-search pass](editor-performance-search.md) targets query feedback
delays in larger log files while preserving Unicode matching and result limits.
The [Ryzen 9 7900X comparison](editor-performance-7900x.md) compares the current
build with nano and Vim on Linux, alongside a fresh Apple M3 Pro reference.

```sh
cargo build --locked --release
python3 tools/editor_compare.py --runs 7
```

On macOS, `/usr/bin/nano` may be Pico. The harness rejects Pico rather than
reporting it as GNU nano. Install GNU nano if necessary and select its path:

```sh
brew install nano
python3 tools/editor_compare.py --nano /opt/homebrew/bin/nano --runs 7
```

Use `--rune` and `--vim` to compare other builds. The default Rune executable is
the current workspace's release build, which may include unreleased changes.
This is separate from Criterion's internal Rune benchmarks in `benches/editor_perf.rs`.

## Workloads

Default fixtures have 1,000 and 100,000 ordinary lines. Two additional fixtures
cover a million-character line and wrapped Unicode. The terminal is 120×40.

| Case | Logical task, followed by save and exit |
| --- | --- |
| `type_N` | Insert 256 ASCII characters at the start of an N-line file. |
| `navigate_N` | Move down 256 logical lines, insert `X`, and verify its position. |
| `find_N` | Find a unique literal `@` near EOF, insert `X` before it, and verify the position. |
| `save_N` | Explicitly save the unmodified file. |
| `longline_append` | Move to the end of a million-character line and append 256 ASCII characters. |
| `unicode_wrap` | Insert 256 CJK characters into a wrapped document containing CJK and combining characters. |

Navigation uses 256 individual movement commands, without a counted jump.
The single-character search deliberately requires only one query evaluation;
Rune's incremental query behavior would otherwise do more work than nano's
search-on-confirm behavior. This is not a multi-character live-search comparison.
Typing cases send ordinary key events in a burst, without bracketed paste.

All editors have personal settings and plugins excluded. Rune uses a disposable
`--config` file, including disabled update checks. GNU nano uses
[`--ignorercfiles`](https://www.nano-editor.org/dist/latest/nano.html), and Vim uses
[`-u NONE -U NONE -i NONE --noplugin`](https://vimhelp.org/starting.txt.html).
Syntax highlighting is off for the `.txt` fixtures in current builds. Earlier
Rune builds still applied generic number/string/comment coloring without a
selected language; the typing report distinguishes that behavior change.
Vim terminal queries are disabled because the PTY does not answer them; pending
queries can otherwise add an artificial wait during exit.
Line numbers, automatic indentation, backups, and wrapping are off, except for
the explicit Unicode wrapping case. Each editor retains its
normal UI chrome and save implementation. Matching terminal size does not mean
identical editor-pane heights.

## What the measurements mean

- **First fixture ms:** parent launch through receipt of the first fixture marker
  in terminal output. This is a first-paint lower bound, not proof that every
  startup task has finished. A fixed 30 ms settling period follows, outside the
  two wall-time measurements.
- **Task+save+exit ms:** sending the task through successful process exit. It
  includes editing, terminal output, saving, and cleanup. Exit acknowledges that
  all queued input has finished; silence on the terminal does not.
- **Child CPU ms:** user plus system CPU for the entire child lifetime, including
  startup and launch setup. It is not editing-only CPU.
- **Peak MiB:** the child's operating-system peak resident set across that lifetime,
  not exclusive heap allocation or a memory delta.
  On Linux, this counter can retain the forked Python child's pre-exec memory;
  exclude affected RSS figures from editor-memory comparisons.
- **Output bytes:** bytes drained from the PTY, including control sequences and
  startup/exit output. The harness drains continuously to avoid artificial output
  stalls. It does not emulate a graphical terminal or measure GPU/font rendering.

Every run must produce exactly the expected bytes and actually write the file.
An incorrect result, missing save, crash, or timeout fails the comparison and
writes a bounded `.failure.ansi` diagnostic beside the requested JSON output.
Temporary files are removed and owned child processes are reaped on failure.

One warmup round is excluded, followed by seven measured rounds. Jobs run
serially in a seeded shuffled order. Files are newly generated and thus have
warm OS caches: these are not cold-disk tests. Run on an otherwise idle machine,
with consistent power and thermal conditions, and do not run Cargo tests or
profilers alongside the timed comparison.

The printed table contains medians. JSON retains every sample, warmup markers,
commands, versions, binary and harness hashes, fixture hashes, CPU/OS details,
terminal size, and Git revision/dirty status. Results default to
`target/editor-compare/latest.json`, outside version control. Compare distributions
and repeated runs before making a regression claim; seven samples do not
establish reliable tail latency.

Rune's save path verifies external changes and uses atomic replacement. Those
safeguards remain enabled. A save-time gap is a target for investigation, not a
reason to remove protection or assume that every editor offers the same durability.
These tests do not measure individual key-to-screen latency, interactive search
with long queries, syntax highlighting, undo history equivalence, or cold storage.

## Stack profiling

Profiling runs separately from measurement and is marked as such in the JSON.
On macOS the harness attaches `sample`; on Linux it uses `perf record` if available
and permitted. Native sampling can include time waiting for input or I/O, so stack
counts are not automatically CPU percentages.

```sh
python3 tools/editor_compare.py --profile rune --case type_1000 --profile-seconds 5
python3 tools/editor_compare.py --profile nano --case type_1000 --profile-seconds 5
python3 tools/editor_compare.py --profile vim  --case type_1000 --profile-seconds 5
```

The selected editor repeatedly performs a neutral cycle under sustained input
(e.g. insert 256 characters and backspace 256). Other cycles return navigation
or search to the start, or repeat saves. A final normal task verifies that the
cycles preserved the document and cursor position. This keeps the document size
bounded; native undo storage policies can still differ. Profile timing/resource
numbers must not be compared with unprofiled benchmark samples.

Artifacts are saved beside the JSON as `.sample.txt` or `.perf.data`. Sampling
permission errors fail explicitly; the script does not request root access.
For Rune source locations as well as function names, build with release debug info:

```sh
CARGO_PROFILE_RELEASE_DEBUG=1 cargo build --locked --release
```

To focus a comparison or save a reference run:

```sh
python3 tools/editor_compare.py --case find_100000 --runs 15 --output target/editor-compare/search.json
python3 tools/editor_compare.py --lines 1000 --case type_1000 --output target/editor-compare/small.json
```

Run the driver self-checks with:

```sh
python3 -m unittest discover -s tools -p 'test_editor_compare.py' -v
```
