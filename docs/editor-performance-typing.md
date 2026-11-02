# Printable ASCII typing

This pass removes Unicode grapheme parsing from drawing cached printable ASCII
into screen cells. The renderer already expands tabs and removes display
controls before caching a line; eligibility now explicitly requires every byte
to be in `0x20..=0x7e`. Each eligible byte writes one cell with the original span
style. Unicode continues through the existing renderer.

Queued ordinary input now processes up to 256 keys before drawing, instead of
64, within the same 8 ms time budget. The old count cap forced intermediate
screens even when processing was well below that budget. Isolated keys still
draw without waiting for more input; viewport updates and undo snapshots remain
per key, and mode changes, resize/mouse events, and other barriers are returned
untouched for the next iteration.

The sustained small-file insert/backspace profile pointed to screen comparison
and `Buffer::set_stringn`, which was reparsing ASCII as Unicode graphemes on
every redraw. The change uses existing cell APIs and buffer storage. It adds no
dependencies or undo grouping changes.

At the user's request, unrecognized file types (including `.txt`) and untitled
buffers now skip syntax parsing entirely and return plain spans. The generic
keyword table is removed. Recognized languages retain their highlighting;
search and selection highlights still apply over plain text. Save As switches
the cached styling in either direction without requiring a text edit.

## In-process edit and redraw measurements

Criterion excludes setup and disk saving. The frozen before executable contains
the preceding buffered-loading pass. Both executables run serially on Apple
M3 Pro/macOS, with 20 samples, 0.3-second warmups, and a 1-second measurement
target (extended by Criterion where needed). Values below are Criterion's
central time estimates. Batches also include disposing of the editor state.

| Workload | Before | After | Less time |
| --- | ---: | ---: | ---: |
| One key + screen update | 213.34 µs | 149.98 µs | 29.7% |
| 256 queued characters; 1,000 plain records | 492.95 µs | 200.55 µs | 59.3% |
| 256 queued characters; highlighted ASCII | 766.65 µs | 263.70 µs | 65.6% |
| 100 inserts, each with a redraw | 15.764 ms | 9.197 ms | 41.7% |
| 100 backspaces, each with a redraw | 15.758 ms | 9.366 ms | 40.6% |

The one-key and per-edit redraw fixtures use 500 highlighted ASCII lines,
120×40 terminals, and normal editor configuration. The plain-record fixture has
1,000 lines of 59 bytes each and no selected syntax language. These are CPU-side
editor/rendering workloads, not physical key-to-screen latency or save-inclusive
workflows.

The larger batch cap also helps queued Unicode input: the wrapped 256-key burst
takes 1.399 → 0.636 ms. The per-edit Unicode redraw workload remains a control
for the ASCII-specific change, since those redraws do not use the batch handler:
100 redraws take 26.412 → 27.289 ms (a 3.3% increase in the point estimate).
A reverse-ordered confirmation, with 30 samples and a 2-second measurement
target, measured 26.810 ms after and 27.364 ms before. The apparent slowdown did
not repeat; variation prevents a reliable change estimate for this control.

## Real-terminal typing without saving in the timer

A separate PTY driver sends either one ordinary ASCII key or 256 ordinary ASCII
keys, then waits until the terminal output contains the final typed marker and
the end of any active synchronized update. Rune's macOS synchronization bracket
must end before a supporting terminal can display that text.
Saving and exit follow afterward, outside that interval. Every session must
save exactly the expected file bytes and change the file's modification time.

The 256-key payload ends in a unique nine-letter marker; all nine letters differ
and were absent from the initial text. This lets the test observe output
completion through the marker even when the editor redraws only changed cells.
The one-key marker is `~`. The test fails on missing output or incorrect saves.
These receipt times include OS scheduling, terminal serialization and the
parent's PTY handling. They are lower bounds on completed screen painting and
exclude graphical terminal/font rendering.

GNU nano and Vim run with syntax disabled. The original Rune build and the
intermediate optimized build retain generic colors in `.txt` fixtures; the
final build uses plain text. The extra intermediate binary separates the
rendering/input improvements from the requested plain-text behavior change.

Thirty-one measured rounds follow three excluded warmups, in seeded shuffled
order: **340 verified sessions**. These medians use the same run and protocol.
Absolute times should not be compared with earlier captures using only marker
receipt or different system conditions.

| Build | One ASCII key, ms | 256 ASCII keys, ms |
| --- | ---: | ---: |
| Rune before this pass | 1.081 | 3.259 |
| Rune with renderer/batch changes, generic colors retained | 0.932 | 1.645 |
| Rune final, unrecognized files plain | 0.898 | 1.534 |
| GNU nano 9.3 | 0.374 | 0.851 |
| Apple Vim 9.1 | 0.225 | 0.997 |

Final Rune improves by **17.0% for one key** and **52.9% for the burst** against
its frozen before build. Paired bootstrap 95% intervals for median after/before
ratios are 0.748–0.966 and 0.418–0.559, respectively. Most of the measured burst
gain occurs in the renderer/batch changes. The table does not establish a
separate timing benefit for the plain-text policy alone. Nano and Vim still
complete these tasks faster; native undo policies and rendering work differ.

Rune output through the completed burst frame drops from 5,993 to 3,511 bytes
(41.4% less), avoiding intermediate scrolling updates to other visible lines.

## Save-inclusive controls

The existing driver runs 21 measured rounds after three warmups, **144 verified
sessions**, against the same frozen before and final after builds.

| Workflow, including save and exit | Before ms | After ms | Before bytes | After bytes |
| --- | ---: | ---: | ---: | ---: |
| 256 ASCII characters; 1,000 records | 11.86 | 9.26 | 10,003 | 7,538 |
| Save unchanged; 1,000 records | 8.17 | 7.61 | 4,000 | 4,000 |
| 256 CJK characters; wrapped Unicode | 15.70 | 11.29 | 26,444 | 11,898 |

Paired bootstrap 95% after/before ratio intervals are 0.719–0.888 for ASCII
typing, 0.847–1.043 for unchanged saves, and 0.664–0.808 for wrapped Unicode.
The save control includes 1, so this run does not establish a save-time
improvement. Rune retains durable atomic writes and external-change checks.
The larger key batches also reduce Unicode workflow traffic by 55.0%.

## Verification and artifacts

- 384 Rust tests pass in debug and release; two explicit live-network checks
  remain ignored. Clippy with all targets/features and denied warnings,
  formatting, and whitespace checks pass.
- 32 before/after PTY screen pairs match glyphs, attributes, cursor positions,
  and saved bytes. Recognized Rust files retain exact colors. Plain `.txt`
  fixtures are checked against the old screens with syntax foreground colors
  removed; selection colors and UI chrome still match.
- Plain-text tests cover loading unknown extensions, untitled buffers,
  unsupported syntax names, Save As in both directions, search and selection
  highlighting, and control stripping.

The ASCII rendering check compares every printable byte against the previous
Paragraph renderer over varying clipping widths, horizontal offsets, and
nonzero screen origins. It includes empty spans, inherited colors, bold and
italic modifiers, and modifier removal. Existing tests retain coverage for
control stripping, Unicode, search/selection, undo/redo, scrolling, and queued
event barriers.

Frozen before/after app and benchmark executables, replay helpers, source
snapshots, and raw logs are under `target/editor-compare/typing-pass/`.
The original sustained profile is
`target/editor-compare/typing-before-profile.rune-type_1000.sample.txt`.
Raw terminal samples are `target/editor-compare/typing-marker.json` and
`typing-before-after.json`; paired intervals are in `typing-statistics.json`.

Cold storage, other operating systems, and graphical terminal latency remain
unmeasured. This is local unreleased work.
