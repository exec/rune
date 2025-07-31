# rune - A Modern CLI Text Editor

A nano-inspired text editor that bridges the gap between simplicity and capability, built with Rust for speed and reliability.

## Features

### Core Philosophy
- **Zero learning curve** - Works instantly without configuration
- **Standard keybindings** - Familiar Ctrl+S, Ctrl+Q shortcuts
- **Terminal-native** - Built for CLI environments
- **Fast startup** - Sub-100ms startup time for quick edits

### Modern Conveniences
- ✅ **Visual selection mode** (Ctrl+V) - Select text like in modern editors
- ✅ **Full mouse support** - Click to position cursor, drag to select, scroll wheel
- ✅ **Syntax highlighting** - Built-in support for Rust, Python, JavaScript, and more
- ✅ **Standard navigation** - Arrow keys, Home, End work as expected
- ✅ **File operations** - Save with Ctrl+S, automatic change detection
- ✅ **Efficient text handling** - Uses rope data structure for large files

### Built for Developers
- Respects nano's simplicity while adding power user features
- No modal editing - what you see is what you get
- Help bar always visible at bottom
- Real-time status information (line/column, file status)

## Installation

### Homebrew (macOS/Linux)
```bash
brew tap exec/rune
brew install rune
```

### Binary Releases
Download pre-built binaries from the [releases page](https://github.com/exec/rune/releases):

```bash
# Linux x86_64
curl -L https://github.com/exec/rune/releases/latest/download/rune-linux-x86_64.tar.gz | tar xz
sudo mv rune /usr/local/bin/

# Other platforms available: linux-aarch64, linux-armv7, freebsd-x86_64, netbsd-x86_64
```

### From Source
```bash
git clone https://github.com/exec/rune
cd rune
cargo build --release
./target/release/rune [filename]
```

### Cargo Install
```bash
cargo install --git https://github.com/exec/rune
```

### Usage
```bash
# Edit an existing file
rune myfile.rs

# Create a new file
rune newfile.py

# Just run the editor
rune
```

## Keybindings

### Essential Commands
- **Ctrl+Q** - Quit editor (prompts to save if changes)
- **Ctrl+S** - Save file (prompts for filename if none)
- **Ctrl+W** - Save As (save with new filename)
- **Ctrl+V** - Enter visual selection mode
- **Esc** - Cancel selection/exit modes/cancel prompts

### Navigation
- **Arrow keys** - Move cursor
- **Home** - Go to start of line
- **End** - Go to end of line
- **Mouse** - Click to position, drag to select, scroll to navigate

### Editing
- **Enter** - Insert new line
- **Backspace** - Delete character before cursor
- **Regular typing** - Insert text at cursor

### Visual Mode
- **Ctrl+V** - Start visual selection
- **Arrow keys** - Extend selection
- **Esc** - Cancel selection

### File Operations
- **Ctrl+S** on new file - Prompts for filename, then saves
- **Ctrl+W** - Save As dialog for existing files
- **Enter** in filename input - Confirm save
- **Esc** in filename input - Cancel operation
- Automatic directory creation if parent directories don't exist

### Quit Confirmation
- **Ctrl+Q** with no changes - Quits immediately
- **Ctrl+Q** with unsaved changes - Shows "Save modified buffer? (Y/N/Ctrl+C)"
  - **Y** - Save and quit (prompts for filename if needed)
  - **N** - Quit without saving
  - **Ctrl+C** or **Esc** - Cancel quit, return to editing
- Works from any mode (editing, visual selection, filename input)

## Supported Languages

Syntax highlighting is automatically detected by file extension:

- **Rust** (.rs) - Keywords, types, comments
- **Python** (.py) - Keywords, builtins, comments  
- **JavaScript/TypeScript** (.js, .ts) - Keywords, types, comments
- **C/C++** (.c, .h, .cpp, .cc, .cxx) - Detected but basic highlighting
- **Go** (.go) - File type detection
- **Java** (.java) - File type detection
- **HTML** (.html) - File type detection
- **CSS** (.css) - File type detection
- **JSON** (.json) - File type detection
- **Markdown** (.md) - File type detection
- **Shell** (.sh, .bash) - File type detection
- **SQL** (.sql) - File type detection

## Architecture

Built with modern Rust libraries:
- **ratatui** - Terminal UI framework
- **crossterm** - Cross-platform terminal manipulation
- **ropey** - Efficient rope data structure for text
- **clap** - Command line argument parsing
- **anyhow** - Error handling

## Philosophy

txt1 follows the "better nano" philosophy:
- Easy things should be easy
- Common tasks should be fast
- The editor should get out of your way
- Modern conveniences shouldn't compromise simplicity
- Terminal environments deserve great text editing

## Comparison

| Feature | rune | nano | micro | vim |
|---------|------|------|--------|-----|
| Zero config | ✅ | ✅ | ✅ | ❌ |
| Standard keys | ✅ | ❌ | ✅ | ❌ |
| Visual selection | ✅ | ❌ | ✅ | ✅ |
| Mouse support | ✅ | ❌ | ✅ | ✅ |
| Syntax highlight | ✅ | ❌ | ✅ | ✅ |
| Learning curve | None | None | Minimal | Steep |
| Performance | Fast | Fast | Fast | Fast |

## Contributing

**rune** aims to be the definitive "nano but better" editor. Contributions welcome for:
- Additional syntax highlighting languages
- Performance improvements  
- Bug fixes and stability
- Documentation improvements

## License

MIT License - see LICENSE file for details.

---

*Built with ❤️ in Rust for developers who want nano's simplicity with modern editor features.*