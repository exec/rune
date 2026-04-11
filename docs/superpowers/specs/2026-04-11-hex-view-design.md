# Hex View Mode

A toggleable read-only hex view for inspecting file contents at the byte level.

## Keybinding

- **Ctrl+B** toggles hex view on/off from Normal mode ("B" for bytes).
- Added to the help modal under a new "VIEW" section.
- Help bar in Normal mode shows `^B: Hex View`.

## Layout

Fixed 16 bytes per row in the classic three-column format:

```
00000000 | 48 65 6C 6C 6F 20 57 6F  72 6C 64 0A 00 FF AB CD |Hello Wo rld.....
00000010 | ...                                                |...
```

- **Offset column**: 8-character hex offset (zero-padded).
- **Hex column**: 16 bytes displayed as two-digit uppercase hex, with a double-space gap after byte 8.
- **ASCII column**: printable characters shown as-is, non-printable rendered as `.`.
- The active byte is highlighted in both the hex and ASCII columns simultaneously.

## Architecture

### New file: `src/hex.rs`

Contains `HexViewState` and rendering logic.

### `HexViewState`

```rust
pub struct HexViewState {
    pub raw_bytes: Vec<u8>,   // file contents as raw bytes
    pub cursor: usize,        // byte offset under cursor
    pub scroll_offset: usize, // first visible row index
}
```

Stored as `Option<HexViewState>` on `Editor`. Populated on toggle-in, dropped on toggle-out.

### `InputMode::HexView`

New variant added to the `InputMode` enum.

### Rendering

New `draw_hex_view()` function in `ui.rs`, called when `input_mode == InputMode::HexView`. Replaces the normal text editor area; the status bar and help bar remain.

## Input Handling

All inputs in hex view are navigation-only. Editing keys are silently ignored.

| Key | Action |
|-----|--------|
| Left / Right | Move cursor one byte |
| Up / Down | Move cursor one row (16 bytes) |
| Page Up / Page Down | Scroll by visible page height |
| Ctrl+B | Exit hex view, return to Normal mode |
| Esc | Exit hex view, return to Normal mode |

Cursor movement clamps to `[0, raw_bytes.len() - 1]`. Scroll offset adjusts automatically to keep the cursor row visible.

## Behavior

### Entering hex view (Ctrl+B in Normal mode)

1. If no file is loaded (`file_path` is `None`): show status message "No file to view in hex mode". Stay in Normal mode.
2. If the buffer has unsaved changes (`modified` is `true`): show status message "Save changes before hex view". Stay in Normal mode.
3. Otherwise: read the file from disk via `fs::read()` into `HexViewState.raw_bytes`. Set cursor to 0, scroll to 0. Switch to `InputMode::HexView`.

### Exiting hex view (Ctrl+B or Esc)

1. Drop `HexViewState` (set to `None`) to free memory.
2. Switch back to `InputMode::Normal`.
3. Restore the previous text cursor position (unchanged during hex view).

### Empty files

If the file exists but is empty (0 bytes), enter hex view and display an empty view with no rows. The cursor stays at offset 0.

## Help Modal Addition

New section added between NAVIGATION and OPTIONS:

```
-------------------------------------------
                  VIEW
-------------------------------------------
^B       Hex view (read-only)
```

## Help Bar

In Normal mode, append `^B: Hex View` to the right side of the help bar.

In HexView mode, the help bar shows:
- Left: `Arrows: Navigate  PgUp/PgDn: Page  ^B/Esc: Exit`
- Right: (empty)
