# Rune Code Review — 2026-06-11

Full read of `src/` (editor, tabs, input, ui, search, syntax, hex, fuzzy,
config, updater, main) at v1.5.3. Every Critical/High finding below was
**empirically verified** with repro tests in `tests/review_repro.rs`
(`cargo test --test review_repro -- --ignored` — each test asserts the
*correct* behavior and currently fails).

> **Resolution status (2026-06-11):** all findings below are fixed on this
> branch except **L8** (Ctrl+H help-vs-backspace is a keybinding product
> decision) and **L12** (panic-hook/updater ordering, documented behavior).
> The repro tests in `tests/review_repro.rs` are no longer `#[ignore]`d and
> run as part of the normal suite.

Overall the architecture is solid: the atomic-save path (temp file + fsync +
rename + permission preservation, `tabs.rs`), the RAII terminal guard + panic
hook (`main.rs`), the undo design (rope clones are cheap with ropey), the
search-match cap, and the careful background updater are all well done. The
bugs cluster in three areas: **prompt/keybinding edge cases, the shell-execute
pipeline, and Unicode column math**.

---

## Critical

### C1. Panic: fuzzy finder crashes on multibyte tab names — `ui.rs:1101-1103`
```rust
let truncated = if label.len() > (width as usize).saturating_sub(2) {
    format!("{}...", &label[..(width as usize).saturating_sub(5)])
```
`label.len()` is bytes and the slice is a byte index. Opening `Ctrl+P` with a
tab whose name contains non-ASCII (e.g. a Japanese filename) panics:
`byte index 45 is not a char boundary; it is inside '名'`. Verified — the
whole editor dies (unsaved work in *all* tabs is lost). Truncate on a char
(ideally display-width) boundary instead.

### C2. Panic risk: 1 MB shell-output truncation — `input.rs:1161-1163`
```rust
if stdout.len() > MAX_OUTPUT {
    stdout.truncate(MAX_OUTPUT);
```
`String::truncate` panics if the cut lands inside a multi-byte character.
Any `Ctrl+E` command emitting >1 MB of UTF-8 with a multibyte char straddling
the boundary kills the editor. Use a char-boundary-safe truncation (e.g. walk
back with `is_char_boundary`).

### C3. Data loss: “Save and close” discards untitled buffers — `input.rs:156-170`
In `handle_confirm_close_tab`, `Y` only saves `if let Some(path)`; for an
untitled buffer it falls straight through to `close_tab()`. So Alt+W → “Save
modified buffer before closing?” → **Y** silently throws the buffer away.
Verified. The Ctrl+Q path (`handle_quit_confirmation`) gets this right by
switching to filename input with `quit_after_save` — `ConfirmCloseTab` should
do the same.

### C4. Unbound Ctrl/Alt chords insert literal text — `input.rs:933-935`
The fall-through editing arm `(_, KeyCode::Char(c))` matches *any* modifiers,
so every Ctrl/Alt combination that isn’t explicitly bound silently types its
letter: Ctrl+J inserts `j`, Alt+X inserts `x`, etc. Verified. A typo’d chord
corrupts the buffer with no feedback. Guard the insert arm with
`!key.modifiers.intersects(CONTROL | ALT)` (keep SHIFT for capitals).

### C5. Stale screen: render cache survives tab close — `ui.rs:19-56, 423-430`
`RenderCache` is keyed by `(&editor.rope as *const _ as usize, len_chars,
dirty_generation, show_whitespace)`. After `tabs.remove(i)`, `Vec` shifts the
next `Editor` into the same memory slot — same address — and two freshly
loaded buffers both have `dirty_generation == 0`. If their `len_chars` happen
to match, the editor **renders the closed file’s contents** for the new tab.
Verified with a `TestBackend` repro (screen shows `AAAA` from the closed tab
instead of `BBBB`). Key the cache on a unique per-`Editor` id (monotonic
counter assigned in `new_buffer()`) instead of a memory address.

---

## High

### H1. Shell execute can deadlock the editor / falsely “time out” — `input.rs:1095-1155`
Two pipe-handling problems in `handle_confirm_execute`:

1. **stdout is never drained while polling.** The loop calls `try_wait()` and
   only reads output after exit. A child writing more than the pipe buffer
   (~64 KB) blocks on a full pipe, never exits, gets killed at the 10 s
   timeout — so any command with moderately large output reports “Command
   timed out” and the output is discarded.
2. **stdin is written on the editor thread before the timeout loop.** Piping a
   large selection (> pipe buffer) into a command that doesn’t promptly read
   stdin makes `stdin.write_all()` block **forever** — the UI freezes and the
   10 s timeout never even starts.

Drain stdout/stderr on threads (or write stdin from a thread) and join with
the timeout.

### H2. Wide-character typing scrambles text — `editor.rs:205-221`
`insert_char` advances the cursor by `1` display column regardless of the
character’s width. After typing `あ` (width 2) the cursor sits *inside* the
glyph, so the next `line_col_to_char_idx` resolves to the wrong index: typing
`あ`, `x`, `y` produces **`あyx`**. Verified. Advance by
`UnicodeWidthChar::width(c)` instead; audit `delete_char` (moves left 1 col
after deleting a wide char) and the verbatim-Tab branch (`input.rs:1010-1018`)
for the same assumption.

### H3. Hard tabs break all column math — pervasive
`unicode-width` gives `'\t'` width `None → 0`, so any line containing a real
tab (opened from disk, or inserted via Alt+V) has wrong display widths:
cursor lands in the wrong place, horizontal slicing/cursor x drift, and
`show_whitespace` rendering (`→`, width 1) disagrees with normal rendering
(width 0). There is no tab-expansion in the render path at all. Either expand
tabs to the configured `tab_width` in both width math and rendering, or
convert on load.

### H4. Replace ignores regex & case-insensitivity — `editor.rs:643-729`
`perform_replace` (Replace All) and `perform_replace_interactive` do literal,
case-**sensitive** `str::find`, while Find honors `use_regex` and
`case_sensitive` (`search.rs`). Verified both ways: with regex mode on, Find
highlights 2 matches for `a.c`, then `A` replaces **0**; with the default
case-insensitive search, Find shows 2 matches for `hello` in `"Hello hello"`,
Replace All replaces **1**. Route replacement through `SearchState`’s match
machinery.

### H5. Read-only mode doesn’t cover all mutation paths — `input.rs:725-861`
In `--view` mode (and `[Help]` tabs): `Ctrl+\` opens the full Replace flow
(verified) which mutates the buffer; `Alt+\` word-completion also mutates.
Both lack the `!is_read_only` guard that cut/paste/indent/etc. have.

---

## Medium

### M1. Interactive replace can’t skip and can loop forever — `editor.rs:682-729`, `input.rs:639-687`
`perform_replace_interactive` always scans **from the top of the document**,
ignoring the cursor. Consequences: (a) “N: Skip” doesn’t skip — it just exits
the dialog, so there is no way to keep one occurrence and replace later ones;
(b) if the replacement still contains the pattern (`a` → `aa`), every `Y`
re-matches at the same spot — no forward progress, ever. Track a “search from
here” position that advances past each handled match.

### M2. One Tab press needs 5 undos — `tabs.rs:903-914`
`handle_tab_insertion` calls `save_undo_state()` and then `insert_char()` per
space, each of which saves *another* undo state. Verified: Tab inserts 4
spaces; one `Ctrl+Z` removes one space. Insert the spaces as a single rope
insert (also avoids 4 separate cache invalidations).

### M3. Tab-bar mouse click bypasses mode reset — `tabs.rs:946-967`
`handle_tab_bar_click` assigns `active_tab` directly without
`reset_editor_mode_on_tab_switch()` (which Alt+arrow / Ctrl+PgUp paths use).
Clicking a tab while in HexView leaves `input_mode = HexView` against a tab
with `hex_state = None`: blank editor area, and pressing Esc then *enters*
hex view on the new tab. Same for Find/Replace prompt states.

### M4. Mouse clicks on status/help rows move the text cursor — `tabs.rs:921-944`, `editor.rs:489-526`
Only the tab-bar row is excluded; rows in the status/help area (bottom 2
lines) map to document lines and move the cursor. Also `ScrollDown` clamps
against the full terminal height rather than the editor height (off by 3),
and the click handler treats the click column as a display column while
`clicked_col` mixes in `viewport_offset.1` without width awareness (ties into
H2/H3).

### M5. Explicitly opening a binary file silently does nothing — `main.rs:53-94`
`expand_paths` drops `is_binary` files even when the user names them
explicitly: `rune image.bin` starts with an empty untitled buffer and no
message. Given the editor ships a hex view, explicitly-named files should
load (or at least error visibly); the binary filter makes sense only for
directory expansion.

### M6. Syntax-cache invalidation defeats itself — `syntax.rs:536-540`
```rust
pub fn invalidate_cache_from_line(&mut self, start_line: usize) {
    self.line_cache.retain(|&line_num, _| line_num < start_line);
    self.file_version += 1;
}
```
`highlight_line` only accepts entries whose `version == file_version`, so the
bump makes the *retained* entries stale too — every edit effectively clears
the whole cache. This silently undoes the DPERF-2 optimization claimed in
AUDIT.md. Either don’t bump the version here, or stamp retained entries.

### M7. Stale width cache consulted mid-mutation — `editor.rs:665-677, 712-723`
`perform_replace`/`perform_replace_interactive` call `clamp_cursor_to_line()`
*after* mutating the rope but *before* `mark_document_changed()` clears
`line_width_cache`, so the clamp can use a pre-edit width. Reorder (invalidate
first, then clamp).

### M8. Search match columns are char offsets used as display columns — `editor.rs:599-604, 616-636`
`SearchState` stores `(line, char_col)` (correct for rendering), but
`perform_find`/`find_next_match` assign that char offset directly to
`cursor_pos.1`, which everywhere else is a *display* column. Wrong cursor
placement on any matched line containing wide chars or tabs. Convert with
`char_idx_to_display_col` when jumping.

---

## Low / polish

- **L1** `input.rs:322-343` — the filename prompt is the only prompt without a
  Ctrl+C cancel binding; it also accepts chars with CONTROL held (ties to C4).
- **L2** `tabs.rs:166-180` — untitled naming can duplicate: with only
  `[untitled-2]` open, a new tab is also named `[untitled-2]`.
- **L3** `tabs.rs:504-530` — `undo()`/`redo()` flash “Undo”/“Redo” even when
  the stack is empty (and mark redraw unconditionally).
- **L4** `input.rs:123-132` — hex-view scrolling hardcodes `visible_rows = 20`;
  `draw_hex_view` recomputes correctly from the real area, so this is dead but
  conflicting logic. Pick one owner for scroll state.
- **L5** `tabs.rs:946-967` / `ui.rs:301-309` — tab-bar hit-testing and widths
  use `String::len()` (bytes) as display width; multibyte tab names misclick
  (same names as C1).
- **L6** `ui.rs:945-955` — `get_syntax_style_at_position` compares a *char*
  position against *byte* lengths; wrong syntax style around multibyte chars
  in search-highlighted lines.
- **L7** `search.rs:84-90` — `find_all_matches` builds a per-call
  `HashMap<usize, String>` “line cache” though each line is visited exactly
  once; it’s pure overhead.
- **L8** `input.rs:754` — Ctrl+H opens Help, but many terminals send Backspace
  as `^H`; on those, Backspace opens a Help tab instead of deleting (nano
  avoids ^H for exactly this reason).
- **L9** `input.rs:1153-1207` — executed commands’ stderr is discarded and the
  exit status is ignored: a failing command inserts nothing and still reports
  “Executed: …”. Consider surfacing non-zero exits and/or stderr.
- **L10** `tabs.rs:772-794` — `paste()` leaves the cursor *above* the pasted
  lines (nano places it after); also `lines_pasted.max(1)` reports “1 line”
  for an inline fragment.
- **L11** `ui.rs:594-600` — in word-wrap mode, a cursor at exactly
  `line_width % content_width == 0` (end of a full row) computes a sub-row one
  past the rendered rows and draws on the next document line’s row.
- **L12** `main.rs:97-114` — a real file literally named `+123` is consumed as
  a line-number argument and never opened (`./+123` works; worth a doc note at
  most).
- **L13** `editor.rs:603` — `perform_find` pins the matched line to the *top*
  of the viewport (`viewport_offset.0 = line`) instead of just scrolling it
  into view; jarring when the match is already on-screen.

---

## Verified-good (checked, no action)

- `atomic_write` (`tabs.rs:983-1034`): temp-in-same-dir, `sync_all`,
  permission preservation, cleanup on failure — correct. (Only nit: no
  directory fsync after rename, which is beyond what most editors do anyway.)
- Terminal restore on panic: both the RAII guard and the panic hook.
- `updater.rs`: cache TTL, silent failure policy, no stdout writes, escaped
  JSON writing — all sound.
- Search history, match cap + truncation indicator, regex compile caching.
- `fuzzy_filter` (render) vs `fuzzy_filter_prepared` (Enter) produce the same
  ordering (same scores, stable sort) — selection matches what’s drawn, though
  consolidating the two would remove the footgun.

## Suggested fix order

1. C3, C4 (small input.rs patches, stop active data corruption/loss)
2. C1, C2 (both are one-line boundary-safe truncations)
3. C5 (unique buffer id for the render cache)
4. H1 (threaded pipe I/O), H4 (replace parity), H5 (read-only guards)
5. H2/H3 + M8 as one “column math” pass — they share the display-vs-char
   confusion and are best fixed together with regression tests
6. The rest as convenient; M6 is a free perf win.
