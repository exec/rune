# Tabs, Fuzzy Finder & Remaining Features (v1.5) Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add a tab system for multi-buffer editing, a fuzzy finder to navigate tabs, folder opening support, and remaining nano features (word completion, backup files, verbatim input, execute command).

**Architecture:** Introduce a `TabManager` struct that holds a `Vec<Editor>` and an `active_tab` index. The main loop operates on `tab_manager.active_editor_mut()`. Each tab is a full `Editor` instance with its own rope, cursor, undo stack, etc. Shared state (config, clipboard) is lifted to `TabManager`. A tab bar renders at the top of the screen. The fuzzy finder is a new `InputMode` with a filtered list of open tabs.

**Tech Stack:** Rust, ratatui, crossterm, `ignore` crate (for .gitignore-aware folder traversal), `regex` crate (already present)

---

## File Structure

| File | Action | Responsibility |
|------|--------|---------------|
| `src/tabs.rs` | Create | `TabManager` struct, tab operations (new, close, switch, reorder) |
| `src/fuzzy.rs` | Create | Fuzzy finder logic — scoring, filtering, UI |
| `src/editor.rs` | Modify | Move shared state (config, clipboard) out; add `display_name()` method; add `FuzzyFinder` to InputMode |
| `src/input.rs` | Modify | Route key events through TabManager; add tab keybindings; add fuzzy finder input handling |
| `src/ui.rs` | Modify | Add tab bar rendering; adjust editor_area to account for tab bar height; render fuzzy finder overlay |
| `src/main.rs` | Modify | Accept multiple file args and `-r`/`--recursive` flag; create TabManager instead of single Editor; pass TabManager through run loop |
| `src/lib.rs` | Modify | Add `pub mod tabs; pub mod fuzzy;` |
| `src/config.rs` | Modify | (unchanged, already shared via Clone) |
| `Cargo.toml` | Modify | Add `ignore = "0.4"` dependency |

---

## Key Architectural Decisions

### TabManager owns shared state

Currently `Editor` holds `config`, `clipboard`, and `input_mode`. With tabs:
- `config` is shared across all tabs (one config for the whole app) → lives on `TabManager`
- `clipboard` is shared (cut in one tab, paste in another) → lives on `TabManager`
- `input_mode` stays on each `Editor` (each tab can be in different modes? No — the *app* has one mode at a time) → actually `input_mode` should live on `TabManager` too, since you can't be in Find mode in one tab and Normal in another simultaneously
- `status_message` is app-global → lives on `TabManager`

### TabManager struct

```rust
pub struct TabManager {
    pub tabs: Vec<Editor>,
    pub active_tab: usize,
    pub config: Config,
    pub clipboard: Vec<String>,
    pub last_cut_line: Option<usize>,
    pub input_mode: InputMode,
    pub status_message: String,
    pub status_message_time: Option<Instant>,
    pub filename_buffer: String,
    pub quit_after_save: bool,
    pub help_scroll: usize,
    pub needs_redraw: bool,
    pub fuzzy_query: String,
    pub fuzzy_selected: usize,
}
```

### Editor becomes a buffer

`Editor` becomes a per-buffer state container:
```rust
pub struct Editor {
    pub rope: Rope,
    pub viewport: ViewportState,
    pub file_path: Option<PathBuf>,
    pub display_name: String,  // tab title
    pub modified: bool,
    pub highlighter: SyntaxHighlighter,
    pub syntax_name: Option<String>,
    pub search: SearchState,
    pub undo_manager: UndoManager,
    pub cached_text: Option<String>,
    pub cache_valid: bool,
    pub hex_state: Option<HexViewState>,
    pub mark_anchor: Option<(usize, usize)>,
}
```

### Tab bar layout

```
[ main.rs ][ editor.rs ][ ui.rs* ][ + ]      ← tab bar (1 row)
1  use anyhow::Result;                        ← editor area
2  ...
─────────────────────────────                 ← status bar
^H Help                     Rune v1.4.0      ← help bar
```

- Active tab: cyan bg, black fg
- Inactive tabs: dark gray bg, white fg
- Modified indicator: `*` after filename
- `[ + ]` button at the end (clickable with mouse)
- Overflow: `‹ 3 ›` indicators when tabs don't fit

---

### Task 1: TabManager Infrastructure

**Files:**
- Create: `src/tabs.rs`
- Modify: `src/editor.rs` — move shared state out, simplify Editor to buffer-only
- Modify: `src/lib.rs` — add `pub mod tabs;`

- [ ] **Step 1: Create `src/tabs.rs` with TabManager**

```rust
use std::path::PathBuf;
use std::time::{Duration, Instant};

use crate::config::{self, Config};
use crate::constants;
use crate::editor::{Editor, InputMode};

pub struct TabManager {
    pub tabs: Vec<Editor>,
    pub active_tab: usize,
    pub config: Config,
    pub clipboard: Vec<String>,
    pub last_cut_line: Option<usize>,
    pub input_mode: InputMode,
    pub status_message: String,
    pub status_message_time: Option<Instant>,
    pub status_message_timeout: Duration,
    pub filename_buffer: String,
    pub quit_after_save: bool,
    pub help_scroll: usize,
    pub needs_redraw: bool,
    // Fuzzy finder state
    pub fuzzy_query: String,
    pub fuzzy_selected: usize,
}

impl TabManager {
    pub fn new() -> Self {
        let config = config::load_config();
        let initial_tab = Editor::new_buffer();
        Self {
            tabs: vec![initial_tab],
            active_tab: 0,
            config,
            clipboard: Vec::new(),
            last_cut_line: None,
            input_mode: InputMode::Normal,
            status_message: String::new(),
            status_message_time: None,
            status_message_timeout: constants::STATUS_MESSAGE_TIMEOUT,
            filename_buffer: String::new(),
            quit_after_save: false,
            help_scroll: 0,
            needs_redraw: true,
            fuzzy_query: String::new(),
            fuzzy_selected: 0,
        }
    }

    pub fn active_editor(&self) -> &Editor {
        &self.tabs[self.active_tab]
    }

    pub fn active_editor_mut(&mut self) -> &mut Editor {
        &mut self.tabs[self.active_tab]
    }

    /// Open a file in a new tab and switch to it.
    pub fn open_in_new_tab(&mut self, path: PathBuf) -> anyhow::Result<()> {
        let mut editor = Editor::new_buffer();
        editor.load_file(path)?;
        self.tabs.push(editor);
        self.active_tab = self.tabs.len() - 1;
        self.needs_redraw = true;
        Ok(())
    }

    /// Open a file in the current tab (with unsaved check done by caller).
    pub fn open_in_current_tab(&mut self, path: PathBuf) -> anyhow::Result<()> {
        self.active_editor_mut().load_file(path)?;
        self.needs_redraw = true;
        Ok(())
    }

    /// Create a new empty tab.
    pub fn new_tab(&mut self) {
        let mut tab = Editor::new_buffer();
        // Generate unique untitled name
        let untitled_count = self.tabs.iter()
            .filter(|t| t.display_name.starts_with("[untitled"))
            .count();
        if untitled_count > 0 {
            tab.display_name = format!("[untitled-{}]", untitled_count + 1);
        }
        self.tabs.push(tab);
        self.active_tab = self.tabs.len() - 1;
        self.needs_redraw = true;
    }

    /// Close the active tab. Returns true if the app should quit (last tab closed).
    pub fn close_tab(&mut self) -> bool {
        if self.tabs.len() <= 1 {
            return true; // signal to quit
        }
        self.tabs.remove(self.active_tab);
        if self.active_tab >= self.tabs.len() {
            self.active_tab = self.tabs.len() - 1;
        }
        self.needs_redraw = true;
        false
    }

    pub fn next_tab(&mut self) {
        if self.tabs.len() > 1 {
            self.active_tab = (self.active_tab + 1) % self.tabs.len();
            self.needs_redraw = true;
        }
    }

    pub fn prev_tab(&mut self) {
        if self.tabs.len() > 1 {
            self.active_tab = if self.active_tab == 0 {
                self.tabs.len() - 1
            } else {
                self.active_tab - 1
            };
            self.needs_redraw = true;
        }
    }

    /// Resolve tab display names — use filename normally, switch to relative path on collisions.
    pub fn resolve_display_names(&mut self) {
        // Collect all filenames
        let names: Vec<String> = self.tabs.iter().map(|t| {
            t.file_path.as_ref()
                .and_then(|p| p.file_name())
                .map(|n| n.to_string_lossy().to_string())
                .unwrap_or_else(|| t.display_name.clone())
        }).collect();

        for (i, tab) in self.tabs.iter_mut().enumerate() {
            if let Some(path) = &tab.file_path {
                let filename = path.file_name()
                    .map(|n| n.to_string_lossy().to_string())
                    .unwrap_or_default();

                // Check if this filename collides with another tab
                let collisions = names.iter().enumerate()
                    .filter(|(j, n)| *j != i && **n == filename)
                    .count();

                if collisions > 0 {
                    // Use relative path or full display path
                    tab.display_name = path.display().to_string();
                } else {
                    tab.display_name = filename;
                }
            }
            // untitled tabs keep their existing display_name
        }
    }

    pub fn set_temporary_status_message(&mut self, message: String) {
        self.status_message = message;
        self.status_message_time = Some(Instant::now());
        self.needs_redraw = true;
    }

    pub fn check_status_message_timeout(&mut self) -> bool {
        if let Some(time) = self.status_message_time {
            if time.elapsed() >= self.status_message_timeout {
                self.status_message.clear();
                self.status_message_time = None;
                return true;
            }
        }
        false
    }

    pub fn reset_cut_tracking(&mut self) {
        self.last_cut_line = None;
    }

    pub fn save_config(&self) {
        let _ = config::save_config(&self.config);
    }
}
```

- [ ] **Step 2: Refactor Editor to be a buffer-only struct**

Move these fields OUT of Editor and into TabManager: `config`, `clipboard`, `last_cut_line`, `input_mode`, `status_message`, `status_message_time`, `status_message_timeout`, `filename_buffer`, `quit_after_save`, `help_scroll`, `needs_redraw`.

Add `display_name: String` to Editor.

Add `new_buffer()` constructor that creates a minimal buffer:
```rust
    pub fn new_buffer() -> Self {
        Self {
            rope: Rope::new(),
            viewport: ViewportState::default(),
            file_path: None,
            display_name: "[untitled]".to_string(),
            modified: false,
            highlighter: SyntaxHighlighter::new(),
            syntax_name: None,
            search: SearchState::default(),
            undo_manager: UndoManager::default(),
            cached_text: None,
            cache_valid: false,
            hex_state: None,
            mark_anchor: None,
        }
    }
```

Update `load_file` to set `display_name` from the path.

- [ ] **Step 3: Update all method signatures**

Every Editor method that currently accesses `self.config`, `self.clipboard`, `self.input_mode`, `self.status_message`, etc. needs to be updated. There are two approaches:

**Option A (recommended):** Methods that need shared state take `&TabManager` or `&mut TabManager` as a parameter.

**Option B:** Methods that need shared state are moved to TabManager, delegating to the active editor.

Go with **Option B** for the key operations (cut, copy, paste, save, quit handling, etc.) — TabManager wraps these with access to both shared state and the active editor. Keep pure buffer operations (insert_char, delete_char, move_cursor, etc.) on Editor.

- [ ] **Step 4: Update `src/main.rs` to use TabManager**

The main loop now operates on `&mut TabManager` instead of `&mut Editor`:
```rust
fn main() -> Result<()> {
    let cli = Cli::parse();
    // ... terminal setup ...
    
    let mut tabs = tabs::TabManager::new();
    
    if tabs.config.mouse_enabled {
        crossterm::execute!(stdout(), crossterm::event::EnableMouseCapture)?;
    }
    
    // Open files from CLI args
    match &cli.files {
        files if !files.is_empty() => {
            for (i, file) in files.iter().enumerate() {
                if i == 0 {
                    tabs.active_editor_mut().load_file(file.clone())?;
                } else {
                    tabs.open_in_new_tab(file.clone())?;
                }
            }
            tabs.active_tab = 0; // start on first file
            tabs.resolve_display_names();
        }
        _ => {} // keep default empty tab
    }
    
    let result = run_editor(&mut terminal, &mut tabs);
    // ... cleanup ...
}
```

Update `run_editor` to pass `&mut TabManager` to `input::handle_key_event` and `ui::draw_ui`.

- [ ] **Step 5: Update `src/input.rs` to use TabManager**

Change `handle_key_event` signature to take `&mut TabManager`. Each handler accesses `tabs.active_editor_mut()` for buffer operations and `tabs` directly for shared state.

- [ ] **Step 6: Update `src/ui.rs` to use TabManager**

Change `draw_ui` to take `&mut TabManager`. Access `tabs.active_editor_mut()` for buffer rendering and `tabs` for status/help bar and tab bar.

- [ ] **Step 7: Add `pub mod tabs;` to `src/lib.rs`**

- [ ] **Step 8: Verify everything compiles and existing tests pass**

Run: `cargo test && cargo clippy`
Expected: All existing tests pass (some test signatures may need updating).

- [ ] **Step 9: Commit**

```bash
git commit -m "refactor: introduce TabManager for multi-buffer support"
```

---

### Task 2: Tab Bar Rendering

**Files:**
- Modify: `src/ui.rs` — add tab bar at top of screen

- [ ] **Step 1: Add tab bar rendering**

In `draw_ui`, add a 1-row tab bar at the top of the screen. Adjust `editor_area` to start 1 row lower:

```rust
let tab_bar_height = 1u16;

let tab_bar_area = Rect {
    x: area.x,
    y: area.y,
    width: area.width,
    height: tab_bar_height,
};

let editor_area = Rect {
    x: area.x,
    y: area.y + tab_bar_height,
    width: area.width,
    height: area.height.saturating_sub(2 + tab_bar_height), // -1 status, -1 help, -1 tab bar
};
```

Render the tab bar:
```rust
fn draw_tab_bar(f: &mut Frame, tabs: &TabManager, area: Rect) {
    let mut spans: Vec<Span> = Vec::new();
    let available_width = area.width as usize;
    let mut used_width = 0;

    for (i, tab) in tabs.tabs.iter().enumerate() {
        let modified = if tab.modified { "*" } else { "" };
        let title = format!(" {}{} ", tab.display_name, modified);
        let title_len = title.len();

        if used_width + title_len > available_width.saturating_sub(4) {
            // Show overflow indicator
            let remaining = tabs.tabs.len() - i;
            spans.push(Span::styled(
                format!(" +{remaining} "),
                Style::default().fg(Color::DarkGray),
            ));
            break;
        }

        let style = if i == tabs.active_tab {
            Style::default().bg(Color::Cyan).fg(Color::Black)
        } else {
            Style::default().bg(Color::DarkGray).fg(Color::White)
        };
        spans.push(Span::styled(title, style));
        used_width += title_len;
    }

    let tab_line = Line::from(spans);
    let tab_widget = Paragraph::new(tab_line)
        .style(Style::default().bg(Color::Black));
    f.render_widget(tab_widget, area);
}
```

- [ ] **Step 2: Verify and commit**

```bash
cargo test && cargo clippy
git commit -m "feat: add tab bar rendering at top of screen"
```

---

### Task 3: Tab Keybindings

**Files:**
- Modify: `src/input.rs` — add tab navigation and management keybindings

- [ ] **Step 1: Wire tab keybindings in `handle_normal`**

```rust
// Tab management
(KeyModifiers::CONTROL, KeyCode::Char('t')) => {
    tabs.new_tab();
}
(KeyModifiers::ALT, KeyCode::Left) => {
    tabs.prev_tab();
}
(KeyModifiers::ALT, KeyCode::Right) => {
    tabs.next_tab();
}
(KeyModifiers::ALT, KeyCode::Char('w')) => {
    let editor = tabs.active_editor();
    if editor.modified {
        tabs.input_mode = InputMode::ConfirmCloseTab;
        tabs.status_message = "Save modified buffer before closing? (Y/N/Ctrl+C)".to_string();
        tabs.needs_redraw = true;
    } else {
        if tabs.close_tab() {
            return Ok(true); // quit if last tab
        }
    }
}
```

- [ ] **Step 2: Add ConfirmCloseTab input mode and handler**

Add `ConfirmCloseTab` to `InputMode` enum. Add handler:
```rust
fn handle_confirm_close_tab(tabs: &mut TabManager, key: KeyEvent) -> Result<bool> {
    match key.code {
        KeyCode::Char('y') | KeyCode::Char('Y') => {
            // Save then close
            let path = tabs.active_editor().file_path.clone();
            if let Some(path) = path {
                tabs.active_editor_mut().perform_save(path)?;
            }
            tabs.input_mode = InputMode::Normal;
            if tabs.close_tab() {
                return Ok(true);
            }
        }
        KeyCode::Char('n') | KeyCode::Char('N') => {
            // Close without saving
            tabs.input_mode = InputMode::Normal;
            if tabs.close_tab() {
                return Ok(true);
            }
        }
        KeyCode::Esc | KeyCode::Char('c') if key.modifiers == KeyModifiers::CONTROL => {
            tabs.input_mode = InputMode::Normal;
            tabs.status_message = "Cancelled".to_string();
            tabs.needs_redraw = true;
        }
        _ => {}
    }
    Ok(false)
}
```

- [ ] **Step 3: Add open-file-in-tab to options menu**

Add `O` and `N` options to the options menu:
```rust
KeyCode::Char('o') => {
    // Open file in current tab
    tabs.input_mode = InputMode::OpenFileCurrentTab;
    tabs.filename_buffer.clear();
    tabs.status_message = "Open file: ".to_string();
    tabs.needs_redraw = true;
}
KeyCode::Char('n') => {
    // Open file in new tab
    tabs.input_mode = InputMode::OpenFileNewTab;
    tabs.filename_buffer.clear();
    tabs.status_message = "Open in new tab: ".to_string();
    tabs.needs_redraw = true;
}
```

Add handlers for `OpenFileCurrentTab` and `OpenFileNewTab` input modes (similar to filename input).

- [ ] **Step 4: Commit**

```bash
git commit -m "feat: add tab navigation keybindings (Ctrl+T, Alt+Left/Right, Alt+W)"
```

---

### Task 4: Multi-File CLI and Folder Opening

**Files:**
- Modify: `src/main.rs` — accept multiple files, `-r`/`--recursive` flag
- Modify: `Cargo.toml` — add `ignore` dependency

- [ ] **Step 1: Update CLI parser**

```rust
#[derive(Parser)]
#[command(author, version, about, long_about = None)]
struct Cli {
    /// Files or directories to open
    files: Vec<PathBuf>,

    /// Recursively open files in directories
    #[arg(short, long)]
    recursive: bool,
}
```

- [ ] **Step 2: Add `ignore` to Cargo.toml**

```toml
ignore = "0.4"
```

- [ ] **Step 3: Implement folder expansion**

```rust
fn expand_paths(paths: &[PathBuf], recursive: bool) -> Vec<PathBuf> {
    use ignore::WalkBuilder;
    
    let mut files = Vec::new();
    
    for path in paths {
        if path.is_file() {
            files.push(path.clone());
        } else if path.is_dir() {
            let mut walker = WalkBuilder::new(path);
            walker.hidden(true); // skip dotfiles
            if !recursive {
                walker.max_depth(Some(1));
            }
            
            for entry in walker.build().flatten() {
                let entry_path = entry.path().to_path_buf();
                if entry_path.is_file() {
                    // Skip binary files (simple heuristic: check first 512 bytes)
                    if let Ok(bytes) = std::fs::read(&entry_path) {
                        let sample = &bytes[..bytes.len().min(512)];
                        if sample.contains(&0) {
                            continue; // likely binary
                        }
                    }
                    files.push(entry_path);
                }
            }
        }
    }
    
    files.sort();
    files.dedup();
    files
}
```

- [ ] **Step 4: Use expanded paths in main**

```rust
let files = expand_paths(&cli.files, cli.recursive);
for (i, file) in files.iter().enumerate() {
    if i == 0 {
        tabs.open_in_current_tab(file.clone())?;
    } else {
        tabs.open_in_new_tab(file.clone())?;
    }
}
tabs.active_tab = 0;
tabs.resolve_display_names();
```

- [ ] **Step 5: Commit**

```bash
git commit -m "feat: support multiple files and folder opening with -r flag"
```

---

### Task 5: Fuzzy Finder (Ctrl+P)

**Files:**
- Create: `src/fuzzy.rs`
- Modify: `src/input.rs` — add FuzzyFinder input mode handler
- Modify: `src/ui.rs` — render fuzzy finder overlay
- Modify: `src/lib.rs` — add `pub mod fuzzy;`

- [ ] **Step 1: Create fuzzy scoring module**

```rust
// src/fuzzy.rs

/// Score a candidate string against a query using fuzzy matching.
/// Returns None if no match, Some(score) if match (higher = better).
pub fn fuzzy_score(query: &str, candidate: &str) -> Option<i32> {
    if query.is_empty() {
        return Some(0);
    }
    
    let query_lower = query.to_lowercase();
    let candidate_lower = candidate.to_lowercase();
    let query_chars: Vec<char> = query_lower.chars().collect();
    let candidate_chars: Vec<char> = candidate_lower.chars().collect();
    
    let mut query_idx = 0;
    let mut score = 0i32;
    let mut last_match_idx: Option<usize> = None;
    
    for (i, &ch) in candidate_chars.iter().enumerate() {
        if query_idx < query_chars.len() && ch == query_chars[query_idx] {
            score += 10; // base match score
            
            // Bonus for consecutive matches
            if let Some(last) = last_match_idx {
                if i == last + 1 {
                    score += 5;
                }
            }
            
            // Bonus for matching at word boundaries
            if i == 0 || candidate_chars[i - 1] == '/' || candidate_chars[i - 1] == '_'
                || candidate_chars[i - 1] == '-' || candidate_chars[i - 1] == '.' {
                score += 8;
            }
            
            last_match_idx = Some(i);
            query_idx += 1;
        }
    }
    
    if query_idx == query_chars.len() {
        // Penalize longer candidates
        score -= candidate.len() as i32;
        Some(score)
    } else {
        None // not all query chars matched
    }
}

/// Filter and rank a list of candidates by fuzzy match score.
pub fn fuzzy_filter(query: &str, candidates: &[(usize, String)]) -> Vec<(usize, String, i32)> {
    let mut scored: Vec<(usize, String, i32)> = candidates
        .iter()
        .filter_map(|(idx, name)| {
            fuzzy_score(query, name).map(|score| (*idx, name.clone(), score))
        })
        .collect();
    
    scored.sort_by(|a, b| b.2.cmp(&a.2)); // highest score first
    scored
}
```

- [ ] **Step 2: Add FuzzyFinder InputMode and handler**

Add `FuzzyFinder` to `InputMode` enum.

In `handle_normal`, add:
```rust
(KeyModifiers::CONTROL, KeyCode::Char('p')) => {
    tabs.input_mode = InputMode::FuzzyFinder;
    tabs.fuzzy_query.clear();
    tabs.fuzzy_selected = 0;
    tabs.needs_redraw = true;
}
```

Add fuzzy finder handler:
```rust
fn handle_fuzzy_finder(tabs: &mut TabManager, key: KeyEvent) -> Result<bool> {
    match key.code {
        KeyCode::Esc => {
            tabs.input_mode = InputMode::Normal;
            tabs.needs_redraw = true;
        }
        KeyCode::Enter => {
            // Switch to selected tab
            let candidates: Vec<(usize, String)> = tabs.tabs.iter().enumerate()
                .map(|(i, t)| (i, t.display_name.clone()))
                .collect();
            let filtered = crate::fuzzy::fuzzy_filter(&tabs.fuzzy_query, &candidates);
            if let Some((tab_idx, _, _)) = filtered.get(tabs.fuzzy_selected) {
                tabs.active_tab = *tab_idx;
            }
            tabs.input_mode = InputMode::Normal;
            tabs.needs_redraw = true;
        }
        KeyCode::Up => {
            tabs.fuzzy_selected = tabs.fuzzy_selected.saturating_sub(1);
            tabs.needs_redraw = true;
        }
        KeyCode::Down => {
            tabs.fuzzy_selected += 1;
            tabs.needs_redraw = true;
        }
        KeyCode::Backspace => {
            tabs.fuzzy_query.pop();
            tabs.fuzzy_selected = 0;
            tabs.needs_redraw = true;
        }
        KeyCode::Char(c) => {
            tabs.fuzzy_query.push(c);
            tabs.fuzzy_selected = 0;
            tabs.needs_redraw = true;
        }
        _ => {}
    }
    Ok(false)
}
```

- [ ] **Step 3: Render fuzzy finder overlay in ui.rs**

Draw a centered overlay with the query input and filtered results list. Active result highlighted in cyan.

- [ ] **Step 4: Commit**

```bash
git commit -m "feat: add fuzzy finder (Ctrl+P) for tab navigation"
```

---

### Task 6: Help as Tab

**Files:**
- Modify: `src/tabs.rs` — add `open_help_tab` method
- Modify: `src/input.rs` — Ctrl+H opens help in a tab instead of toggling mode

- [ ] **Step 1: Implement help-as-tab**

When Ctrl+H is pressed:
- Check if a help tab already exists (display_name == "[Help]")
- If yes, switch to it
- If no, create a new tab with the help content as its rope, read-only

```rust
pub fn open_help_tab(&mut self) {
    // Check if help tab already exists
    if let Some(idx) = self.tabs.iter().position(|t| t.display_name == "[Help]") {
        self.active_tab = idx;
        self.needs_redraw = true;
        return;
    }
    
    let help_text = crate::ui::help_text();
    let mut tab = Editor::new_buffer();
    tab.rope = Rope::from_str(&help_text);
    tab.display_name = "[Help]".to_string();
    // Help tab is read-only — we don't set modified, and editing keys are no-ops
    self.tabs.push(tab);
    self.active_tab = self.tabs.len() - 1;
    self.needs_redraw = true;
}
```

Remove `InputMode::Help` and `help_scroll` — help is now just a regular tab that happens to be read-only.

- [ ] **Step 2: Commit**

```bash
git commit -m "feat: help opens in a tab instead of fullscreen overlay"
```

---

### Task 7: Word Completion (Alt+\)

**Files:**
- Modify: `src/editor.rs` — add word_complete method
- Modify: `src/input.rs` — wire Alt+\

- [ ] **Step 1: Implement word completion**

Scan the current buffer for words matching the prefix under the cursor:
```rust
pub fn word_complete(&mut self) {
    // Get the partial word before the cursor
    let line_idx = self.viewport.cursor_pos.0;
    let col = self.viewport.cursor_pos.1;
    let content = line_content_str(&self.rope, line_idx);
    let before_cursor = &content[..col.min(content.len())];
    
    // Find the word prefix
    let prefix: String = before_cursor.chars().rev()
        .take_while(|c| c.is_alphanumeric() || *c == '_')
        .collect::<String>()
        .chars().rev().collect();
    
    if prefix.is_empty() {
        return;
    }
    
    // Scan all words in the document
    let text = self.rope.to_string();
    let mut candidates: Vec<&str> = Vec::new();
    for word in text.split(|c: char| !c.is_alphanumeric() && c != '_') {
        if word.starts_with(&prefix) && word != prefix && !candidates.contains(&word) {
            candidates.push(word);
        }
    }
    
    if let Some(completion) = candidates.first() {
        let suffix = &completion[prefix.len()..];
        // Insert the completion suffix
        for c in suffix.chars() {
            self.insert_char_no_undo(c);
        }
    }
}
```

- [ ] **Step 2: Wire Alt+\ and commit**

```bash
git commit -m "feat: add word completion (Alt+\\)"
```

---

### Task 8: Backup Files

**Files:**
- Modify: `src/editor.rs` — create backup before saving
- Modify: `src/config.rs` — add `backup_on_save: bool` config option

- [ ] **Step 1: Implement backup on save**

Before writing the file in `perform_save`, if `backup_on_save` is true and the file exists, copy it to `filename~`:
```rust
if config.backup_on_save {
    if path.exists() {
        let backup_path = PathBuf::from(format!("{}~", path.display()));
        let _ = std::fs::copy(&path, &backup_path);
    }
}
```

- [ ] **Step 2: Commit**

```bash
git commit -m "feat: add backup files on save (filename~ suffix)"
```

---

### Task 9: Verbatim Input (Alt+V)

**Files:**
- Modify: `src/input.rs` — add VerbatimInput mode
- Modify: `src/editor.rs` — add VerbatimInput to InputMode

- [ ] **Step 1: Implement verbatim input**

When Alt+V is pressed, enter VerbatimInput mode. The next keypress is inserted literally (including control characters displayed as their Unicode representation, or the raw character):

```rust
fn handle_verbatim_input(tabs: &mut TabManager, key: KeyEvent) -> Result<bool> {
    match key.code {
        KeyCode::Char(c) => {
            tabs.active_editor_mut().insert_char(c);
        }
        KeyCode::Enter => {
            tabs.active_editor_mut().insert_char('\n');
        }
        KeyCode::Tab => {
            tabs.active_editor_mut().insert_char('\t');
        }
        _ => {} // ignore non-character keys
    }
    tabs.input_mode = InputMode::Normal;
    tabs.needs_redraw = true;
    Ok(false)
}
```

- [ ] **Step 2: Commit**

```bash
git commit -m "feat: add verbatim input mode (Alt+V)"
```

---

### Task 10: Execute Command / Pipe

**Files:**
- Modify: `src/editor.rs` — add execute_command method
- Modify: `src/input.rs` — add ExecuteCommand input mode

- [ ] **Step 1: Implement execute command**

When triggered, prompt for a shell command. Execute it and either:
- If no selection: insert command output at cursor
- If selection: pipe selection through command and replace with output

```rust
pub fn execute_command(&mut self, command: &str, selection: Option<String>) -> Result<String> {
    use std::process::{Command, Stdio};
    
    let mut child = Command::new("sh")
        .arg("-c")
        .arg(command)
        .stdin(if selection.is_some() { Stdio::piped() } else { Stdio::null() })
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()?;
    
    if let Some(input) = selection {
        use std::io::Write;
        if let Some(mut stdin) = child.stdin.take() {
            stdin.write_all(input.as_bytes())?;
        }
    }
    
    let output = child.wait_with_output()?;
    Ok(String::from_utf8_lossy(&output.stdout).to_string())
}
```

- [ ] **Step 2: Wire keybinding and input mode, commit**

Use Ctrl+E or a submenu for execute command.

```bash
git commit -m "feat: add execute command with pipe support"
```

---

### Task 11: Update Help Content and Final Polish

**Files:**
- Modify: `src/ui.rs` — update help text with all new keybindings
- Modify: help tab content

- [ ] **Step 1: Update help text to include all new features**

Add to help content:
```
TABS
─────────────────────────────────────────
^T       New tab
M-Left   Previous tab
M-Right  Next tab
M-W      Close tab
^P       Fuzzy finder (switch tab)
^O → O   Open file in current tab
^O → N   Open file in new tab
```

And the new features:
```
M-\      Word completion
M-V      Verbatim input (insert raw char)
```

- [ ] **Step 2: Final verification**

```bash
cargo test && cargo clippy
cargo build --release && cp target/release/rune ~/.cargo/bin/rune
```

- [ ] **Step 3: Commit**

```bash
git commit -m "feat: update help content with all v1.5 features"
```

---

## Self-Review Checklist

**Spec coverage:**
- [x] Tab system — Tasks 1, 2, 3
- [x] Tab navigation (Alt+Left/Right, Alt+W, Ctrl+T) — Task 3
- [x] Multi-file CLI (`rune file1 file2`) — Task 4
- [x] Folder opening (`rune .`, `rune -r .`) — Task 4
- [x] Smart tab titles (collision detection) — Task 1 (resolve_display_names)
- [x] Tab overflow indicators — Task 2
- [x] Fuzzy finder (Ctrl+P) — Task 5
- [x] Help as tab — Task 6
- [x] Open file in current tab (Ctrl+O → O) — Task 3
- [x] Open file in new tab (Ctrl+O → N) — Task 3
- [x] Word completion (Alt+\) — Task 7
- [x] Backup files — Task 8
- [x] Verbatim input (Alt+V) — Task 9
- [x] Execute command / pipe — Task 10
- [x] Updated help — Task 11

**Type consistency:** TabManager methods match across tasks. InputMode variants are consistent.
