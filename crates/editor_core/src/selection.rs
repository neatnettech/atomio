//! Cursor + selection primitives.
//!
//! A [`Selection`] is a pair of character indices into a buffer: an anchor
//! (where the selection started) and a head (where the caret currently is).
//! When the two are equal the selection is a caret with no highlighted
//! range. Multi-cursor support lives above this type in `EditorState` and
//! will be added post-v0.0.

/// A single cursor + selection. `head == anchor` means a caret with no selection.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
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

    /// Lower bound of the selected range (inclusive).
    pub fn start(&self) -> usize {
        self.anchor.min(self.head)
    }

    /// Upper bound of the selected range (exclusive).
    pub fn end(&self) -> usize {
        self.anchor.max(self.head)
    }

    pub fn range(&self) -> std::ops::Range<usize> {
        self.start()..self.end()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn caret_has_zero_range() {
        let s = Selection::caret(7);
        assert!(s.is_caret());
        assert_eq!(s.range(), 7..7);
    }

    #[test]
    fn range_is_order_insensitive() {
        let forward = Selection { anchor: 2, head: 5 };
        let backward = Selection { anchor: 5, head: 2 };
        assert_eq!(forward.range(), 2..5);
        assert_eq!(backward.range(), 2..5);
    }
}
