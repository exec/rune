use crate::constants;
pub use rune_core::editor::{
    char_display_width, line_display_width, str_display_width, unindent_len, UndoManager,
    UndoState, ViewportState, TAB_WIDTH,
};
pub type Editor = rune_core::editor::EditorBuffer<crate::syntax::Theme<ratatui::style::Style>>;

/// Different input modes the editor can be in
#[derive(Debug, Clone, PartialEq)]
pub enum InputMode {
    Normal,
    EnteringFilename,
    EnteringSaveAs,
    ConfirmQuit,
    ConfirmOpen,
    ConfirmOverwrite,
    ConfirmCloseTab,
    OptionsMenu,
    Find,
    FindOptionsMenu,
    Replace,
    ReplaceConfirm,
    GoToLine,
    HexView,
    OpenFileCurrentTab,
    OpenFileNewTab,
    FuzzyFinder,
    VerbatimInput,
    ExecuteCommand,
    ConfirmExecute,
}

pub fn handle_mouse_event(
    editor: &mut Editor,
    event: crossterm::event::MouseEvent,
    terminal_height: usize,
    line_num_width: usize,
) {
    use crossterm::event::MouseEventKind;
    match event.kind {
        MouseEventKind::Down(_) => {
            let clicked_line = editor.viewport.viewport_offset.0 + event.row as usize;
            // Subtract gutter width and add horizontal scroll offset
            let clicked_col = (event.column as usize).saturating_sub(line_num_width)
                + editor.viewport.viewport_offset.1;

            if clicked_line < editor.rope.len_lines() {
                editor.viewport.cursor_pos.0 = clicked_line;
                editor.viewport.cursor_pos.1 = clicked_col;
                editor.clamp_cursor_to_line();
            }
        }
        MouseEventKind::Drag(_) => {}
        MouseEventKind::ScrollDown
            if editor.viewport.viewport_offset.0
                < editor.rope.len_lines().saturating_sub(terminal_height) =>
        {
            editor.viewport.viewport_offset.0 = (editor.viewport.viewport_offset.0
                + constants::SCROLL_SPEED)
                .min(editor.rope.len_lines().saturating_sub(terminal_height));
        }
        MouseEventKind::ScrollUp => {
            editor.viewport.viewport_offset.0 = editor
                .viewport
                .viewport_offset
                .0
                .saturating_sub(constants::SCROLL_SPEED);
        }
        _ => {}
    }
}
