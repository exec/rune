# rune-core

Shared engine for the Rune terminal editor and Etch graphical editor. It has no
terminal, GUI, settings, directory-walker, or updater dependency and no frontend
feature flags. The IDE builds on Etch's GUI library, which uses this engine.

```text
Rune terminal app --------------------> rune-core
Etch app -> Etch GUI library ---------> rune-core
IDE ------> Etch GUI library ---------> rune-core
```

Use `Document`, `Selection`, and `EditError` for character-indexed document edits;
`file_io` for safe saves; `search::SearchState` for search;
and `syntax::TokenHighlighter` for neutral highlight roles. See
[embedding documentation](../../docs/embedding.md) for contracts and limits.

`syntax::HighlightTheme` lets a frontend specialize cached highlight styles at
compile time. Rune owns its Ratatui mapping; core documents always use neutral
roles, regardless of which frontends are in the dependency graph.

This first extraction preserves optimized editing, undo, rope traversal, cache
invalidation, and save algorithms. The hidden legacy `editor` and `hex` modules
still support Rune's column-based buffer and viewport internals. They are not the
API for new frontends; separating document storage from those legacy internals is
a later step. Core file I/O is synchronous and highlight caches are thread-local.

```sh
cargo test --locked -p rune-core
cargo tree --locked -p rune-core --edges normal
```
