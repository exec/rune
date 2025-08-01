# rune - A Modern CLI Text Editor

A nano-inspired text editor that feels familiar but adds the power you've always wanted. Built with Rust for speed and reliability.

## Features

### Core Philosophy
- **Smart defaults** - Works great out-of-the-box, customizable as needed
- **Standard keybindings** - Familiar Ctrl+S save, Ctrl+X exit
- **Terminal-native** - Built for CLI environments
- **Fast startup** - Lightning-fast startup with lazy syntax highlighting

### Modern Conveniences
- ✅ **Full mouse support** - Click to position cursor, drag to select, scroll wheel (configurable)
- ✅ **Smart syntax highlighting** - Lazy, cached highlighting for all major languages
- ✅ **Standard navigation** - Arrow keys, Home, End work as expected
- ✅ **File operations** - Save with Ctrl+S, automatic change detection
- ✅ **Efficient text handling** - Uses rope data structure for large files

### Built for Developers
- **Enhanced Find/Replace** - Unified workflow like nano, but with regex support and live highlighting
- **Smart Search** - All arrow keys navigate results, instant visual feedback, case-sensitive toggle
- **Interactive Replace** - Y/N/A confirmation exactly like nano, with "Replace All" option
- **Go to line** - Ctrl+G to jump to specific lines
- **Undo/Redo** - Ctrl+Z/Ctrl+R with 100-action history
- **Options Menu** - Ctrl+O for organized settings (just like nano)
- No modal editing - what you see is what you get
- Help bar always visible at bottom
- Real-time status information (line/column, file status)

## Installation

### Package Managers

#### Homebrew (macOS/Linux)
```bash
brew tap exec/rune
brew install rune-editor
```

#### Arch Linux (AUR)
```bash
# Using an AUR helper like yay or paru:
yay -S rune-editor
# or
paru -S rune-editor

# Manual installation:
git clone https://aur.archlinux.org/rune-editor.git
cd rune-editor
makepkg -si
```

#### Cargo
```bash
cargo install --git https://github.com/exec/rune
```

### Binary Releases
Download pre-built binaries from the [releases page](https://github.com/exec/rune/releases):

```bash
# Linux x86_64
curl -L https://github.com/exec/rune/releases/latest/download/rune-linux-x86_64.tar.gz | tar xz
sudo mv rune /usr/local/bin/

# macOS Apple Silicon
curl -L https://github.com/exec/rune/releases/latest/download/rune-macos-aarch64.tar.gz | tar xz
xattr -d com.apple.quarantine rune  # Remove quarantine attribute
sudo mv rune /usr/local/bin/

# macOS Intel
curl -L https://github.com/exec/rune/releases/latest/download/rune-macos-x86_64.tar.gz | tar xz
xattr -d com.apple.quarantine rune  # Remove quarantine attribute
sudo mv rune /usr/local/bin/

# Other platforms: linux-aarch64, linux-armv7, freebsd-x86_64, netbsd-x86_64
```

### From Source
```bash
git clone https://github.com/exec/rune
cd rune
cargo build --release
./target/release/rune [filename]
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
- **Ctrl+Q** or **Ctrl+X** - Quit editor (prompts to save if changes)
- **Ctrl+S** - Save file (prompts for filename if none)
- **Ctrl+W** - Save As (save with new filename)
- **Ctrl+O** - Open options menu (configure settings)
- **Ctrl+F** - Find text (enhanced search with regex support)
- **Ctrl+\\** - Replace text (nano-compatible) 
- **Ctrl+G** - Go to line number
- **Ctrl+Z** - Undo last change
- **Ctrl+R** - Redo last undone change (in normal editing mode)
- **Ctrl+V** - Page down (nano-compatible)
- **Ctrl+Y** - Page up (nano-compatible)
- **Page Up/Down** - Page navigation (if available)
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

### Enhanced Find & Replace
- **Ctrl+F** - Enhanced search mode with regex support
- **Ctrl+R** (within find) - Switch to replace mode (unified workflow)
- **Ctrl+O** (within find/replace) - Options menu for case sensitivity and regex toggle
- **All arrow keys** - Navigate between matches (shows "3/7 matches")
- **Replace confirmation** - Y: Replace This | N: Skip | A: Replace All (exactly like nano)
- **Enter** - Execute search/replace or exit mode
- **Escape** - Cancel and return to original position
- **Regex support** - Full regex patterns with fallback to literal search
- Search automatically wraps around document boundaries

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

## Configuration

Press **Ctrl+O** to open the options menu for live configuration. Settings are automatically saved to `~/.config/rune/config.toml`.

### Available Options
- **M** - Toggle mouse mode on/off (allows terminal text selection when disabled)
- **L** - Toggle line numbers on/off
- **W** - Toggle word wrap on/off
- **T** - Cycle tab width (2 → 4 → 8 → 2)

### Configuration File
The config file supports:
```toml
mouse_enabled = true     # Enable/disable mouse support
show_line_numbers = false # Show/hide line numbers
tab_width = 4            # Tab width (2, 4, or 8)
word_wrap = false        # Enable/disable word wrap
```
- Works from any mode (editing, visual selection, filename input)

## Syntax Highlighting

**Smart, lazy highlighting** with intelligent caching for optimal performance:

### Fully Highlighted Languages
- **Rust** (.rs) - Keywords, types, strings, numbers, comments
- **Python** (.py) - Keywords, builtins, strings, numbers, comments  
- **JavaScript/TypeScript** (.js, .ts) - Keywords, types, strings, numbers, comments

### Enhanced Features
- **String literals** - Proper escape sequence handling
- **Numeric literals** - Integers, floats, hex numbers
- **Comments** - Line and block comments  
- **Keywords** - Language-specific syntax
- **Types** - Built-in and common types
- **Lazy loading** - Only highlights visible lines
- **Intelligent caching** - Remembers highlighted lines for speed

### Additional Language Detection
Auto-detects file types for syntax-aware editing:
- C/C++, Go, Java, HTML, CSS, JSON, Markdown, Shell, SQL

### Performance
- **Lazy highlighting** - Only processes visible text
- **Incremental updates** - Re-highlights only changed lines  
- **Cached results** - Lightning-fast scrolling and navigation
- **Zero startup cost** - Highlighting loads as needed

## Architecture

Built with modern Rust libraries:
- **ratatui** - Terminal UI framework
- **crossterm** - Cross-platform terminal manipulation
- **ropey** - Efficient rope data structure for text
- **clap** - Command line argument parsing
- **anyhow** - Error handling

## Philosophy  

rune follows the "better nano" philosophy:
- Easy things should be easy
- Common tasks should be fast  
- The editor should get out of your way
- Modern conveniences shouldn't compromise simplicity
- **Familiar workflow** - If you know nano, you know rune
- Terminal environments deserve great text editing

## Comparison

| Feature | rune | nano | micro | vim |
|---------|------|------|--------|-----|
| Smart defaults | ✅ | ✅ | ✅ | ❌ |
| Regex search | ✅ | ✅ | ✅ | ✅ |
| Visual selection | ✅ | ❌ | ✅ | ✅ |
| Mouse support | ✅ | ❌ | ✅ | ✅ |
| Syntax highlight | ✅ | ✅ | ✅ | ✅ |
| Find/Search | ✅ | ✅ | ✅ | ✅ |
| Go to line | ✅ | ✅ | ✅ | ✅ |
| Undo/Redo | ✅ | ✅ | ✅ | ✅ |
| Line numbers | ✅ | ❌ | ✅ | ✅ |
| Learning curve | None | None | Minimal | Steep |
| Performance | Fast | Fast | Fast | Fast |

## Contributing

**rune** aims to be the definitive "nano but better" editor. Contributions welcome for:
- Additional syntax highlighting languages
- Performance improvements  
- Bug fixes and stability
- Documentation improvements

## Acknowledgments

**rune** is inspired by GNU nano, the excellent terminal text editor created by Chris Allegretta and maintained by Benno Schulenberg. While rune is a complete rewrite in Rust with modern enhancements, we deeply appreciate nano's design philosophy of making text editing accessible and straightforward.

- **GNU nano**: https://www.nano-editor.org/
- **Original author**: Chris Allegretta
- **Current maintainer**: Benno Schulenberg
- **License**: GNU General Public License

rune adopts nano's user-friendly approach while adding modern features like regex search, syntax highlighting, and performance optimizations.

## License

MIT License - see LICENSE file for details.

---

*Built with ❤️ in Rust for developers who want nano's simplicity with modern editor features.*