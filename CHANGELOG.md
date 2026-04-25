# Changelog

All notable changes to rune are documented here. The format follows
[Keep a Changelog](https://keepachangelog.com/) loosely; versions follow
[Semantic Versioning](https://semver.org/).

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
