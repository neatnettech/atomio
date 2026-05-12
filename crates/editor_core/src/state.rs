//! Editor state — the smallest complete "interactive buffer": a buffer, a
//! caret, and an undo/redo history.
//!
//! All user-facing editing operations (insert, backspace, cursor moves,
//! undo, redo) route through this type so the rules live in one place and
//! can be unit-tested without a GUI.
//!
//! ### Undo model
//!
//! The history is a pair of stacks of **groups** of `Edit` operations.
//! Each group is one user-perceptible undo step: typing a run of
//! letters at the caret produces one group, not one group per
//! keystroke. Boundaries break a run open (start a new group) on
//! cursor moves, selection changes, newlines, and any non-edit action
//! that changes the caret. `undo` pops the top group, applies every
//! edit's inverse in reverse order, and pushes the group onto the
//! redo stack; `redo` is the mirror.
//!
//! Group-based history maps cleanly onto a CRDT layer later -- a
//! group becomes one logical "transaction" rather than N atomic ops.

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

    /// Whether this edit can be appended to the current run started by
    /// `prev` without breaking the user's intuition about "one undo
    /// step". Only single-character contiguous typing or backspace /
    /// delete-forward runs qualify; newlines always start their own
    /// group so an unfortunate undo doesn't roll back a whole
    /// paragraph in one go.
    fn extends(&self, prev: &Edit) -> bool {
        match (prev, self) {
            (Edit::Insert { at: pa, text: pt }, Edit::Insert { at: na, text: nt }) => {
                let prev_len = pt.chars().count();
                prev_len == 1
                    && nt.chars().count() == 1
                    && *na == *pa + prev_len
                    && !pt.contains('\n')
                    && !nt.contains('\n')
            }
            (Edit::Remove { at: pa, text: pt }, Edit::Remove { at: na, text: nt }) => {
                let single = pt.chars().count() == 1 && nt.chars().count() == 1;
                if !single || pt.contains('\n') || nt.contains('\n') {
                    return false;
                }
                // Backspace run: new removal is one char to the left.
                let backspace = *na + 1 == *pa;
                // Delete-forward run: new removal is at the same
                // position (the buffer shifted under us).
                let forward = *na == *pa;
                backspace || forward
            }
            _ => false,
        }
    }
}

/// One user-perceptible undo step. A non-empty sequence of edits plus
/// the caret state from before the group's first edit, so undo can
/// restore the user to exactly where they started the run instead of
/// wherever the inverse of the earliest edit happens to leave them.
#[derive(Debug, Clone)]
struct UndoGroup {
    /// Selection before the group's first edit ran. `undo` restores
    /// this so a delete-forward run from caret 0 ends at caret 0
    /// rather than 1 (where the last applied `Insert(0, ...)` inverse
    /// would otherwise leave the cursor).
    pre_selection: Selection,
    edits: Vec<Edit>,
}

/// A buffer plus the state needed to edit it interactively.
#[derive(Debug, Clone, Default)]
pub struct EditorState {
    pub buffer: Buffer,
    pub selection: Selection,
    undo: Vec<UndoGroup>,
    redo: Vec<UndoGroup>,
    /// When true, all mutating ops are no-ops. Used for Metro-served
    /// source files where edits would diverge from what the runtime
    /// actually executed.
    read_only: bool,
    /// When set, the next edit starts a fresh undo group instead of
    /// extending the current one. Flipped on cursor moves, selection
    /// changes, replace_buffer, and after undo/redo so the next typed
    /// character lands in its own group rather than merging with the
    /// just-restored history.
    break_group: bool,
}

impl EditorState {
    pub fn new(buffer: Buffer) -> Self {
        Self {
            buffer,
            selection: Selection::caret(0),
            undo: Vec::new(),
            redo: Vec::new(),
            read_only: false,
            break_group: true,
        }
    }

    /// Append `edit` to the current undo group when it continues a
    /// typing run, or start a fresh group otherwise. Always clears
    /// the redo stack -- a new edit invalidates the redo timeline.
    /// Resets the break flag so subsequent typing in the same run
    /// merges in.
    ///
    /// `pre_selection` is the caret state from before this edit was
    /// applied. It's stored on the group's first edit so undo can
    /// restore it; subsequent merges keep the original pre-selection
    /// because that's where the user actually started the run.
    fn push_edit(&mut self, edit: Edit, pre_selection: Selection) {
        let merged = !self.break_group
            && self
                .undo
                .last()
                .and_then(|g| g.edits.last())
                .is_some_and(|last| edit.extends(last));
        if merged {
            self.undo.last_mut().unwrap().edits.push(edit);
        } else {
            self.undo.push(UndoGroup {
                pre_selection,
                edits: vec![edit],
            });
        }
        self.redo.clear();
        self.break_group = false;
    }

    /// Whether the buffer rejects mutating operations.
    pub fn is_read_only(&self) -> bool {
        self.read_only
    }

    /// Set the read-only flag. Toggling does not touch buffer contents
    /// or history.
    pub fn set_read_only(&mut self, read_only: bool) {
        self.read_only = read_only;
    }

    /// Replace the backing buffer (e.g. after opening a new file). Clears
    /// history because the old edits no longer apply to the new content.
    /// `read_only` carries through `replace_buffer` (per-buffer flag must
    /// be set explicitly).
    pub fn replace_buffer(&mut self, buffer: Buffer) {
        self.buffer = buffer;
        self.selection = Selection::caret(0);
        self.undo.clear();
        self.redo.clear();
        self.break_group = true;
    }

    /// Insert `text` at the caret, replacing any active selection first.
    pub fn insert_str(&mut self, text: &str) {
        if self.read_only || text.is_empty() {
            return;
        }
        self.delete_selection();
        let pre = self.selection;
        let at = self.selection.head;
        let edit = Edit::Insert {
            at,
            text: text.to_string(),
        };
        edit.apply(&mut self.buffer, &mut self.selection);
        self.push_edit(edit, pre);
    }

    /// Delete the character before the caret. If a selection is active,
    /// delete the selection instead.
    pub fn backspace(&mut self) {
        if self.read_only {
            return;
        }
        if !self.selection.is_caret() {
            self.delete_selection();
            return;
        }
        let head = self.selection.head;
        if head == 0 {
            return;
        }
        let pre = self.selection;
        let at = head - 1;
        let removed = self.buffer.slice_to_string(at..head);
        let edit = Edit::Remove { at, text: removed };
        edit.apply(&mut self.buffer, &mut self.selection);
        self.push_edit(edit, pre);
    }

    /// Delete the character after the caret, or the active selection.
    pub fn delete_forward(&mut self) {
        if self.read_only {
            return;
        }
        if !self.selection.is_caret() {
            self.delete_selection();
            return;
        }
        let head = self.selection.head;
        if head >= self.buffer.len_chars() {
            return;
        }
        let pre = self.selection;
        let removed = self.buffer.slice_to_string(head..head + 1);
        let edit = Edit::Remove {
            at: head,
            text: removed,
        };
        edit.apply(&mut self.buffer, &mut self.selection);
        self.push_edit(edit, pre);
    }

    /// Return the currently selected text, or `None` if the selection is a
    /// caret. Used by the UI layer for copy / cut.
    pub fn selected_text(&self) -> Option<String> {
        if self.selection.is_caret() {
            None
        } else {
            Some(self.buffer.slice_to_string(self.selection.range()))
        }
    }

    /// Remove the active selection and return its text. Returns `None` when
    /// the selection is a caret (nothing to cut).
    pub fn cut_selection(&mut self) -> Option<String> {
        if self.read_only {
            return None;
        }
        let text = self.selected_text()?;
        self.delete_selection();
        Some(text)
    }

    fn delete_selection(&mut self) {
        if self.read_only || self.selection.is_caret() {
            return;
        }
        let pre = self.selection;
        let range = self.selection.range();
        let removed = self.buffer.slice_to_string(range.clone());
        let edit = Edit::Remove {
            at: range.start,
            text: removed,
        };
        edit.apply(&mut self.buffer, &mut self.selection);
        self.push_edit(edit, pre);
    }

    /// Move the caret to `new_head`. When `extend` is true the anchor is
    /// preserved (growing / shrinking the selection); when false the
    /// selection collapses to a caret at the new position. Always
    /// breaks the active undo group -- a user-initiated cursor move
    /// is a clear signal that subsequent typing is a fresh edit run.
    fn set_head(&mut self, new_head: usize, extend: bool) {
        let new_head = new_head.min(self.buffer.len_chars());
        if extend {
            self.selection.head = new_head;
        } else {
            self.selection = Selection::caret(new_head);
        }
        self.break_group = true;
    }

    fn head_line_left(&self) -> usize {
        self.selection.head.saturating_sub(1)
    }

    fn head_line_right(&self) -> usize {
        self.selection.head + 1
    }

    fn head_line_up(&self) -> usize {
        let (line, col) = self.buffer.line_col(self.selection.head);
        if line == 0 {
            return 0;
        }
        let target_line = line - 1;
        let line_start = self.buffer.line_to_char(target_line);
        let clamped_col = col.min(self.buffer.line_len(target_line));
        line_start + clamped_col
    }

    fn head_line_down(&self) -> usize {
        let (line, col) = self.buffer.line_col(self.selection.head);
        let last_line = self.buffer.len_lines().saturating_sub(1);
        if line >= last_line {
            return self.buffer.len_chars();
        }
        let target_line = line + 1;
        let line_start = self.buffer.line_to_char(target_line);
        let clamped_col = col.min(self.buffer.line_len(target_line));
        line_start + clamped_col
    }

    fn head_line_start(&self) -> usize {
        let (line, _) = self.buffer.line_col(self.selection.head);
        self.buffer.line_to_char(line)
    }

    fn head_line_end(&self) -> usize {
        let (line, _) = self.buffer.line_col(self.selection.head);
        self.buffer.line_to_char(line) + self.buffer.line_len(line)
    }

    // --- non-extending moves: collapse to caret -------------------------

    pub fn move_left(&mut self) {
        self.set_head(self.head_line_left(), false);
    }

    pub fn move_right(&mut self) {
        self.set_head(self.head_line_right(), false);
    }

    pub fn move_up(&mut self) {
        self.set_head(self.head_line_up(), false);
    }

    pub fn move_down(&mut self) {
        self.set_head(self.head_line_down(), false);
    }

    pub fn move_line_start(&mut self) {
        self.set_head(self.head_line_start(), false);
    }

    pub fn move_line_end(&mut self) {
        self.set_head(self.head_line_end(), false);
    }

    /// Move caret to an absolute `(line, col)` position. Both are
    /// 0-indexed and clamped to the buffer (lines beyond EOF land on
    /// the last line; cols beyond line length land at end-of-line).
    /// Clears any active selection.
    pub fn move_cursor_to(&mut self, line: usize, col: usize) {
        let last_line = self.buffer.len_lines().saturating_sub(1);
        let target_line = line.min(last_line);
        let line_start = self.buffer.line_to_char(target_line);
        let clamped_col = col.min(self.buffer.line_len(target_line));
        self.set_head(line_start + clamped_col, false);
    }

    // --- extending moves: preserve anchor -------------------------------

    pub fn move_left_extending(&mut self) {
        self.set_head(self.head_line_left(), true);
    }

    pub fn move_right_extending(&mut self) {
        self.set_head(self.head_line_right(), true);
    }

    pub fn move_up_extending(&mut self) {
        self.set_head(self.head_line_up(), true);
    }

    pub fn move_down_extending(&mut self) {
        self.set_head(self.head_line_down(), true);
    }

    pub fn move_line_start_extending(&mut self) {
        self.set_head(self.head_line_start(), true);
    }

    pub fn move_line_end_extending(&mut self) {
        self.set_head(self.head_line_end(), true);
    }

    /// Select the entire buffer.
    pub fn select_all(&mut self) {
        self.selection = Selection {
            anchor: 0,
            head: self.buffer.len_chars(),
        };
        self.break_group = true;
    }

    /// Undo the most recent edit group. Applies every edit's inverse
    /// in reverse order, then restores the caret state from before
    /// the group's first edit so the user ends up exactly where they
    /// started the run. Moves the group onto the redo stack so the
    /// next `redo()` re-applies them in original order.
    pub fn undo(&mut self) {
        if self.read_only {
            return;
        }
        let Some(group) = self.undo.pop() else { return };
        for edit in group.edits.iter().rev() {
            edit.invert().apply(&mut self.buffer, &mut self.selection);
        }
        self.selection = group.pre_selection;
        self.redo.push(group);
        self.break_group = true;
    }

    /// Redo the most recently undone group. Mirrors [`Self::undo`].
    pub fn redo(&mut self) {
        if self.read_only {
            return;
        }
        let Some(group) = self.redo.pop() else { return };
        for edit in group.edits.iter() {
            edit.apply(&mut self.buffer, &mut self.selection);
        }
        self.undo.push(group);
        self.break_group = true;
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
    fn extending_move_grows_and_collapses() {
        let mut st = state_from("hello");
        st.move_right_extending();
        st.move_right_extending();
        assert_eq!(st.selection.anchor, 0);
        assert_eq!(st.selection.head, 2);
        st.move_left_extending();
        assert_eq!(st.selection.head, 1);
        st.move_right(); // non-extending collapses
        assert!(st.selection.is_caret());
        assert_eq!(st.selection.head, 2);
    }

    #[test]
    fn select_all_spans_buffer() {
        let mut st = state_from("abc\ndef");
        st.select_all();
        assert_eq!(st.selection.anchor, 0);
        assert_eq!(st.selection.head, 7);
    }

    #[test]
    fn line_start_end_moves() {
        let mut st = state_from("abc\ndef");
        for _ in 0..5 {
            st.move_right();
        }
        assert_eq!(st.cursor_line_col(), (1, 1));
        st.move_line_start();
        assert_eq!(st.cursor_line_col(), (1, 0));
        st.move_line_end();
        assert_eq!(st.cursor_line_col(), (1, 3));
    }

    #[test]
    fn move_cursor_to_absolute_position() {
        let mut st = state_from("abc\ndefgh\nij");
        st.move_cursor_to(1, 3);
        assert_eq!(st.cursor_line_col(), (1, 3));
        assert!(st.selection.is_caret());
    }

    #[test]
    fn move_cursor_to_clamps_col_to_line_end() {
        let mut st = state_from("abc\ndef");
        st.move_cursor_to(1, 999);
        assert_eq!(st.cursor_line_col(), (1, 3));
    }

    #[test]
    fn move_cursor_to_clamps_line_to_last() {
        let mut st = state_from("abc\ndef");
        st.move_cursor_to(99, 0);
        assert_eq!(st.cursor_line_col(), (1, 0));
    }

    #[test]
    fn move_cursor_to_clears_selection() {
        let mut st = state_from("abc\ndef");
        st.move_right_extending();
        st.move_right_extending();
        assert!(!st.selection.is_caret());
        st.move_cursor_to(1, 1);
        assert!(st.selection.is_caret());
        assert_eq!(st.cursor_line_col(), (1, 1));
    }

    #[test]
    fn shift_home_extends_to_line_start() {
        let mut st = state_from("abcdef");
        for _ in 0..4 {
            st.move_right();
        }
        st.move_line_start_extending();
        assert_eq!(st.selection.anchor, 4);
        assert_eq!(st.selection.head, 0);
    }

    #[test]
    fn selected_text_and_cut() {
        let mut st = state_from("hello world");
        assert_eq!(st.selected_text(), None);
        for _ in 0..5 {
            st.move_right_extending();
        }
        assert_eq!(st.selected_text().as_deref(), Some("hello"));
        let cut = st.cut_selection();
        assert_eq!(cut.as_deref(), Some("hello"));
        assert_eq!(
            st.buffer.slice_to_string(0..st.buffer.len_chars()),
            " world"
        );
        assert!(st.selection.is_caret());
        assert_eq!(st.selection.head, 0);
        st.undo();
        assert_eq!(
            st.buffer.slice_to_string(0..st.buffer.len_chars()),
            "hello world"
        );
    }

    #[test]
    fn read_only_blocks_mutations() {
        let mut st = state_from("hello");
        st.set_read_only(true);
        st.insert_str(" world");
        assert_eq!(st.buffer.slice_to_string(0..st.buffer.len_chars()), "hello");

        st.move_right();
        st.move_right();
        st.backspace();
        assert_eq!(st.buffer.slice_to_string(0..st.buffer.len_chars()), "hello");

        st.delete_forward();
        assert_eq!(st.buffer.slice_to_string(0..st.buffer.len_chars()), "hello");

        st.select_all();
        assert!(st.cut_selection().is_none());
        assert_eq!(st.buffer.slice_to_string(0..st.buffer.len_chars()), "hello");
    }

    #[test]
    fn read_only_blocks_undo_redo() {
        let mut st = state_from("hello");
        st.move_line_end();
        st.insert_str("!");
        assert_eq!(
            st.buffer.slice_to_string(0..st.buffer.len_chars()),
            "hello!"
        );
        st.set_read_only(true);
        st.undo();
        // Read-only must not undo.
        assert_eq!(
            st.buffer.slice_to_string(0..st.buffer.len_chars()),
            "hello!"
        );
        st.set_read_only(false);
        st.undo();
        assert_eq!(st.buffer.slice_to_string(0..st.buffer.len_chars()), "hello");
    }

    #[test]
    fn read_only_allows_cursor_motion_and_selection() {
        let mut st = state_from("hello");
        st.set_read_only(true);
        st.move_right();
        st.move_right();
        assert_eq!(st.cursor_line_col(), (0, 2));
        st.select_all();
        assert_eq!(st.selected_text().as_deref(), Some("hello"));
    }

    #[test]
    fn typing_over_selection_replaces_it() {
        let mut st = state_from("hello world");
        st.selection = Selection { anchor: 0, head: 5 };
        st.insert_str("howdy");
        assert_eq!(st.buffer.to_string(), "howdy world");
        assert_eq!(st.selection.head, 5);
    }

    // ------- undo grouping -------

    #[test]
    fn typing_a_run_collapses_into_one_undo_step() {
        // Five single-character inserts (simulating real typing) should
        // combine into one undo group, so a single `undo()` rolls back
        // the whole word rather than leaving "hell".
        let mut st = state_from("");
        for c in ["h", "e", "l", "l", "o"] {
            st.insert_str(c);
        }
        assert_eq!(st.buffer.to_string(), "hello");
        st.undo();
        assert_eq!(st.buffer.to_string(), "");
        assert_eq!(st.selection.head, 0);
    }

    #[test]
    fn backspace_run_collapses_into_one_undo_step() {
        let mut st = state_from("hello");
        st.move_line_end();
        for _ in 0..3 {
            st.backspace();
        }
        assert_eq!(st.buffer.to_string(), "he");
        st.undo();
        assert_eq!(st.buffer.to_string(), "hello");
        assert_eq!(st.selection.head, 5);
    }

    #[test]
    fn delete_forward_run_collapses_into_one_undo_step() {
        let mut st = state_from("hello");
        // Caret at start.
        for _ in 0..3 {
            st.delete_forward();
        }
        assert_eq!(st.buffer.to_string(), "lo");
        st.undo();
        assert_eq!(st.buffer.to_string(), "hello");
        assert_eq!(st.selection.head, 0);
    }

    #[test]
    fn cursor_move_breaks_the_typing_run() {
        let mut st = state_from("");
        st.insert_str("a");
        st.insert_str("b");
        st.move_left();
        st.move_right();
        st.insert_str("c");
        // After the move, "c" lands in a fresh group. The first undo
        // pops just "c"; the second pops "ab".
        assert_eq!(st.buffer.to_string(), "abc");
        st.undo();
        assert_eq!(st.buffer.to_string(), "ab");
        st.undo();
        assert_eq!(st.buffer.to_string(), "");
    }

    #[test]
    fn switching_edit_type_breaks_the_run() {
        // Typing then backspacing must not merge into one group, even
        // though both end at the same caret position.
        let mut st = state_from("");
        st.insert_str("a");
        st.insert_str("b");
        st.backspace();
        assert_eq!(st.buffer.to_string(), "a");
        st.undo(); // restore the backspaced char
        assert_eq!(st.buffer.to_string(), "ab");
        st.undo(); // undo the typing run
        assert_eq!(st.buffer.to_string(), "");
    }

    #[test]
    fn newline_starts_its_own_group() {
        // Typing "ab\ncd" produces three groups: "ab", "\n", "cd". An
        // unfortunate undo never rolls back a full paragraph in one go.
        let mut st = state_from("");
        st.insert_str("a");
        st.insert_str("b");
        st.insert_str("\n");
        st.insert_str("c");
        st.insert_str("d");
        assert_eq!(st.buffer.to_string(), "ab\ncd");
        st.undo();
        assert_eq!(st.buffer.to_string(), "ab\n");
        st.undo();
        assert_eq!(st.buffer.to_string(), "ab");
        st.undo();
        assert_eq!(st.buffer.to_string(), "");
    }

    #[test]
    fn undo_then_type_starts_fresh_group() {
        // After an undo, the next typed char must not merge with the
        // edit that was just popped back onto the redo stack -- the
        // user's mental model is that the typing run ended.
        let mut st = state_from("");
        st.insert_str("a");
        st.insert_str("b");
        st.undo();
        assert_eq!(st.buffer.to_string(), "");
        st.insert_str("c");
        // First undo rolls back "c"; second undo would normally be a
        // no-op but the redo we cleared by typing "c" never gets
        // re-applied, so the buffer ends up empty.
        st.undo();
        assert_eq!(st.buffer.to_string(), "");
    }

    #[test]
    fn multi_char_insert_str_is_one_group() {
        // The legacy contract: a single `insert_str("hello")` call is
        // one undo step regardless of the new grouping rules.
        let mut st = state_from("");
        st.insert_str("hello");
        st.undo();
        assert_eq!(st.buffer.to_string(), "");
    }

    #[test]
    fn replace_buffer_resets_the_break_flag_and_history() {
        let mut st = state_from("");
        st.insert_str("a");
        st.replace_buffer("new".parse().unwrap());
        // After replace_buffer the caret is at 0, so typing inserts at
        // the start of the new buffer. The "xy" run is one group, and
        // a single undo restores it to the seed.
        st.insert_str("x");
        st.insert_str("y");
        assert_eq!(st.buffer.to_string(), "xynew");
        st.undo();
        assert_eq!(st.buffer.to_string(), "new");
    }
}
