//! editor_core — the buffer + selection model that backs every atomio editor view.
//!
//! v0.0: a thin wrapper over `ropey` so the rest of the codebase can depend on a
//! stable `Buffer` type while we iterate on the underlying representation.
//! A CRDT layer will slot in here later (see ROADMAP — collab is post-v1.0).

use std::convert::Infallible;
use std::fmt;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::str::FromStr;

use ropey::Rope;

/// A text buffer. One per open file.
#[derive(Debug, Clone, Default)]
pub struct Buffer {
    rope: Rope,
    path: Option<PathBuf>,
    dirty: bool,
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

    pub fn insert(&mut self, char_idx: usize, text: &str) {
        self.rope.insert(char_idx, text);
        self.dirty = true;
    }

    pub fn remove(&mut self, range: std::ops::Range<usize>) {
        self.rope.remove(range);
        self.dirty = true;
    }
}

impl FromStr for Buffer {
    type Err = Infallible;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Ok(Self {
            rope: Rope::from_str(s),
            path: None,
            dirty: false,
        })
    }
}

impl fmt::Display for Buffer {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.rope)
    }
}

/// A single cursor + selection. `head == anchor` means a caret with no selection.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Selection {
    pub anchor: usize,
    pub head: usize,
}

impl Selection {
    pub fn caret(pos: usize) -> Self {
        Self {
            anchor: pos,
            head: pos,
        }
    }

    pub fn is_caret(&self) -> bool {
        self.anchor == self.head
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
    fn selection_caret() {
        let s = Selection::caret(7);
        assert!(s.is_caret());
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
    fn open_then_save_roundtrip() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("greet.txt");
        std::fs::write(&path, "one\ntwo\n").unwrap();

        let mut b = Buffer::open(&path).unwrap();
        assert_eq!(b.path(), Some(path.as_path()));
        assert!(!b.is_dirty());
        assert_eq!(b.len_lines(), 3); // "one", "two", trailing empty

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
}
