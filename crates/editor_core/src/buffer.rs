//! Text buffer backed by a rope.
//!
//! Owns on-disk identity (path, dirty flag) plus the rope itself. Kept
//! deliberately free of cursor/selection state — that lives in
//! [`crate::state::EditorState`] — so it can be reused by non-interactive
//! callers (formatters, linters, tests).
//!
//! ### Anchors
//!
//! [`Buffer`] also tracks a set of [`Anchor`]s -- opaque IDs that
//! identify a logical character position that **survives edits**. When
//! text is inserted before an anchor the anchor shifts forward; when
//! text is deleted before it the anchor shifts back; when text is
//! deleted *containing* it the anchor collapses to the deletion's
//! start so it stays alive rather than vanishing. This is the
//! foundation breakpoints will use to stay glued to their source
//! line through a save+reformat (separate PR plumbs it in).
//!
//! Insertion bias is **right**: a new character typed at the anchor's
//! exact position pushes the anchor past it. That matches the
//! breakpoint UX -- inserting a line above line 5 should move the
//! breakpoint to the new line 6, not leave it stuck on the new
//! (empty) line 5.

use std::collections::BTreeMap;
use std::convert::Infallible;
use std::fmt;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::str::FromStr;

use ropey::Rope;

/// An opaque handle to a character position inside a [`Buffer`] that
/// survives edits. Cheap to clone; ordering and hashing are stable so
/// anchors can live in maps and sets. The underlying number is a
/// per-buffer monotonic counter and is not meaningful outside the
/// buffer that issued it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Ord, PartialOrd)]
pub struct Anchor(u64);

/// A text buffer. One per open file.
#[derive(Debug, Clone, Default)]
pub struct Buffer {
    rope: Rope,
    path: Option<PathBuf>,
    dirty: bool,
    /// Anchor id -> current character position. Updated in-place by
    /// every `insert` / `remove` so callers never have to re-resolve.
    anchors: BTreeMap<Anchor, usize>,
    /// Next anchor id; bumped on every [`Buffer::insert_anchor`]. We
    /// don't reuse freed ids because keeping them monotonic makes
    /// ordering well-defined even when ids cross buffer instances
    /// (e.g. via a serialized debug log).
    next_anchor: u64,
}

impl Buffer {
    pub fn new() -> Self {
        Self::default()
    }

    /// Load a buffer from disk. The path is retained so `save` works without
    /// a "save as" dialog on subsequent writes.
    pub fn open(path: impl AsRef<Path>) -> io::Result<Self> {
        let path = path.as_ref();
        let text = fs::read_to_string(path)?;
        Ok(Self {
            rope: Rope::from_str(&text),
            path: Some(path.to_path_buf()),
            dirty: false,
            anchors: BTreeMap::new(),
            next_anchor: 0,
        })
    }

    /// Save to the buffer's current path. Errors if no path is set — callers
    /// should route that case through `save_as` (typically via a native
    /// dialog).
    pub fn save(&mut self) -> io::Result<()> {
        let path = self
            .path
            .clone()
            .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidInput, "buffer has no path"))?;
        self.save_as(path)
    }

    /// Save to an explicit path and adopt it as the buffer's path.
    pub fn save_as(&mut self, path: impl AsRef<Path>) -> io::Result<()> {
        let path = path.as_ref();
        fs::write(path, self.rope.to_string())?;
        self.path = Some(path.to_path_buf());
        self.dirty = false;
        Ok(())
    }

    pub fn path(&self) -> Option<&Path> {
        self.path.as_deref()
    }

    pub fn is_dirty(&self) -> bool {
        self.dirty
    }

    pub fn len_chars(&self) -> usize {
        self.rope.len_chars()
    }

    pub fn len_lines(&self) -> usize {
        self.rope.len_lines()
    }

    /// Return the `(line, column)` pair for a character index.
    /// Both are zero-based. Column is measured in characters, not bytes.
    pub fn line_col(&self, char_idx: usize) -> (usize, usize) {
        let idx = char_idx.min(self.rope.len_chars());
        let line = self.rope.char_to_line(idx);
        let line_start = self.rope.line_to_char(line);
        (line, idx - line_start)
    }

    /// Return the character index at the start of `line`.
    pub fn line_to_char(&self, line: usize) -> usize {
        let line = line.min(self.rope.len_lines().saturating_sub(1));
        self.rope.line_to_char(line)
    }

    /// Length of a given line in characters, excluding the trailing newline.
    pub fn line_len(&self, line: usize) -> usize {
        if line >= self.rope.len_lines() {
            return 0;
        }
        let slice = self.rope.line(line);
        let len = slice.len_chars();
        // Ropey's `line()` includes the trailing '\n' (if any). Strip it so
        // column math matches what a user perceives.
        if len > 0 && slice.char(len - 1) == '\n' {
            len - 1
        } else {
            len
        }
    }

    /// Return the text of `line` as an owned `String`, excluding any
    /// trailing newline. Out-of-range lines return an empty string so the
    /// UI can iterate `0..len_lines()` without bounds-checking every tick.
    pub fn line_text(&self, line: usize) -> String {
        if line >= self.rope.len_lines() {
            return String::new();
        }
        let len = self.line_len(line);
        let start = self.rope.line_to_char(line);
        self.rope.slice(start..start + len).to_string()
    }

    pub fn insert(&mut self, char_idx: usize, text: &str) {
        let count = text.chars().count();
        self.rope.insert(char_idx, text);
        self.dirty = true;
        if count > 0 {
            self.shift_anchors_for_insert(char_idx, count);
        }
    }

    pub fn remove(&mut self, range: std::ops::Range<usize>) {
        let len = range.end.saturating_sub(range.start);
        self.rope.remove(range.clone());
        self.dirty = true;
        if len > 0 {
            self.shift_anchors_for_remove(range, len);
        }
    }

    /// Read a character range as an owned `String`. Used by the undo stack
    /// to capture what was deleted before removing it.
    pub fn slice_to_string(&self, range: std::ops::Range<usize>) -> String {
        self.rope.slice(range).to_string()
    }

    // --- anchors ------------------------------------------------------------

    /// Create a new anchor at character position `at`. `at` is
    /// clamped to the current buffer length so callers don't have to
    /// guard for stale offsets. The returned [`Anchor`] tracks the
    /// position across subsequent edits.
    pub fn insert_anchor(&mut self, at: usize) -> Anchor {
        let id = Anchor(self.next_anchor);
        self.next_anchor += 1;
        let at = at.min(self.rope.len_chars());
        self.anchors.insert(id, at);
        id
    }

    /// Current character position of `anchor`, or `None` if it has
    /// been removed via [`Buffer::remove_anchor`]. Anchors whose
    /// position falls inside a deleted range survive the deletion --
    /// they collapse to the deletion's start byte rather than being
    /// invalidated.
    pub fn anchor_position(&self, anchor: Anchor) -> Option<usize> {
        self.anchors.get(&anchor).copied()
    }

    /// Convenience: anchor position resolved to `(line, column)`.
    /// `None` matches [`Buffer::anchor_position`]; otherwise the
    /// result is whatever [`Buffer::line_col`] returns for the
    /// current character offset.
    pub fn anchor_line_col(&self, anchor: Anchor) -> Option<(usize, usize)> {
        self.anchor_position(anchor).map(|pos| self.line_col(pos))
    }

    /// Drop `anchor`. Returns the position it was tracking, or
    /// `None` if it was already gone. Subsequent queries with this
    /// id return `None`.
    pub fn remove_anchor(&mut self, anchor: Anchor) -> Option<usize> {
        self.anchors.remove(&anchor)
    }

    /// Number of anchors currently held. Exposed for tests and
    /// instrumentation; not a hot-path concern.
    pub fn anchor_count(&self) -> usize {
        self.anchors.len()
    }

    /// Shift every anchor at or past `at` forward by `count` chars.
    /// RightBias: anchors at the exact insertion point get pushed
    /// past the newly inserted text rather than swallowed by it.
    fn shift_anchors_for_insert(&mut self, at: usize, count: usize) {
        for pos in self.anchors.values_mut() {
            if *pos >= at {
                *pos += count;
            }
        }
    }

    /// Shift every anchor for a removal of `len` chars covering
    /// `range`. Anchors past the deletion shift back; anchors inside
    /// it collapse to the deletion's start byte so they survive a
    /// containing delete (a breakpoint inside a deleted function
    /// ends up at the start of where that function used to be,
    /// rather than vanishing).
    fn shift_anchors_for_remove(&mut self, range: std::ops::Range<usize>, len: usize) {
        for pos in self.anchors.values_mut() {
            if *pos >= range.end {
                *pos -= len;
            } else if *pos > range.start {
                *pos = range.start;
            }
        }
    }
}

impl FromStr for Buffer {
    type Err = Infallible;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Ok(Self {
            rope: Rope::from_str(s),
            path: None,
            dirty: false,
            anchors: BTreeMap::new(),
            next_anchor: 0,
        })
    }
}

impl fmt::Display for Buffer {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.rope)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn buffer_roundtrip() {
        let mut b: Buffer = "hello".parse().unwrap();
        b.insert(5, ", atomio");
        assert_eq!(b.to_string(), "hello, atomio");
        assert_eq!(b.len_chars(), 13);
    }

    #[test]
    fn insert_marks_dirty() {
        let mut b = Buffer::new();
        assert!(!b.is_dirty());
        b.insert(0, "x");
        assert!(b.is_dirty());
    }

    #[test]
    fn remove_range() {
        let mut b: Buffer = "hello, atomio".parse().unwrap();
        b.remove(5..13);
        assert_eq!(b.to_string(), "hello");
        assert_eq!(b.len_chars(), 5);
    }

    #[test]
    fn line_col_basics() {
        let b: Buffer = "one\ntwo\nthree".parse().unwrap();
        assert_eq!(b.line_col(0), (0, 0));
        assert_eq!(b.line_col(3), (0, 3)); // end of "one"
        assert_eq!(b.line_col(4), (1, 0)); // start of "two"
        assert_eq!(b.line_col(8), (2, 0)); // start of "three"
        assert_eq!(b.line_col(13), (2, 5));
    }

    #[test]
    fn line_text_strips_trailing_newline() {
        let b: Buffer = "one\ntwo\nthree".parse().unwrap();
        assert_eq!(b.line_text(0), "one");
        assert_eq!(b.line_text(1), "two");
        assert_eq!(b.line_text(2), "three");
        // Out of range is empty, not a panic.
        assert_eq!(b.line_text(99), "");
    }

    #[test]
    fn line_text_empty_trailing_line() {
        // A trailing '\n' produces an extra empty line in ropey's model —
        // the UI renders it as a blank row, so we need it to come back as
        // an empty string not a panic.
        let b: Buffer = "one\n".parse().unwrap();
        assert_eq!(b.len_lines(), 2);
        assert_eq!(b.line_text(0), "one");
        assert_eq!(b.line_text(1), "");
    }

    #[test]
    fn line_len_excludes_newline() {
        let b: Buffer = "abc\nde\n".parse().unwrap();
        assert_eq!(b.line_len(0), 3);
        assert_eq!(b.line_len(1), 2);
    }

    #[test]
    fn slice_to_string_roundtrip() {
        let b: Buffer = "hello, atomio".parse().unwrap();
        assert_eq!(b.slice_to_string(7..13), "atomio");
    }

    #[test]
    fn open_then_save_roundtrip() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("greet.txt");
        std::fs::write(&path, "one\ntwo\n").unwrap();

        let mut b = Buffer::open(&path).unwrap();
        assert_eq!(b.path(), Some(path.as_path()));
        assert!(!b.is_dirty());
        assert_eq!(b.len_lines(), 3);

        b.insert(b.len_chars(), "three\n");
        assert!(b.is_dirty());

        b.save().unwrap();
        assert!(!b.is_dirty());
        assert_eq!(std::fs::read_to_string(&path).unwrap(), "one\ntwo\nthree\n");
    }

    #[test]
    fn save_without_path_errors() {
        let mut b: Buffer = "unsaved".parse().unwrap();
        let err = b.save().unwrap_err();
        assert_eq!(err.kind(), std::io::ErrorKind::InvalidInput);
    }

    #[test]
    fn save_as_adopts_path() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("out.txt");
        let mut b: Buffer = "content".parse().unwrap();
        b.save_as(&path).unwrap();
        assert_eq!(b.path(), Some(path.as_path()));
        assert!(!b.is_dirty());
        assert_eq!(std::fs::read_to_string(&path).unwrap(), "content");
    }

    // ------- anchors --------------------------------------------------------

    #[test]
    fn anchor_returns_initial_position() {
        let mut b: Buffer = "hello".parse().unwrap();
        let a = b.insert_anchor(3);
        assert_eq!(b.anchor_position(a), Some(3));
        assert_eq!(b.anchor_count(), 1);
    }

    #[test]
    fn anchor_clamps_to_buffer_length() {
        let mut b: Buffer = "abc".parse().unwrap();
        let a = b.insert_anchor(999);
        assert_eq!(b.anchor_position(a), Some(3));
    }

    #[test]
    fn insert_before_anchor_pushes_it_forward() {
        let mut b: Buffer = "hello".parse().unwrap();
        let a = b.insert_anchor(3);
        b.insert(0, "ABC");
        assert_eq!(b.anchor_position(a), Some(6));
    }

    #[test]
    fn insert_after_anchor_leaves_it_alone() {
        let mut b: Buffer = "hello".parse().unwrap();
        let a = b.insert_anchor(2);
        b.insert(4, "X");
        assert_eq!(b.anchor_position(a), Some(2));
    }

    #[test]
    fn insert_at_anchor_position_pushes_right() {
        // RightBias: an anchor at the insertion point gets shoved past
        // the new text rather than staying buried inside it.
        let mut b: Buffer = "hello".parse().unwrap();
        let a = b.insert_anchor(3);
        b.insert(3, "X");
        assert_eq!(b.anchor_position(a), Some(4));
    }

    #[test]
    fn remove_before_anchor_shifts_it_back() {
        let mut b: Buffer = "hello".parse().unwrap();
        let a = b.insert_anchor(4);
        b.remove(0..2);
        assert_eq!(b.anchor_position(a), Some(2));
    }

    #[test]
    fn remove_after_anchor_leaves_it_alone() {
        let mut b: Buffer = "hello".parse().unwrap();
        let a = b.insert_anchor(2);
        b.remove(3..5);
        assert_eq!(b.anchor_position(a), Some(2));
    }

    #[test]
    fn remove_containing_anchor_collapses_to_start() {
        let mut b: Buffer = "abcdef".parse().unwrap();
        let a = b.insert_anchor(3);
        b.remove(2..5);
        // Anchor was inside the removed range -- it survives by
        // collapsing to the deletion's start, so a breakpoint inside
        // a deleted block ends up at the block's old start position
        // rather than vanishing.
        assert_eq!(b.anchor_position(a), Some(2));
    }

    #[test]
    fn remove_anchor_drops_it() {
        let mut b: Buffer = "hello".parse().unwrap();
        let a = b.insert_anchor(2);
        let prev = b.remove_anchor(a);
        assert_eq!(prev, Some(2));
        assert_eq!(b.anchor_position(a), None);
        assert_eq!(b.anchor_count(), 0);
    }

    #[test]
    fn remove_anchor_is_idempotent() {
        let mut b: Buffer = "hello".parse().unwrap();
        let a = b.insert_anchor(2);
        assert_eq!(b.remove_anchor(a), Some(2));
        assert_eq!(b.remove_anchor(a), None);
    }

    #[test]
    fn anchor_line_col_tracks_line_changes() {
        let mut b: Buffer = "one\ntwo\nthree".parse().unwrap();
        let a = b.insert_anchor(8); // start of "three"
        assert_eq!(b.anchor_line_col(a), Some((2, 0)));
        // Insert a new line at the very start. The anchor still points
        // to the same logical "three" but the line index moves down.
        b.insert(0, "zero\n");
        assert_eq!(b.anchor_line_col(a), Some((3, 0)));
    }

    #[test]
    fn multiple_anchors_update_independently() {
        let mut b: Buffer = "hello".parse().unwrap();
        let a1 = b.insert_anchor(1);
        let a2 = b.insert_anchor(4);
        b.insert(2, "XX");
        // a1 at 1 (before the insert) stays put. a2 at 4 (after) shifts.
        assert_eq!(b.anchor_position(a1), Some(1));
        assert_eq!(b.anchor_position(a2), Some(6));
    }

    #[test]
    fn anchor_at_remove_start_stays() {
        // Edge case: anchor at the exact start of the removed range
        // (which is also the new position after removal). Stays where
        // it is rather than collapsing -- nothing to collapse toward.
        let mut b: Buffer = "abcdef".parse().unwrap();
        let a = b.insert_anchor(2);
        b.remove(2..4);
        assert_eq!(b.anchor_position(a), Some(2));
    }

    #[test]
    fn anchor_at_remove_end_collapses_to_start() {
        // Edge case: anchor at the very end of the removed range.
        // After removal that exact char position is gone -- collapse.
        let mut b: Buffer = "abcdef".parse().unwrap();
        let a = b.insert_anchor(4);
        b.remove(2..4);
        // After the remove, what was at position 4 is now at position
        // 2 (the "e" / "f" / etc.). The anchor lands there.
        assert_eq!(b.anchor_position(a), Some(2));
    }

    #[test]
    fn anchor_survives_clone() {
        let mut b: Buffer = "hello".parse().unwrap();
        let a = b.insert_anchor(3);
        let mut copy = b.clone();
        copy.insert(0, "XX");
        // The clone keeps its anchor and updates it in isolation; the
        // original is unaffected.
        assert_eq!(copy.anchor_position(a), Some(5));
        assert_eq!(b.anchor_position(a), Some(3));
    }
}
