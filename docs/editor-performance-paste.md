# Terminal paste responsiveness

Large terminal pastes were processed as ordinary keystrokes, with an undo
snapshot per character and repeated screen updates. This created visible
pauses even after ordinary typing had become fast.

Rune now enables bracketed paste on supporting terminals and handles the
resulting paste event as one literal text insertion and one undo step. The
existing inline clipboard insertion moved into `Editor::insert_text`, shared
by terminal paste and Ctrl+U; terminal paste preserves the internal clipboard.
Tabs, indentation, Unicode, and pasted line endings are inserted literally.
Ordinary Enter auto-indentation does not rewrite a pasted block.

Text prompts accept a complete paste with one refresh; Find performs one search
for the completed query. Single-line prompts discard control characters rather
than interpreting them as submission or editor commands. Read-only buffers,
Help, confirmation dialogs, menus, and hex view do not mutate on paste.

The terminal guard disables paste mode on normal exit and unwinding, and the
panic hook also restores it. Unsupported legacy Windows consoles keep the
previous keystroke behavior. No dependencies or save guarantees change.

## Real-terminal measurements

The frozen before build includes the preceding ASCII typing and plain-text
coloring changes. Both builds run through real 120×40 pseudo-terminals on Apple
M3 Pro/macOS, with isolated configurations and update checks disabled. Jobs run
serially in seeded shuffled order: 15 measured rounds after two excluded
warmups, **204 byte-verified sessions** across four paste sizes and two controls.

The driver follows the terminal's advertised paste mode. Before, it sends
ordinary characters and CR for Enter; after, it wraps the unchanged clipboard
text in bracketed-paste markers. Every run saves exactly the same expected file
bytes and must actually write the destination. Automatic indentation and
wrapping are disabled in both benchmark configurations.

Times below are median **paste + save + exit**, excluding startup and a fixed
30 ms settling period. They include durable file writes; these are not isolated
insertion timings or measurements of graphical terminal latency.

| Pasted text | Before | After | Speedup |
| --- | ---: | ---: | ---: |
| 4 KiB | 12.22 ms | 5.36 ms | 2.3× |
| 32 KiB | 52.71 ms | 7.86 ms | 6.7× |
| 256 KiB | 310.21 ms | 26.39 ms | 11.8× |
| 1 MiB | 1,267.39 ms | 78.27 ms | 16.2× |

The 1 MiB task removes **1.19 seconds** of waiting. Child CPU for that task
falls from 1,262.96 to 28.06 ms; full-session terminal output falls from
455,947 to 3,456 bytes. Peak RSS is 7.00 → 7.30 MiB. Paste events hold the
complete pasted text in memory, so their temporary memory use grows with size.

Paired bootstrap 95% after/before time-ratio intervals are 0.356–0.488 (4 KiB),
0.134–0.172 (32 KiB), 0.068–0.090 (256 KiB), and 0.055–0.064 (1 MiB).

| Control, including save and exit | Before | After |
| --- | ---: | ---: |
| Type 256 ordinary keys; 1,000 records | 5.596 ms | 5.599 ms |
| Save unchanged; 1,000 records | 5.074 ms | 4.928 ms |

Control ratio intervals include 1: 0.884–1.074 for ordinary typing and
0.924–1.304 for unchanged saves. This pass establishes a paste improvement,
with no established timing change for those controls.

## Verification and limits

- 390 Rust tests pass in debug and release; two explicit live-network checks
  remain ignored. Clippy with all targets/features and denied warnings,
  formatting, and whitespace checks pass.
- Shared insertion retains exhaustive existing clipboard tests for cursor
  positions, tabs, wide/combining characters, CRLF splits, undo, and exact bytes.
- New tests cover literal control characters, read-only/Help protection,
  single-step undo/redo, prompt filtering, Find/Replace, and ignored confirmations.
- Seven additional real-PTY checks exercise literal controls, Unicode cursor
  placement, undoing a large paste, Find, ignored quit confirmations, read-only
  mode, fragmented UTF-8/paste bodies/end markers, and terminal-mode cleanup.

One transport limitation remains in the existing Crossterm parser: if the
paste opener's initial Escape byte arrives alone, it can be interpreted as an
Escape key instead of the start of a sequence. An artificial test delivering
the opener one byte at a time reproduced this. A complete opener followed by
one-byte body/end-marker writes passes. Delayed or split opening markers are
not fixed by this pass; the failure trace is retained for follow-up.

Raw samples and hashes are in `target/editor-compare/paste-before-after.json`,
with paired intervals in `paste-statistics.json`. Frozen builds, source
snapshots, replay/verification drivers, logs, and the split-opener failure trace
are retained under `target/editor-compare/paste-pass/`.

Tests use macOS PTYs, not manual clipboard interaction in a graphical terminal.
Other operating systems and slow-link transport behavior are not validated.
Terminals without bracketed-paste support retain keystroke pasting. This is
local unreleased work.
