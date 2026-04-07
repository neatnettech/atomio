//! editor_core — the buffer + selection model that backs every atomio editor view.
//!
//! v0.0: a thin wrapper over `ropey` so the rest of the codebase can depend on a
//! stable `Buffer` type while we iterate on the underlying representation.
//! A CRDT layer will slot in here later (see ROADMAP — collab is post-v1.0).

use std::convert::Infallible;
use std::fmt;
use std::str::FromStr;

use ropey::Rope;

/// A text buffer. One per open file.
#[derive(Debug, Clone, Default)]
pub struct Buffer {
    rope: Rope,
}

impl Buffer {
    pub fn new() -> Self {
        Self { rope: Rope::new() }
    }

    pub fn len_chars(&self) -> usize {
        self.rope.len_chars()
    }

    pub fn len_lines(&self) -> usize {
        self.rope.len_lines()
    }

    pub fn insert(&mut self, char_idx: usize, text: &str) {
        self.rope.insert(char_idx, text);
    }

    pub fn remove(&mut self, range: std::ops::Range<usize>) {
        self.rope.remove(range);
    }
}

impl FromStr for Buffer {
    type Err = Infallible;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Ok(Self {
            rope: Rope::from_str(s),
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
}
