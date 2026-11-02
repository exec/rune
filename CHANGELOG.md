# Changelog

All notable changes to rune are documented here. The format follows
[Keep a Changelog](https://keepachangelog.com/) loosely; versions follow
[Semantic Versioning](https://semver.org/).

## [Unreleased]

### Changed

- Extract shared documents, undo, safe file I/O, search, and neutral syntax into the independent `rune-core` workspace crate. Rune retains compatibility imports; terminal styles and mouse-event adaptation remain in the terminal frontend. Etch can depend directly on the engine.

## [1.7.0] - 2026-11-02

### Added

- A headless document API with character-indexed selections and edits, shared undo/save protections, neutral highlight roles, and optional terminal dependencies for graphical frontends such as Etch.
- A source-only Editor Bench tool that fetches checksum-pinned nano/Vim sources, builds private competitors, verifies timed tasks, and scores latency against fixed weighted budgets.
- Handle terminal bracketed pastes as literal text blocks, with one-step undo and paste support in text prompts.
- A real-terminal comparison harness for Rune, GNU nano, and Vim, with verified editing tasks, raw timing/resource measurements, and optional stack profiles.
- Select an explicit settings file with `--config PATH` for isolated configurations.

### Performance

- Scan ordinary literal searches in bounded text blocks instead of invoking the matcher separately for every line, reducing live-search pauses in large logs.
- Insert terminal pastes in one bulk edit instead of dispatching each character and repeatedly redrawing the screen.
- Draw cached printable ASCII directly into screen cells without Unicode grapheme parsing during redraws.
- Process up to 256 queued ordinary keys within the existing 8 ms budget, avoiding extra intermediate redraws during fast input.
- Load text through a bounded read buffer instead of retaining a full-file string alongside the rope, reducing large-file opening memory.
- Use shorter column-only cursor commands for gaps on a known terminal row, reducing ASCII and Unicode output traffic.
- Track printed Unicode glyph widths when emitting terminal cells, avoiding redundant cursor-position commands while retaining explicit positioning for uncertain emoji widths.
- Handle queued ordinary editing and navigation keys in bounded bursts, preserving per-key viewport updates while avoiding intermediate redraws.
- Retain unchanged rendered and highlighted lines during typing, deletion, and Tab insertion; coalesce pending invalidations into bounded line ranges and skip redundant syntax resets.
- Draw hex views directly into terminal cells without allocating per-byte strings, span lists, or formatted offset strings.
- Batch dense indentation edits to short selected lines in one pass.
- Use bulk string copies for quoted text and block comments, and select syntax-cache eviction victims without sorting every entry.
- Copy clipboard text directly from rope chunks, borrow single clipboard entries during paste, and avoid rescanning ASCII inline pastes to position the cursor.
- Stop interactive replacement lookup at the first match without scanning for a second, and select the next Find result with binary search.
- Reject impossible fuzzy matches using cached character summaries, score short prefix matches from cached bytes, and skip ranking work for empty queries.
- Advance right-arrow movement from cached cursor positions without measuring the entire line after edits.
- Stop fuzzy candidate scans once the full query has matched, and copy line-comment tails as contiguous text.
- Stop vertical cursor clamping once the requested column is reached, and cache lines unaffected by active search or selection.
- Ignore passive mouse movement and button-release events instead of requesting unchanged redraws.
- Scan wrapped Unicode lines once per visible window instead of once per screen row.
- Inspect only comment prefixes when uncommenting and skip line-width scans when the cursor is already at column zero.
- Stream bracket matching and word navigation directly from the rope instead of seeking or copying every character.
- Find free untitled tab names with one name index and build tab labels only for the visible portion of the tab bar.
- Slice highlighting spans instead of rebuilding characters, and retain both selection endpoints to avoid repeated long-line scans.
- Reuse cursor conversion positions in both directions and retain exact positions during repeated backspace.
- Limit wrapped viewport navigation to the visible screen instead of scanning the intervening document.
- Borrow contiguous search lines, traverse rope lines sequentially, and reuse compiled search patterns across mode-aware queries.
- Stream save verification and atomic writes without allocating full-file byte buffers.
- Share rendered line caches between wrapped and horizontal views and draw cached ASCII text without copying spans.
- Borrow syntax identifiers and word-completion prefixes instead of allocating temporary copies; use prepared fuzzy candidates while rendering.
- Rebuild dense bulk replacements in one pass while retaining the existing sparse-edit path.
- Resolve duplicate tab filenames in one counting pass instead of comparing every pair of tabs.

### Fixed

- Allow tab shortcuts while searching, replacing, entering a filename, or viewing hex; document Terminal.app's Option-as-Meta setup.
- Render `.txt`, unrecognized file types, and untitled buffers without generic syntax colors; retain search and selection highlights.
- Refresh cached rendered colors when the syntax language changes, including after Save As.
- Invalidate cursor position caches before placing the cursor after cutting a selection.
- Align the cursor with inserted text after typing inside a tab or wide character.
- Invalidate cached line widths before clamping the cursor after uncommenting or unindenting.

## [1.6.0] - 2026-10-21

### Fixed

- Protect unsaved buffers when opening files and reset undo/search/selection state on load.
- Confirm overwriting Save As destinations and refuse saving over external file changes.
- Preserve symbolic links when atomically saving their targets.
- Prevent replacement crashes when edits remove the cursor's line.
- Bound shell execution and pipe cleanup by one deadline, terminating descendant processes on timeout.
- Render text following block comments and highlight the actual length of regex matches.
- Preserve combining-character typing order and CRLF line endings during editing.
- Replace all occurrences beyond the search display limit; resume interactive replacement without rescanning earlier lines.
- Bound search-result allocation while scanning long lines and preserve Unicode source offsets in case-insensitive searches.
- Scroll within tall wrapped lines, map mouse clicks to wrapped text, preserve wheel scrolling, and implement drag selection.
- Position the cursor correctly after pasting a trailing newline.
- Persist option changes immediately and report settings write errors.
- Avoid a debug-build panic when the hex pane has zero height.
- **Editor no longer hangs on CRLF files.** Two width functions disagreed about
  control characters (`UnicodeWidthStr` counts a lone `\r` as 1 column,
  `UnicodeWidthChar` reports 0), so pressing Right at the end of a CRLF line
  spun forever at 100% CPU and every unsaved buffer was lost.
- **Panic when a selection mark outlived its line.** `Delete` did not clear the
  mark, so deleting a newline left it naming a line the rope no longer had.
- **`Alt+;` destroyed code on lines with multi-byte indentation.** A byte offset
  was passed to a char-indexed rope operation, so `　// hello` uncommented to
  `　//llo`, and some inputs panicked outright.
- **Panic leaving hex view with the cursor inside a multi-byte character.**
- **Backspace corrupted state after clicking inside a wide char or tab.** Debug
  builds panicked on underflow; release builds wrapped silently and left the
  cursor on a nonexistent line.
- **Undo, redo, and opening a file showed stale text.** The render cache is keyed
  on a generation counter that none of the three bumped.

### Security

- Update the updater's TLS dependencies to patched rustls 0.23.46, rustls-webpki 0.103.16, and webpki-roots 1.0.10.
- **Backup on save no longer writes through a hostile `<path>~`.** The previous
  `fs::copy` had create+truncate semantics, so a pre-existing backup path owned
  by another user was truncated *in place* — preserving their inode, and landing
  a `0600` file's contents somewhere they could `chmod` and read. A FIFO at the
  same path blocked the UI thread forever in `open(O_WRONLY)`. Backups are now
  written by unlinking and then opening with `create_new` (`O_CREAT|O_EXCL`),
  which refuses to follow anything left in the way, closes the TOCTOU window in
  the old symlink-only guard, and inherits the source file's mode.
- **Terminal escape sequences from file content no longer reach the terminal.**
  ratatui's `Paragraph` does not filter control characters (unlike
  `Buffer::set_stringn`); it only skips zero-*width* graphemes, and a lone `ESC`
  measures as width 1. Opening a hostile file — including with `--view` — let it
  rewrite the window title, forge a status bar over rune's own UI, and emit
  terminal queries whose answerback is decoded as synthetic keystrokes. Control
  characters are now stripped at the render boundary for buffer text, status
  messages, and tab titles.

### Documentation

- Corrected two wrong keybindings in the README: replace is `Ctrl+\` (not
  `Ctrl+H`, which opens help), and redo is `Ctrl+R` (not `Ctrl+Y`, which is
  page-up).

## [1.5.2]

### Added
- Notify-only update checker. On startup, a background thread checks the
  GitHub releases API at most once per 24h (cached in
  `~/.cache/rune/update.json`) and surfaces a status-bar notice if a newer
  version is available. Disable with `check_for_updates = false` in config.

## [1.5.1]

### Fixed
- **Atomic save** — `Ctrl+S` now writes via a sibling temp file +
  `fsync` + rename, so a crash, panic, or power loss mid-save can no
  longer truncate or corrupt the user's file. Unix file modes are
  preserved across the swap.
- **Panic-safe terminal cleanup** — a `Drop` guard plus a `panic::set_hook`
  now restore raw mode, alt-screen, and mouse capture even if the editor
  panics mid-run. Previously, an unwinding panic could leave the terminal
  unusable until `reset`.
- **`Ctrl+K` / `Ctrl+U` redraw** — cut and paste-inline mutated the buffer
  but didn't request a redraw, so the change wasn't visible until the next
  keystroke. Both now repaint immediately.

## [1.5.0]

### Added
- Comprehensive syntax highlighting for 26 languages.
- Criterion benchmark harness (`benches/editor_perf.rs`) covering load,
  edit, render, search, and syntax paths.
- CLI flags: `+LINE,COL` positioning, `--view`, `--line-numbers`,
  `--word-wrap`, `--no-mouse`.
- Word-completion cycling.
- Save warning when the target path would create new directories.
- Find/replace integration tests.

### Changed
- **Performance pass** — replace is now ~325× faster on 100k-line files
  (rope-native), search ~40% faster (line cache + regex reuse), syntax
  cold-highlight ~37% faster (`phf` keyword maps), render frame ~14–17%
  faster (cached span layout), typing latency ~22% lower (single-cell
  width cache).
- Undo stack now uses `VecDeque` (O(1) front removal, was O(n)).
- Tab bar click handler respects scroll offset.
- Many byte-vs-char-vs-display-column correctness fixes across search,
  replace, word completion, and cursor movement.

For changes prior to 1.5.0, see the git history: `git log v1.4.1`.
