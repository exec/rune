# rune-core

Shared engine for the Rune terminal editor and Etch graphical editor. It has no
terminal, GUI, settings, directory-walker, or updater dependency and no frontend
feature flags. Rune and Etch are independent applications built on the same
editing engine.

```text
Rune terminal app --------------------> rune-core
Etch app -> Etch GUI library ---------> rune-core
```

## Use from another project

```toml
[dependencies]
rune-core = { git = "https://github.com/exec/rune", tag = "core-v0.1.0", version = "0.1.0" }
```

The GitHub `core-v0.1.0` release also includes a standalone `.crate` source package.

Use `Document`, `Selection`, and `EditError` for character-indexed document edits;
`file_io` for safe saves; `search::SearchState` for search;
and `syntax::TokenHighlighter` for neutral highlight roles. See
[embedding documentation](../../docs/embedding.md) for contracts and limits.

`syntax::HighlightTheme` lets a frontend specialize cached highlight styles at
compile time. Rune owns its Ratatui mapping; core documents always use neutral
roles, regardless of which frontends are in the dependency graph.

This release preserves optimized editing, undo, rope traversal, cache
invalidation, and save algorithms. The hidden legacy `editor` and `hex` modules
still support Rune's column-based buffer and viewport internals. They are not the
API for new frontends; they remain compatibility support for Rune's existing
terminal implementation. Core file I/O is synchronous and highlight caches are
thread-local.

```sh
cargo test --locked -p rune-core
cargo tree --locked -p rune-core --edges normal
```
