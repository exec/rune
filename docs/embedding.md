# Using rune-core from Etch

The dependency structure is Rune → rune-core and IDE → Etch's GUI library →
rune-core. The shared engine lives in `crates/rune-core` in Rune's workspace.
Etch depends directly on that crate, with a dependency alias preserving its
existing `etch::rune` re-export:

```toml
[dependencies]
rune = { package = "rune-core", path = "../rune/crates/rune-core", version = "0.1.0" }
```

A new consumer can name the dependency `rune-core` and import `rune_core::Document`.
It has no terminal/GUI feature switches or dependencies on Ratatui, Crossterm,
CLI parsing, settings, directory walking, or the updater. The engine's public
`Document` type always uses neutral highlight roles. Frontends specialize the
shared lexer with `syntax::HighlightTheme` to cache their native styles without
runtime conversion or copying spans; Rune's adapter stays in Rune.

For compatibility, `rune::{Document, Selection, EditError}` and the search/file-I/O
modules re-export the same engine types. Existing `rune` consumers using
`default-features = false` still work. Rune's default `terminal` feature builds
the terminal application. No separate engine copy, process protocol, or FFI layer
is required.

## Documents and positions

```rust
use rune::{Document, Selection};

let mut document = Document::new();
document.insert_text("Hello, 界!\n")?;
document.set_selection(Selection { anchor: 7, caret: 8 })?;
document.insert_text("Etch")?; // Replace the selected scalar with four scalars.
assert_eq!(document.text().to_string(), "Hello, Etch!\n");
assert!(document.undo()); // Restores both text and directed selection.
```

Every position in `Selection` and `replace_range` counts Unicode scalar values
from the document start. Positions are not byte offsets, UTF-16 offsets, terminal
cells, grapheme clusters, or pixel coordinates. Etch owns shaping, hit testing,
grapheme navigation, IME preedit, and translation to document positions. Reversed
selections are valid; reversed replacement ranges and out-of-bounds positions
return `EditError` without changing text/history.

`replace_range(range, text)` is one undoable transaction and leaves a caret at
the inserted text's end. `insert_text` replaces the current selection. Empty
insertions at an empty selection do nothing. Undo/redo retain directed selections
and use the same snapshot/history implementation as terminal Rune, specialized
for text positions. Undo/redo mark the buffer modified, matching terminal behavior.

`text()` lends the actual rope read-only. Use rope slices, line/chunk iterators,
or cheap rope snapshots; avoid converting the whole file to a String each frame.
Only edit through the document API so undo, revisions, and invalidation stay
consistent. The legacy `rune::editor::Editor` remains available for terminal callers;
its viewport/cursor APIs use terminal display columns and should not drive GUI
positions. The first extraction keeps those implementations in hidden core
modules; splitting storage from legacy viewport state is a later step.

`id()` identifies the loaded buffer, `revision()` changes on text edits, and
`take_changed_lines()` consumes coalesced dirty lines. Its exclusive end may be
`usize::MAX`, meaning invalidate the entire suffix. A shared view controller
should distribute that change to every view before consuming the next change.
Font, viewport width, syntax, and theme changes need their own view invalidation.
Selection changes do not bump the text revision.

## Shared editing commands

`Document::insert_newline(auto_indent)` uses the loaded file's newline convention
and optionally copies spaces/tabs before the replacement start. It replaces the
selection as one undo step, with the caret after the inserted indentation.

`indent_lines(width, outdent)` adds `width` spaces or removes up to that many
leading spaces / one leading tab. `toggle_line_comment()` uses the detected
language's marker. Both act on selected lines, preserve directed selections, and
create one undo step. An exclusive selection endpoint at a line start excludes
that line. Width zero, an outdent with nothing to remove, or an unsupported
line-comment language produces no edit or undo entry. The boolean result reports
whether the command edited the buffer. Comment addition uses Rune's existing
column-zero marker style; uncommenting also recognizes markers after whitespace.

These commands share the optimized dense/sparse line-prefix editor used by the
terminal frontend. Their invalidation covers changed lines; cached graphical
layouts must still update absolute text offsets after edits on earlier lines,
even when those later lines' shapes can be reused.

## Search and highlighting

Use `search::SearchState` against `document.text()`. Search span coordinates are
`(line, scalar_column, scalar_length)`; `rope.line_to_char(line) + scalar_column`
converts a result to an absolute document position. Search limits and Unicode
matching semantics are unchanged.

Use `syntax::TokenHighlighter` for cached `(HighlightKind, String)` spans. It
shares the lexer/cache implementation with terminal highlighting, without
Ratatui types or a conversion pass. Etch maps roles to its own theme and draws
only visible lines. Unrecognized/untitled syntax remains plain.

```rust
use rune::{Document, syntax::TokenHighlighter};
let mut document = Document::new();
let mut highlighter = TokenHighlighter::new();
document.insert_text("plain text\n")?;
highlighter.set_syntax(document.syntax_name());
if let Some(lines) = document.take_changed_lines() {
    highlighter.invalidate_cache_range(lines.start, lines.end);
}
let line = document.text().line(0).to_string();
let spans = highlighter.highlight_line(0, &line);
```

Refresh `set_syntax(document.syntax_name())` after loading or Save As; repeated
unchanged calls are cheap. Explicit language selection can instead be owned by
the frontend. Neither this API nor Rune performs graphical font layout.

## Safe files and scheduling

`load_file(path)` preserves the existing document, selection, and history on
failure; successful loads reset history/selection and update buffer identity.
`save(path, SaveOptions)` uses the same streamed validation, backup handling,
symlink behavior, and atomic replacement as terminal Rune.

The typed save errors distinguish `ChangedOnDisk`, `DestinationExists`, and
`Io`. Etch owns the dialogs: retry a confirmed Save As with `overwrite: true`.
That flag never bypasses external-change checks on the original file. A
successful `SaveOutcome` separately reports newly created parents and any
nonfatal backup error. Ordinary saves clear the modified flag.

These APIs are synchronous. Keep blocking I/O and expensive work away from the
GUI rendering thread through Etch's scheduling. Highlight caches use `Rc`, so a
document stays on its owning thread; construct it there rather than moving it
between threads. Immutable rope snapshots can be sent to background readers.

```sh
cargo test --locked -p rune-core
cargo run --locked --no-default-features --example headless
cargo test --locked --workspace --no-default-features --lib --tests --examples
cargo bench --locked --no-default-features --bench document_perf
```

The terminal feature remains on by default, preserving normal builds, installs,
and existing key bindings. Rune and direct core consumers can coexist in a
dependency graph: frontend features cannot enable terminal dependencies in
`rune-core` or change its document/token types.

See [the staged performance checks](editor-performance-core.md) for the measured
cost of these boundaries.

See [the extraction measurements](editor-performance-extraction.md) for the crate-boundary checks.
