# Changelog

## [0.1.0] - 2026-11-02

- Extract the shared editing engine from Rune into a standalone workspace library.
- Provide scalar-indexed documents and selections, shared undo/redo, safe saving,
  search, and renderer-independent syntax roles.
- Preserve optimized rope traversal, dense/sparse edits, and bounded cache invalidation.
- Keep terminal and graphical dependencies in their frontends; Rune retains
  compatibility imports and Etch consumes the core directly.
- Retain legacy column/viewport internals for Rune's existing terminal implementation.

Published on GitHub as `core-v0.1.0`; this release is not published to crates.io.
