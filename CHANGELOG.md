# Changelog

All notable changes to rune are documented here. The format follows
[Keep a Changelog](https://keepachangelog.com/) loosely; versions follow
[Semantic Versioning](https://semver.org/).

## [Unreleased]

### Security
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

### Fixed
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
