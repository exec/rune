# Changelog

All notable changes to rune will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

## [0.1.0] - 2025-01-31

### Added
- Initial release of rune text editor
- Core text editing functionality with ropey rope data structure
- Standard keybindings (Ctrl+S save, Ctrl+Q quit, Ctrl+W save-as)
- Visual selection mode (Ctrl+V) with keyboard and mouse support
- Full mouse support (click to position, drag to select, scroll to navigate)
- Syntax highlighting for popular languages (Rust, Python, JavaScript, etc.)
- Smart file operations:
  - Filename prompts when saving untitled files
  - Automatic directory creation
  - Save-as functionality
- Quit confirmation for unsaved changes
- Zero-configuration operation - works immediately
- Cross-platform terminal support via crossterm and ratatui
- Fast startup and responsive editing
- Real-time status bar with file info and cursor position
- Context-sensitive help bar

### Technical Details
- Built with Rust for performance and reliability
- Uses ratatui for terminal UI rendering
- Crossterm for cross-platform terminal manipulation
- Ropey for efficient text buffer management
- Syntect for syntax highlighting (simplified implementation)
- Clap for command-line argument parsing

### Platforms Supported
- Linux (x86_64, aarch64, armv7, i686, musl variants)
- FreeBSD (x86_64)
- NetBSD (x86_64)
- macOS support planned for future releases

[Unreleased]: https://github.com/exec/rune/compare/v0.1.0...HEAD
[0.1.0]: https://github.com/exec/rune/releases/tag/v0.1.0