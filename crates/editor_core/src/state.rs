//! Editor state — the smallest complete "interactive buffer": a buffer, a
//! caret, and an undo/redo history.
//!
//! All user-facing editing operations (insert, backspace, cursor moves,
//! undo, redo) route through this type so the rules live in one place and
//! can be unit-tested without a GUI.
//!
//! ### Undo model
//!
//! The history is a pair of stacks of [`Edit`] operations. Every mutation
//! pushes an inverse record onto the undo stack and clears the redo stack.
//! `undo` pops the top edit, applies its inverse, and pushes the original
//! onto the redo stack; `redo` is the mirror. This is the textbook
//! single-user editor model and maps cleanly onto a CRDT layer later.

use crate::{Buffer, Selection};

/// A single reversible edit operation.
#[derive(Debug, Clone)]
enum Edit {
    /// Text was inserted at `at`. Undo removes `text.len()` chars starting
    /// there; redo re-inserts the text.
    Insert { at: usize, text: String },
    /// `text` was removed from `at..at+text.len()`. Undo re-inserts; redo
    /// removes again.
    Remove { at: usize, text: String },
}

impl Edit {
    fn apply(&self, buffer: &mut Buffer, selection: &mut Selection) {
        match self {
            Edit::Insert { at, text } => {
                buffer.insert(*at, text);
                let head = at + text.chars().count();
                *selection = Selection::caret(head);
            }
            Edit::Remove { at, text } => {
                let end = at + text.chars().count();
                buffer.remove(*at..end);
                *selection = Selection::caret(*at);
            }
        }
    }

    fn invert(&self) -> Edit {
        match self {
            Edit::Insert { at, text } => Edit::Remove {
                at: *at,
                text: text.clone(),
            },
            Edit::Remove { at, text } => Edit::Insert {
                at: *at,
                text: text.clone(),
            },
        }
    }
}

/// A buffer plus the state needed to edit it interactively.
#[derive(Debug, Clone, Default)]
pub struct EditorState {
    pub buffer: Buffer,
    pub selection: Selection,
    undo: Vec<Edit>,
    redo: Vec<Edit>,
}

impl EditorState {
    pub fn new(buffer: Buffer) -> Self {
        Self {
            buffer,
            selection: Selection::caret(0),
            undo: Vec::new(),
            redo: Vec::new(),
        }
    }

    /// Replace the backing buffer (e.g. after opening a new file). Clears
    /// history because the old edits no longer apply to the new content.
    pub fn replace_buffer(&mut self, buffer: Buffer) {
        self.buffer = buffer;
        self.selection = Selection::caret(0);
        self.undo.clear();
        self.redo.clear();
    }

    /// Insert `text` at the caret, replacing any active selection first.
    pub fn insert_str(&mut self, text: &str) {
        if text.is_empty() {
            return;
        }
        self.delete_selection();
        let at = self.selection.head;
        let edit = Edit::Insert {
            at,
            text: text.to_string(),
        };
        edit.apply(&mut self.buffer, &mut self.selection);
        self.undo.push(edit);
        self.redo.clear();
    }

    /// Delete the character before the caret. If a selection is active,
    /// delete the selection instead.
    pub fn backspace(&mut self) {
        if !self.selection.is_caret() {
            self.delete_selection();
            return;
        }
        let head = self.selection.head;
        if head == 0 {
            return;
        }
        let at = head - 1;
        let removed = self.buffer.slice_to_string(at..head);
        let edit = Edit::Remove { at, text: removed };
        edit.apply(&mut self.buffer, &mut self.selection);
        self.undo.push(edit);
        self.redo.clear();
    }

    /// Delete the character after the caret, or the active selection.
    pub fn delete_forward(&mut self) {
        if !self.selection.is_caret() {
            self.delete_selection();
            return;
        }
        let head = self.selection.head;
        if head >= self.buffer.len_chars() {
            return;
        }
        let removed = self.buffer.slice_to_string(head..head + 1);
        let edit = Edit::Remove {
            at: head,
            text: removed,
        };
        edit.apply(&mut self.buffer, &mut self.selection);
        self.undo.push(edit);
        self.redo.clear();
    }

    fn delete_selection(&mut self) {
        if self.selection.is_caret() {
            return;
        }
        let range = self.selection.range();
        let removed = self.buffer.slice_to_string(range.clone());
        let edit = Edit::Remove {
            at: range.start,
            text: removed,
        };
        edit.apply(&mut self.buffer, &mut self.selection);
        self.undo.push(edit);
        self.redo.clear();
    }

    /// Move the caret one character left. Drops any active selection.
    pub fn move_left(&mut self) {
        let head = self.selection.head.saturating_sub(1);
        self.selection = Selection::caret(head);
    }

    /// Move the caret one character right, clamped to buffer length.
    pub fn move_right(&mut self) {
        let head = (self.selection.head + 1).min(self.buffer.len_chars());
        self.selection = Selection::caret(head);
    }

    /// Move the caret one visual line up, preserving column when possible.
    pub fn move_up(&mut self) {
        let (line, col) = self.buffer.line_col(self.selection.head);
        if line == 0 {
            self.selection = Selection::caret(0);
            return;
        }
        let target_line = line - 1;
        let line_start = self.buffer.line_to_char(target_line);
        let clamped_col = col.min(self.buffer.line_len(target_line));
        self.selection = Selection::caret(line_start + clamped_col);
    }

    /// Move the caret one visual line down, preserving column when possible.
    pub fn move_down(&mut self) {
        let (line, col) = self.buffer.line_col(self.selection.head);
        let last_line = self.buffer.len_lines().saturating_sub(1);
        if line >= last_line {
            self.selection = Selection::caret(self.buffer.len_chars());
            return;
        }
        let target_line = line + 1;
        let line_start = self.buffer.line_to_char(target_line);
        let clamped_col = col.min(self.buffer.line_len(target_line));
        self.selection = Selection::caret(line_start + clamped_col);
    }

    pub fn undo(&mut self) {
        let Some(edit) = self.undo.pop() else { return };
        edit.invert().apply(&mut self.buffer, &mut self.selection);
        self.redo.push(edit);
    }

    pub fn redo(&mut self) {
        let Some(edit) = self.redo.pop() else { return };
        edit.apply(&mut self.buffer, &mut self.selection);
        self.undo.push(edit);
    }

    /// `(line, column)` for the current caret, zero-based.
    pub fn cursor_line_col(&self) -> (usize, usize) {
        self.buffer.line_col(self.selection.head)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn state_from(s: &str) -> EditorState {
        EditorState::new(s.parse().unwrap())
    }

    #[test]
    fn insert_advances_caret() {
        let mut st = state_from("");
        st.insert_str("hi");
        assert_eq!(st.buffer.to_string(), "hi");
        assert_eq!(st.selection.head, 2);
    }

    #[test]
    fn backspace_removes_previous_char() {
        let mut st = state_from("");
        st.insert_str("hello");
        st.backspace();
        assert_eq!(st.buffer.to_string(), "hell");
        assert_eq!(st.selection.head, 4);
    }

    #[test]
    fn backspace_at_start_is_noop() {
        let mut st = state_from("abc");
        st.backspace();
        assert_eq!(st.buffer.to_string(), "abc");
        assert_eq!(st.selection.head, 0);
    }

    #[test]
    fn undo_redo_insert() {
        let mut st = state_from("");
        st.insert_str("hi");
        st.undo();
        assert_eq!(st.buffer.to_string(), "");
        assert_eq!(st.selection.head, 0);
        st.redo();
        assert_eq!(st.buffer.to_string(), "hi");
        assert_eq!(st.selection.head, 2);
    }

    #[test]
    fn undo_redo_backspace() {
        let mut st = state_from("");
        st.insert_str("hello");
        st.backspace();
        assert_eq!(st.buffer.to_string(), "hell");
        st.undo();
        assert_eq!(st.buffer.to_string(), "hello");
        assert_eq!(st.selection.head, 5);
        st.redo();
        assert_eq!(st.buffer.to_string(), "hell");
    }

    #[test]
    fn new_edit_clears_redo() {
        let mut st = state_from("");
        st.insert_str("a");
        st.undo();
        st.insert_str("b");
        // Redo stack should have been cleared by the "b" insert.
        st.redo();
        assert_eq!(st.buffer.to_string(), "b");
    }

    #[test]
    fn arrow_move_clamps() {
        let mut st = state_from("abc");
        st.move_left();
        assert_eq!(st.selection.head, 0);
        st.move_right();
        st.move_right();
        st.move_right();
        st.move_right();
        st.move_right();
        assert_eq!(st.selection.head, 3); // clamped to len
    }

    #[test]
    fn move_up_down_preserves_column() {
        let mut st = state_from("abcdef\nghi\njklmno");
        // Start at column 4 on line 0.
        for _ in 0..4 {
            st.move_right();
        }
        assert_eq!(st.cursor_line_col(), (0, 4));

        st.move_down();
        // "ghi" only has 3 columns — caret clamps to end of line 1.
        assert_eq!(st.cursor_line_col(), (1, 3));

        st.move_down();
        // Column 3 survives on line 2 because the original desired column
        // (4) was clamped but the raw head index math still lands at col 3.
        assert_eq!(st.cursor_line_col().0, 2);
    }

    #[test]
    fn replace_buffer_clears_history() {
        let mut st = state_from("");
        st.insert_str("a");
        st.replace_buffer("other".parse().unwrap());
        st.undo();
        assert_eq!(st.buffer.to_string(), "other");
    }

    #[test]
    fn typing_over_selection_replaces_it() {
        let mut st = state_from("hello world");
        st.selection = Selection { anchor: 0, head: 5 };
        st.insert_str("howdy");
        assert_eq!(st.buffer.to_string(), "howdy world");
        assert_eq!(st.selection.head, 5);
    }
}
