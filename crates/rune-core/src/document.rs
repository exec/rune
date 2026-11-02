//! Frontend-independent document API. All positions count Unicode scalar values.
use crate::editor::{Editor, UndoManager};
use crate::file_io::{self, SaveError, SaveOptions, SaveOutcome};
use ropey::Rope;
use std::ops::Range;
use std::path::{Path, PathBuf};

/// Directed selection; equal endpoints represent a caret. Never measured in pixels or cells.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Selection {
    pub anchor: usize,
    pub caret: usize,
}

impl Selection {
    pub fn range(self) -> Range<usize> {
        self.anchor.min(self.caret)..self.anchor.max(self.caret)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum EditError {
    InvalidRange,
    InvalidSelection,
}

impl std::fmt::Display for EditError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            Self::InvalidRange => "Text range is reversed or outside the document",
            Self::InvalidSelection => "Selection is outside the document",
        })
    }
}

impl std::error::Error for EditError {}

/// Owns selection and undo in text coordinates while reusing Rune's buffer, caches, and I/O.
/// Renderers borrow the rope; edits must go through this API to keep invalidation correct.
pub struct Document {
    editor: Editor,
    selection: Selection,
    undo: UndoManager<Selection>,
}

impl Default for Document {
    fn default() -> Self {
        Self::new()
    }
}

impl Document {
    /// No configuration reads, terminal operations, or background work.
    pub fn new() -> Self {
        Self {
            editor: Editor::new_buffer(),
            selection: Selection::default(),
            undo: UndoManager::default(),
        }
    }

    #[inline]
    pub fn text(&self) -> &Rope {
        &self.editor.rope
    }

    #[inline]
    pub fn selection(&self) -> Selection {
        self.selection
    }

    pub fn set_selection(&mut self, selection: Selection) -> Result<(), EditError> {
        if selection.anchor > self.text().len_chars() || selection.caret > self.text().len_chars() {
            return Err(EditError::InvalidSelection);
        }
        self.selection = selection;
        Ok(())
    }

    pub fn insert_text(&mut self, text: &str) -> Result<(), EditError> {
        self.replace_range(self.selection.range(), text)
    }

    /// Insert the file's newline, copying only indentation before the selection start.
    pub fn insert_newline(&mut self, auto_indent: bool) {
        let text = self
            .editor
            .newline_with_indent(self.selection.range().start, auto_indent);
        self.insert_text(&text).expect("bounded document selection");
    }

    fn selected_lines(&self) -> Range<usize> {
        let range = self.selection.range();
        let first = self.text().char_to_line(range.start);
        // An exclusive endpoint at the next line's start does not select that line.
        let last = self.text().char_to_line(if range.is_empty() {
            range.end
        } else {
            range.end - 1
        });
        first..last + 1
    }

    /// Indent/outdent selected lines in one undo step, retaining directed selection.
    /// Outdent removes one leading tab or up to `width` leading spaces.
    pub fn indent_lines(&mut self, width: usize, outdent: bool) -> bool {
        if width == 0 {
            return false;
        }
        let lines = self.selected_lines();
        if outdent
            && !self
                .text()
                .lines_at(lines.start)
                .take(lines.len())
                .any(|line| crate::editor::unindent_len(line, width) > 0)
        {
            return false;
        }
        let prefix = if outdent {
            String::new()
        } else {
            " ".repeat(width)
        };
        self.edit_line_prefixes(&prefix, |line| {
            (
                0,
                if outdent {
                    crate::editor::unindent_len(line, width)
                } else {
                    0
                },
            )
        });
        true
    }

    /// Toggle the language's line comment marker in one undo step.
    /// Returns false for languages that require block comments.
    pub fn toggle_line_comment(&mut self) -> bool {
        use crate::editor::{line_comment_at, line_comment_prefix};
        let Some(prefix) = line_comment_prefix(self.syntax_name()) else {
            return false;
        };
        let lines = self.selected_lines();
        let remove = self
            .text()
            .lines_at(lines.start)
            .take(lines.len())
            .all(|line| line_comment_at(line, prefix).is_some());
        self.edit_line_prefixes(if remove { "" } else { prefix }, |line| {
            if remove {
                line_comment_at(line, prefix).unwrap_or((0, 0))
            } else {
                (0, 0)
            }
        });
        true
    }

    fn edit_line_prefixes(
        &mut self,
        prefix: &str,
        edit: impl Fn(ropey::RopeSlice<'_>) -> (usize, usize),
    ) {
        let lines = self.selected_lines();
        let anchor_line = self.text().char_to_line(self.selection.anchor);
        let anchor_col = self.selection.anchor - self.text().line_to_char(anchor_line);
        let caret_line = self.text().char_to_line(self.selection.caret);
        let caret_col = self.selection.caret - self.text().line_to_char(caret_line);
        let mut selection = self.selection;
        let added = prefix.chars().count();
        self.undo.save_state(&self.editor.rope, selection);
        self.editor
            .edit_line_prefixes(lines.start, lines.end - 1, prefix, |line_index, line| {
                let (offset, removed) = edit(line);
                for (position, original_line, original_col) in [
                    (&mut selection.anchor, anchor_line, anchor_col),
                    (&mut selection.caret, caret_line, caret_col),
                ] {
                    if original_line > line_index {
                        *position = *position - removed + added;
                    } else if original_line == line_index && original_col >= offset {
                        *position = *position - removed.min(original_col - offset) + added;
                    }
                }
                (offset, removed)
            });
        self.selection = selection;
        self.editor.modified = true;
        self.editor.mark_changed_range(lines.start, lines.end);
    }

    /// One transaction and one undo step; offsets are Unicode scalars, not bytes/graphemes/UTF-16.
    pub fn replace_range(&mut self, range: Range<usize>, text: &str) -> Result<(), EditError> {
        if range.start > range.end || range.end > self.text().len_chars() {
            return Err(EditError::InvalidRange);
        }
        if range.is_empty() && text.is_empty() {
            return Ok(());
        }
        self.undo.save_state(&self.editor.rope, self.selection);
        let line = self.text().char_to_line(range.start);
        let old_line_count = self.text().len_lines();
        self.editor.rope.remove(range.clone());
        self.editor.rope.insert(range.start, text);
        let caret = range.start + text.chars().count();
        self.selection = Selection {
            anchor: caret,
            caret,
        };
        self.editor.modified = true;
        // Replacement can touch more than its starting line even if line count is unchanged.
        if range.is_empty() {
            self.editor
                .mark_local_edit(line, range.start, old_line_count);
        } else {
            let after_line = self
                .text()
                .char_to_line(range.start.min(self.text().len_chars()));
            self.editor.mark_document_changed(line.min(after_line));
        }
        Ok(())
    }

    pub fn undo(&mut self) -> bool {
        if !self.undo.undo(&mut self.editor.rope, &mut self.selection) {
            return false;
        }
        self.editor.modified = true;
        self.editor.mark_document_changed(0);
        true
    }

    pub fn redo(&mut self) -> bool {
        if !self.undo.redo(&mut self.editor.rope, &mut self.selection) {
            return false;
        }
        self.editor.modified = true;
        self.editor.mark_document_changed(0);
        true
    }

    /// On failure, preserves the current document, selection, and undo history.
    pub fn load_file(&mut self, path: PathBuf) -> anyhow::Result<()> {
        self.editor.load_file(path)?;
        self.selection = Selection::default();
        self.undo = UndoManager::default();
        Ok(())
    }

    pub fn save(&mut self, path: &Path, options: SaveOptions) -> Result<SaveOutcome, SaveError> {
        file_io::save(&mut self.editor, path, options)
    }

    #[inline]
    pub fn path(&self) -> Option<&Path> {
        self.editor.file_path.as_deref()
    }

    pub fn display_name(&self) -> &str {
        &self.editor.display_name
    }

    #[inline]
    pub fn syntax_name(&self) -> Option<&str> {
        self.editor.syntax_name.as_deref()
    }

    #[inline]
    pub fn is_modified(&self) -> bool {
        self.editor.modified
    }

    #[inline]
    pub fn id(&self) -> u64 {
        self.editor.buffer_id
    }

    #[inline]
    pub fn revision(&self) -> u64 {
        self.editor.dirty_generation
    }

    /// Consume pending invalidation; `end == usize::MAX` means the entire suffix.
    /// A view must also invalidate its layout when its font, width, or styling changes.
    pub fn take_changed_lines(&mut self) -> Option<Range<usize>> {
        let start = std::mem::replace(&mut self.editor.dirty_from_line, usize::MAX);
        let end = std::mem::take(&mut self.editor.dirty_to_line);
        (start != usize::MAX).then_some(start..end)
    }
}
