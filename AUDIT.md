# Rune v1.4.0 Code Audit

> Collaborative review performed by a team of specialized reviewers.
> Date: 2026-04-11
> Status update: 2026-04-12 (verified against v1.4.1)
> Second update: 2026-04-12 (post-perf-pass, benchmark-validated)

## Status Update (2026-04-12, post-perf-pass)

Following a second audit pass focused purely on performance (3 specialist agents reading editor core, render path, and algorithms), a Tier-1 implementation wave landed all the biggest wins. Measured against a criterion baseline:

- **`perform_replace/100k lines`: 7.43s → 22.85ms (−99.7%, ~325× faster)** — closes ARCH-9 / SEC-5.
- `perform_replace/10k lines`: 71.6ms → 2.00ms (−97.1%).
- `search_literal` across 1k–50k lines: **−40%** from line cache + char-vec elimination.
- `word_complete_first_scan`: 2.02ms → 1.35ms (−33%).
- `insert_char_typing`: 94.7µs → 73.3µs (−22%) after single-cell width cache reworked.
- `cursor_navigation/right_500_in_long_line`: **36× faster** than uncached.

All 125 tests still pass. A criterion bench suite (`benches/editor_perf.rs`) now guards against regressions.

| Category | Done | Not done | N/A |
|---|---|---|---|
| Security (12) | 9 | 1 | 2 |
| Correctness (7) | 7 | 0 | — |
| Performance (6) | 5 | 1 | — |
| Architecture (17) | 11 | 3 | 3 |

### Per-item status

| ID | Status | Evidence |
|----|--------|----------|
| SEC-1 | DONE | `InputMode::ConfirmExecute` confirmation prompt (`input.rs:1045`) |
| SEC-2 | DONE | `try_wait` polling + kill on timeout (`input.rs:1130-1155`) |
| SEC-3 | DONE | 1MB output cap + truncation warning (`input.rs:1158-1169`) |
| SEC-4 | DONE | `VecDeque` for undo stack, O(1) `pop_front` (`editor.rs:29`) |
| SEC-5 | DONE | `perform_replace*` now in-place rope mutate, no rebuild (`editor.rs`) |
| SEC-6 | DONE | Single `to_string()` in `toggle_hex_view` (`editor.rs:529`) |
| SEC-7 | NOT DONE | `create_dir_all` still silent on save (`tabs.rs:322`) |
| SEC-8 | N/A | Mitigated by `regex` crate design |
| SEC-9 | DONE | Rope-iteration `word_complete` (`editor.rs:1026-1038`) |
| SEC-10 | DONE | `debug_assert` guards in `active_editor` (`tabs.rs:89-94`) |
| SEC-11 | DONE | `fuzzy_selected` clamped (`input.rs:989`) |
| SEC-12 | N/A | Silent backup failure accepted as design |
| COR-1 | DONE | Byte→char conversion in `find_matches_in_line` (`search.rs:204-206`) |
| COR-2 | DONE | `word_complete` uses display-col conversion (`editor.rs:996-1004`) |
| COR-3 | DONE | `move_word_*` convert char idx → display col (`editor.rs:790-817`) |
| COR-5 | DONE | Mouse click subtracts gutter + scroll (`editor.rs:475-477`) |
| COR-6 | DONE | Tab bar click respects `tab_scroll_offset` (`tabs.rs:905-926`) |
| COR-10 | DONE | `perform_replace_interactive` byte→char fix (`editor.rs:668`) |
| COR-11 | DONE | Auto-indent uses `UnicodeWidthStr::width` (`editor.rs:252`) |
| PERF-1 | DONE | `toggle_hex_view` + `word_complete` deduped (`editor.rs:514, 1026`) |
| PERF-2 | DONE | `collect_span_chars_range` for word-wrap sub-rows (`ui.rs`) |
| PERF-3 | DONE | `slice_spans_horizontal` iterates without per-span `Vec<char>` (`ui.rs`) |
| PERF-5 | DONE | `FuzzyCandidate` pre-lowercase + `fuzzy_filter_prepared` (`fuzzy.rs`) |
| PERF-6 | DONE | Selection range computed once per frame, passed in (`ui.rs`) |
| PERF-7 | DONE | Binary-searched match slice per line, no per-line filter+sort (`ui.rs`) |
| ARCH-1 | DONE | Input-mode split-brain fixed (commit `d605d87`) |
| ARCH-2 | DONE | `filename_buffer` → `input_buffer` (commit `d605d87`) |
| ARCH-3 | DONE | `char_idx_to_display_col` extracted (`editor.rs:319-329`) |
| ARCH-4 | NOT DONE | `handle_find` repetition not refactored |
| ARCH-5 | DONE | Same fix as COR-6 (`tabs.rs:905-926`) |
| ARCH-6 | NOT DONE | Rope line-to-string pattern still duplicated |
| ARCH-7 | DONE | `set_temporary_status_message` standardized (23 uses in `tabs.rs`) |
| ARCH-8 | DONE | `perform_save` returns `Err` on write failure (`tabs.rs:320`) |
| ARCH-9 | DONE | `perform_replace` uses rope-native `remove`+`insert` with incremental byte→char tracking (`editor.rs`) |
| ARCH-10 | NOT DONE | No find/replace integration tests |
| ARCH-11 | N/A | Naming collision accepted |
| ARCH-12 | N/A | `input.rs` size acceptable |
| ARCH-13 | DONE | Arrow keys restricted to `KeyModifiers::NONE` (commit `d605d87`) |
| ARCH-14 | DONE | `cached_text`/`cache_valid` removed |
| ARCH-15 | N/A | Silent backup failure accepted |
| ARCH-16 | DONE | Tests added for `syntax` & `config` (commit `c211ba4`) |
| ARCH-17 | DONE | Word-completion cycling (commit `ad3852c`) |

### Remaining work

- **SEC-7** — warn user when save would create new directories.
- **ARCH-4** — extract `update_find_status` helper from `handle_find`.
- **ARCH-6** — extract rope line-to-string helper.
- **ARCH-10** — integration tests covering full find/replace state machine.

### New work surfaced by perf pass (second audit, 2026-04-12)

Additional fixes landed beyond the original audit's scope, all measured via `benches/editor_perf.rs`:

- **Regex cache** on `SearchState` — reuses compiled `Regex` when pattern unchanged (keystroke latency win).
- **Per-search line-string cache** in `find_all_matches` — eliminates duplicate `get_line_str` fallback on fragmented ropes (-40% search).
- **`validate_match_at_position` char-vec elimination** — direct string slicing, single lowercase per call.
- **`perform_save` uses `rope.bytes()`** instead of `rope.to_string()`.
- **Incremental display-column tracking in `move_word_*`** — single forward pass instead of two.
- **Single-cell `Cell<Option<(usize,usize)>>` line-width cache** — replaces HashMap after it regressed the typing path; single-line hits near-free, misses cost one comparison.
- **Inlined cursor advance in `insert_char`** — bypasses `move_cursor_right`'s cache lookup on the typing hot path (-22% typing).
- **`word_complete` rope-iteration** — drops per-line `String` allocation; single `word_buf` reused across entire scan.

---

## Executive Summary

Rune is a Rust CLI text editor built with `ratatui` and `ropey`. The codebase demonstrates strong Rust safety practices -- zero `unsafe` blocks, no `unwrap()`/`expect()`/`panic!()` calls, and consistent use of `Result` propagation. The editor has been modularized into `editor.rs`, `input.rs`, `ui.rs`, `search.rs`, `tabs.rs`, `config.rs`, `syntax.rs`, `hex.rs`, and `fuzzy.rs`, which is a significant improvement over earlier monolithic versions.

**Top concerns:**

1. **Command injection with no timeout** -- The execute command feature (`Ctrl+E`) passes user input directly to `sh -c` with no confirmation, no timeout, and no output size limit. A long-running or infinite command hangs the editor indefinitely with no recovery path (SEC-1, SEC-2, SEC-3).
2. **Pervasive byte-vs-char-vs-display-column confusion** -- Search, replace, word completion, and cursor movement functions conflate byte offsets, char indices, and display columns, causing incorrect behavior and potential panics with non-ASCII text (COR-1, COR-2, COR-3, COR-5).
3. **Unnecessary full-document materialization** -- Multiple hot paths call `rope.to_string()` when rope-native operations would suffice, causing O(n) allocation for large files (PERF-1).
4. **Tab bar click handler ignores scroll offset** -- Clicking tabs when scrolled selects the wrong tab (COR-6).
5. **Architectural friction points** -- `input_mode` split-brain between TabManager and Editor, overloaded `filename_buffer`, duplicated char-to-display-col conversions, and inconsistent status message handling (ARCH-1, ARCH-2, ARCH-3, ARCH-7).

**Positive observations:**

- Zero `unsafe` code -- full memory safety guaranteed by the compiler.
- No panicking unwraps -- all error handling uses pattern matching or `?`.
- Clean module separation with TabManager/Editor split.
- Search history properly bounded (50 entries), undo stack capped at 100.
- Config loading gracefully falls back to defaults on missing or corrupt files.
- Terminal cleanup is robust across normal exit paths.
- Tab switching is efficient with no unnecessary recomputation.

## Critical & High Severity Findings

| ID | Category | Severity | Summary | File | Line(s) |
|----|----------|----------|---------|------|----------|
| SEC-1 | Security | Critical | Command injection via execute command with no confirmation or timeout | `src/input.rs` | 1096-1098 |
| SEC-2 | Security | High | Execute command blocks editor indefinitely on long-running commands | `src/input.rs` | 1123 |
| SEC-3 | Security | High | Unbounded command output can exhaust memory | `src/input.rs` | 1124-1125 |
| COR-1 | Correctness | High | Search match positions are byte offsets used as char positions | `src/search.rs` | 217-228 |
| COR-10 | Correctness | High | `perform_replace_interactive` same byte-vs-char bug | `src/editor.rs` | 639-648 |
| PERF-1 | Performance | High | `toggle_hex_view` calls `rope.to_string()` 3 times; `word_complete` materializes entire buffer | `src/editor.rs` | 499-528, 915 |
| ARCH-5 | Architecture | High | Tab bar click handler ignores scroll offset -- clicks select wrong tab | `src/tabs.rs` | 870-882 |

## Security & Robustness

### Critical

#### SEC-1: Command Injection via Execute Command
- **File:** `src/input.rs:1096-1098`
- **Description:** `handle_execute_command` passes user-supplied input directly to `sh -c` with no sanitization, confirmation prompt, or sandboxing. The command runs with full user privileges. While this is an intentional feature (like nano's `^R` or vim's `:!`), there are no safeguards against accidental destructive commands.
- **Fix:** Add a confirmation prompt before execution. Add a timeout to prevent indefinite hangs. Document the security implications.

### High

#### SEC-2: Execute Command Blocks Editor Indefinitely
- **File:** `src/input.rs:1123`
- **Description:** `child.wait_with_output()` is a blocking call. If the command never terminates (e.g., `sleep 999999`, any interactive command), the editor becomes completely unresponsive. Raw terminal mode is still active, so the user cannot Ctrl+C out easily. Recovery requires killing the process from another terminal.
- **Fix:** Use a timeout mechanism -- spawn the child in a thread and poll with a timeout, or use `wait_timeout` from the `wait-timeout` crate.

#### SEC-3: Unbounded Command Output Can Exhaust Memory
- **File:** `src/input.rs:1124-1125`
- **Description:** Command output is captured entirely into memory via `wait_with_output()`. A command producing large output causes OOM. The output is then inserted into the rope buffer, potentially creating an enormous document.
- **Fix:** Limit stdout capture to a reasonable size (e.g., 1MB) and warn the user if output was truncated.

### Medium

#### SEC-4: Undo Stack Clones Entire Rope on Every Edit
- **File:** `src/editor.rs:33-43`
- **Description:** Every edit operation clones the entire `Rope` into the undo stack. While `ropey::Rope::clone()` uses reference counting internally, the undo stack limit of 100 means up to 100 copies are kept. Additionally, `self.undo_stack.remove(0)` on line 42 is O(n) for Vec.
- **Fix:** Use `VecDeque` instead of `Vec` for O(1) front removal. Monitor memory for very large files.

#### SEC-5: Replace Functions Materialize Entire Rope to String
- **File:** `src/editor.rs:616, 637-641`
- **Description:** Both `perform_replace` and `perform_replace_interactive` call `self.rope.to_string()`, doubling memory usage for large files. The interactive replace also clones the string for a third copy.
- **Fix:** Use rope-native search/replace operations instead of converting to String.

#### SEC-6: `toggle_hex_view` Materializes Entire File Twice
- **File:** `src/editor.rs:520-528`
- **Description:** Calls `self.rope.to_string()` twice -- once to get bytes and once to compute byte offset. Wasteful for large files.
- **Fix:** Call `to_string()` once and reuse the result.

#### SEC-7: No Path Traversal Protection on Save
- **File:** `src/tabs.rs:270-273`
- **Description:** `perform_save` calls `create_dir_all(parent)` on whatever path the user provides, silently creating arbitrary directory trees on typos.
- **Fix:** Warn the user if the save path would create new directories.

#### SEC-8: Regex Denial of Service in Search (Mitigated)
- **File:** `src/search.rs:97-107`
- **Description:** User-supplied regex patterns are compiled and executed directly. The Rust `regex` crate uses finite automata to avoid catastrophic backtracking, so this is largely mitigated. Complex patterns could still cause slow compilation.
- **Fix:** Consider adding a size limit on the pattern string as defense in depth.

#### SEC-9: `word_complete` Materializes Entire Document
- **File:** `src/editor.rs:915`
- **Description:** `word_complete` calls `self.rope.to_string()` to scan for completion candidates on every Alt+\ press. O(n) allocation + O(n) scan per invocation.
- **Fix:** Use rope iteration instead of materializing the whole string.

### Low

#### SEC-10: `active_editor()` Can Panic on Index Out of Bounds
- **File:** `src/tabs.rs:80-85`
- **Description:** Direct indexing `self.tabs[self.active_tab]` without bounds checking. Generally safe due to careful invariant maintenance, but a bug in tab management could cause a panic.
- **Fix:** Add a debug assertion or use `.get()` with a fallback.

#### SEC-11: Fuzzy Finder `fuzzy_selected` Not Clamped
- **File:** `src/input.rs:1024-1026`
- **Description:** Pressing Down in the fuzzy finder increments `fuzzy_selected` without an upper bound. The selection indicator in the UI could go off-screen.
- **Fix:** Clamp to `filtered.len().saturating_sub(1)`.

#### SEC-12: Backup File Path Construction
- **File:** `src/tabs.rs:277`
- **Description:** Backup path is constructed by appending `~` to the display path. Backup copy failure is silently ignored.
- **Fix:** Consider logging backup failures.

### Info

- **Terminal state cleanup is robust** (`src/main.rs:127-130`) -- raw mode restored on all normal exit paths.
- **Config file parsing is graceful** (`src/config.rs:39-48`) -- falls back to defaults; `tab_width` clamped to minimum 1.
- **File loading uses `read_to_string`** (`src/editor.rs:169`) -- non-UTF-8 files fail gracefully; binary files pre-filtered by `is_binary()`.
- **`to_string_lossy` used for filenames** (`src/editor.rs:181`) -- correctly handles non-UTF-8 filenames.
- **Selection range handling is safe** (`src/tabs.rs:571-605`) -- uses `saturating_sub` to prevent underflow.
- **Internal clipboard only** -- no system clipboard interaction, no external data injection risk.
- **Zero `unsafe` code** -- full memory safety guaranteed by the compiler.
- **No `unwrap`/`expect`/`panic!` calls** -- all fallible operations use pattern matching or `?`.

## Architecture & Code Quality

### High

#### ARCH-5: Tab Bar Click Handler Ignores Scroll Offset
- **File:** `src/tabs.rs:870-882`
- **Description:** `handle_tab_bar_click` iterates all tabs from index 0, but `draw_tab_bar` (`src/ui.rs:238-342`) renders from `tab_scroll_offset` and adds overflow indicators. Clicking a visible tab when scrolled selects the wrong tab.
- **Fix:** Start iteration from `self.tab_scroll_offset` and account for the left overflow indicator width.

### Medium

#### ARCH-1: `input_mode` Split-Brain Between TabManager and Editor
- **File:** `src/tabs.rs:16`, `src/editor.rs:84-102`
- **Description:** `InputMode` includes per-editor modes (Find, Replace, GoToLine, HexView) but `input_mode` is a single field on `TabManager`. Switching tabs during a find operation loses the find state silently. The search state lives on each `Editor` but the mode driving the UI lives on `TabManager`.
- **Fix:** Move editor-specific modes to `Editor`, or save/restore mode on tab switch.

#### ARCH-2: `filename_buffer` on TabManager Is Overloaded
- **File:** `src/tabs.rs:18`
- **Description:** Used for filename input, fuzzy query input, AND execute command input. Works because only one mode is active at a time, but fragile and confusing. The fuzzy finder has its own `fuzzy_query` field, which is inconsistent.
- **Fix:** Rename to `input_buffer` or split into purpose-specific buffers.

#### ARCH-3: Char-Index-to-Display-Col Conversion Duplicated 5+ Times
- **File:** `src/tabs.rs:593-598`, `src/tabs.rs:746-751`, `src/editor.rs:506-512`, `src/editor.rs:851-857`, `src/editor.rs:881-883`
- **Description:** The pattern of converting a char index to a display column by iterating over chars and accumulating `UnicodeWidthChar::width()` is repeated at least 5 times.
- **Fix:** Extract a utility method `char_idx_to_display_col(line, char_offset) -> usize` on `Editor`.

#### ARCH-4: `handle_find` Is Very Long and Repetitive
- **File:** `src/input.rs:390-608`
- **Description:** ~218 lines with significant repetition in status message formatting. The pattern of cloning `search_buffer`, calling `perform_find`, then building a status message appears 4+ times.
- **Fix:** Extract a helper like `update_find_status(tabs)`.

#### ARCH-6: Rope Line-to-String Extraction Pattern Repeated 4+ Times
- **File:** `src/search.rs:66-74`, `src/search.rs:113-119`, `src/ui.rs:363-371`, `src/ui.rs:467-474`
- **Description:** The `as_str()` with fallback to `chars().collect()` pattern is duplicated throughout.
- **Fix:** Extract into a small helper function.

#### ARCH-7: Inconsistent Use of `set_temporary_status_message` vs Direct Assignment
- **File:** Multiple locations in `src/input.rs` and `src/tabs.rs`
- **Description:** Some places use `set_temporary_status_message(...)` (with auto-clear timeout), while others directly assign `status_message` (persists indefinitely). This means some messages linger and others auto-clear unpredictably.
- **Fix:** Standardize on `set_temporary_status_message` for all user-facing messages, or make direct assignment set a default timeout.

#### ARCH-8: `perform_save` Swallows Its Own Error
- **File:** `src/tabs.rs:270-310`
- **Description:** Returns `Ok(())` even when the write fails -- shows an error status message but the caller gets `Ok`. `handle_quit_confirmation` checks `editor.modified` to detect failure rather than the Result.
- **Fix:** Return `Err` on write failure so callers can rely on the Result type.

#### ARCH-9: `perform_replace` Rebuilds Entire Rope from String
- **File:** `src/editor.rs:610-629`
- **Description:** Calls `self.rope.to_string()`, does `String::replace`, then creates a new `Rope::from_str`. This loses the rope's efficient chunk structure and is O(n) in document size.
- **Fix:** Modify the rope in-place using rope-native operations.

#### ARCH-10: No Integration Tests for Search/Replace Flow
- **File:** `src/input.rs`
- **Description:** The find and replace handlers are complex state machines but only `search.rs` has unit tests for matching logic. The full flow has no integration test.
- **Fix:** Add integration tests covering the complete find/replace workflow.

### Low

#### ARCH-11: `toggle_hex_view` and `toggle_mark` Naming Collision
- **File:** `src/editor.rs:494-537` / `src/tabs.rs:484-493`, `src/editor.rs:662-668` / `src/tabs.rs:757-767`
- **Description:** Both `Editor` and `TabManager` have identically named methods for these features. The pattern is reasonable (TabManager wraps Editor) but the naming can be confusing.

#### ARCH-12: `input.rs` Is Large but Well-Organized
- **File:** `src/input.rs` (1194 lines)
- **Description:** Each handler is clearly separated by mode. Could be split into per-mode modules if it grows further, but not urgent.

#### ARCH-13: Arrow Keys Silently Consume All Modifiers
- **File:** `src/input.rs:924-953`
- **Description:** Arrow keys match `(_, KeyCode::Up)` etc., meaning Shift+Arrow is silently consumed. This blocks future selection-by-keyboard features.

#### ARCH-14: `cached_text` / `cache_valid` Fields Appear Unused
- **File:** `src/editor.rs:117-118`
- **Description:** Set to `None`/`false` in `invalidate_cache()` but never populated or read. Vestigial fields from a removed optimization.
- **Fix:** Remove if confirmed unused.

#### ARCH-15: Backup Failure Silently Ignored
- **File:** `src/tabs.rs:278`
- **Description:** `let _ = std::fs::copy(...)` -- saving proceeds without notification on backup failure. Arguably correct, but the user has no indication.

#### ARCH-16: No Tests for UI, Syntax, Config, or Hex Modules
- **File:** `src/ui.rs`, `src/syntax.rs`, `src/config.rs`, `src/hex.rs`
- **Description:** These modules have zero tests. Rendering is hard to unit test, but syntax highlighting keyword matching, config serialization round-trips, and hex cursor movement could all benefit from tests.

#### ARCH-17: Word Completion Is Naive -- First Match Only
- **File:** `src/editor.rs:893-933`
- **Description:** `word_complete` finds the first matching word with no cycling through alternatives.

### Info

- **`Editor::new_for_test` is identical to `new_buffer`** (`src/editor.rs:164-166`) -- exists for test clarity.
- **Fuzzy finder only searches open tabs, not files** (`src/input.rs:1006-1017`) -- design choice, not a bug.
- **Verbatim input is well-integrated** (`src/input.rs:1043-1069`) -- clean implementation.
- **Error handling is generally consistent** -- most I/O uses `anyhow::Result` with status message display.
- **Consider an `InputBuffer` abstraction** for the repeated pattern of char accumulation, Enter submit, Esc cancel across multiple modes.
- **Consider a richer selection model** (`Vec<Selection>`) to support future multi-cursor or rectangular selection.

## Performance & Correctness

### High (Correctness Bugs)

#### COR-1: Search Match Positions Are Byte Offsets, Not Char Offsets
- **File:** `src/search.rs:217-228`, used at `src/search.rs:86`
- **Description:** `find_matches_in_line()` returns positions from `str::find()`, which are byte offsets. But `validate_match` (line 232) and `validate_match_at_position` (line 252) interpret positions as character indices. For any non-ASCII content (emoji, CJK, accented characters), this causes incorrect match highlighting, wrong cursor positioning, and potential out-of-bounds access if byte offset exceeds char count.
- **Fix:** Convert byte positions to character positions using `.chars().count()` on the prefix, or use a char-aware search method.

#### COR-10: `perform_replace_interactive` Uses Byte Position as Char Index
- **File:** `src/editor.rs:639-648`
- **Description:** `text.find(search_term)` returns a byte offset, but it's passed to `rope.char_to_line(pos)` which expects a char index. For non-ASCII text, this produces incorrect cursor positioning or panics if the byte offset exceeds the char count. Same class of bug as COR-1.
- **Fix:** Convert byte offset to char index before passing to rope methods.

### High (Performance)

#### PERF-1: `rope.to_string()` Called in Multiple Hot Paths
- **File:** `src/editor.rs:499-500, 520-528, 616-617, 638-639, 915`
- **Description:** `toggle_hex_view()` calls `self.rope.to_string()` THREE times. `perform_replace()` and `perform_replace_interactive()` materialize the full document. `word_complete()` materializes the entire buffer on every Alt+\ press.
- **Fix:** Cache the string where multiple calls occur in the same method. Use rope-native operations where possible.

### Medium (Correctness Bugs)

#### COR-2: `word_complete` Byte/Char/Display-Column Confusion
- **File:** `src/editor.rs:893-933`
- **Description:** At line 898, `col` (a display column) is used as a byte index into a String. For lines with multi-byte characters or wide characters, this will panic (byte index not on char boundary) or slice at the wrong position. At line 931, byte length is added to a display column.
- **Fix:** Convert display column to byte/char index before string slicing.

#### COR-3: `move_word_right`/`move_word_left` Char Index vs Display Column
- **File:** `src/editor.rs:755-804`
- **Description:** These functions iterate by character index and assign the result to `viewport.cursor_pos.1` (a display column). For wide characters (CJK, emoji), a single character can occupy 2 display columns, causing incorrect cursor positioning after word-jump.
- **Fix:** Convert the final char index to a display column using `UnicodeWidthChar`.

#### COR-5: Mouse Click Doesn't Account for Line Numbers or Horizontal Scroll
- **File:** `src/editor.rs:460-467`
- **Description:** `clicked_col = event.column as usize` is used directly without subtracting the line number gutter width or accounting for horizontal scroll offset (`viewport_offset.1`). Cursor placement is wrong when line numbers are shown or horizontal scrolling is active.
- **Fix:** Subtract gutter width and add horizontal scroll offset to the click column.

#### COR-6: Tab Bar Click Handler Ignores Tab Scroll Offset
- **File:** `src/tabs.rs:870-882`
- **Description:** The click handler iterates all tabs from index 0, but rendering starts from `tab_scroll_offset`. Clicking a tab when scrolled selects the wrong one. (Also reported as ARCH-5.)
- **Fix:** Start iteration from `tab_scroll_offset` and account for overflow indicator width.

### Medium (Performance)

#### PERF-2: Word Wrap Rendering Allocates Per-Frame
- **File:** `src/ui.rs:524`
- **Description:** Every visible line calls `collect_span_chars()` which allocates a `Vec<(char, Style)>` proportional to line length. For very long lines (e.g., minified JSON), this is expensive every frame.
- **Fix:** Consider caching or amortizing allocations across frames.

#### PERF-7: Search Highlighting Re-validates Matches Every Frame
- **File:** `src/ui.rs:751-838`
- **Description:** `apply_search_highlighting()` calls `validate_match_at_position()` for every visible match every frame, each time collecting the line into `Vec<char>`. Matches were already validated when `find_all_matches` ran.
- **Fix:** Trust validated matches and skip per-frame re-validation, or cache validation results.

### Low (Correctness)

#### COR-7: Execute Command Has No Timeout (Can Hang UI)
- **File:** `src/input.rs:1096-1163`
- **Description:** No deadlock risk (stdin is properly closed), but no timeout on `wait_with_output()`. Overlaps with SEC-2.

#### COR-8: 0-Width Terminal Handling
- **File:** `src/editor.rs:347-349, 442-444`
- **Description:** Handled safely -- `update_viewport_for_size` returns early if `editor_height == 0`.

#### COR-11: `insert_newline` Auto-Indent Uses `indent.len()` as Display Column
- **File:** `src/editor.rs:253`
- **Description:** For tab-indented files, `indent.len()` (char count) differs from display width. Cursor position will be wrong after auto-indent with tabs.
- **Fix:** Use display width calculation for the indent string.

### Low (Performance)

#### PERF-3: `slice_spans_horizontal` Per-Span Allocation
- **File:** `src/ui.rs:600-634`
- **Description:** Uses `.chars().collect::<Vec<char>>()` per span. Acceptable for typical code lines; only matters with pathologically long lines.

#### PERF-5: Fuzzy Finder Allocations
- **File:** `src/fuzzy.rs:1-63`
- **Description:** `to_lowercase()` + `chars().collect()` creates two allocations per candidate per keystroke. Negligible for tab count but would matter if extended to file search.

#### PERF-6: Selection Highlighting Per-Frame Work
- **File:** `src/ui.rs:856-929`
- **Description:** Calls `line_col_to_char_idx()` twice per visible line per frame and builds new spans. Proportional to visible lines, not document size.

### Info

- **Tab switching is efficient** (`src/tabs.rs:153-168`) -- no unnecessary recomputation.
- **Empty file edge case handled correctly** -- `Rope::new()` with `len_lines()` returning 1 works properly.

## Recommendations

### Priority 1 -- Security (fix immediately)

1. **Add timeout to execute command** (SEC-1, SEC-2) -- Use async execution or `wait_timeout` so a hanging command doesn't freeze the editor. Add a confirmation prompt before execution.
2. **Limit command output capture** (SEC-3) -- Cap stdout/stderr capture at a reasonable size (e.g., 1MB) and warn on truncation.

### Priority 2 -- Correctness (fix before release)

3. **Fix byte-vs-char position bugs in search** (COR-1, COR-10) -- Convert byte offsets from `str::find()` to char indices throughout `search.rs` and `editor.rs`. This is the most impactful correctness fix as it affects all non-ASCII users.
4. **Fix `word_complete` column confusion** (COR-2) -- Convert display columns to byte/char indices before string slicing to prevent panics.
5. **Fix `move_word_right`/`move_word_left` for wide chars** (COR-3) -- Convert char index to display column for cursor positioning.
6. **Fix mouse click gutter/scroll offset** (COR-5) -- Subtract line number width and add horizontal scroll offset.
7. **Fix tab bar click with scroll offset** (COR-6, ARCH-5) -- Account for `tab_scroll_offset` and overflow indicators in click handler.

### Priority 3 -- Architecture (improve maintainability)

8. **Extract char-to-display-col utility** (ARCH-3) -- Eliminate 5+ duplicated conversion patterns.
9. **Fix `input_mode` split-brain** (ARCH-1) -- Either move per-editor modes to `Editor` or save/restore on tab switch.
10. **Standardize status message handling** (ARCH-7) -- Use `set_temporary_status_message` consistently.
11. **Fix `perform_save` error return** (ARCH-8) -- Return `Err` on write failure instead of showing a message and returning `Ok`.
12. **Remove unused `cached_text`/`cache_valid` fields** (ARCH-14).
13. **Add tests for syntax, config, and hex modules** (ARCH-16).

### Priority 4 -- Performance (optimize)

14. **Deduplicate `rope.to_string()` calls in `toggle_hex_view`** (PERF-1, SEC-6) -- Call once, reuse the result.
15. **Use rope-native operations for replace** (SEC-5, ARCH-9) -- Avoid materializing the full document for find/replace.
16. **Use `VecDeque` for undo stack** (SEC-4) -- O(1) front removal instead of O(n) `Vec::remove(0)`.
17. **Eliminate redundant per-frame search match re-validation** (PERF-7) -- Trust the initial validation results.
18. **Use rope iteration in `word_complete`** (SEC-9) -- Avoid full document materialization on every invocation.
