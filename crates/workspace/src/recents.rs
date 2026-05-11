//! Recents list persisted to disk.
//!
//! Stores up to [`Recents::MAX`] most recently opened project paths
//! plus the timestamp of last use. Serialised as JSON so the UI can
//! restore on launch and offer a one-click reopen.
//!
//! Persistence is the caller's responsibility -- this module only
//! handles the in-memory list + JSON encode/decode. The atomio shell
//! wires it to `~/Library/Application Support/atomio/state.json`.

use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};

/// One entry in the recents list. Timestamp is unix-seconds for easy
/// JSON round-trip; we don't need sub-second precision here.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RecentsEntry {
    /// Absolute path on disk. Stays as `PathBuf` so the UI can pass
    /// it back to [`crate::Workspace::open`] without re-parsing.
    pub path: PathBuf,
    /// Project display name (manifest name when available, otherwise
    /// the directory basename). Cached to avoid re-reading the
    /// manifest on the launch screen.
    pub name: String,
    /// Last opened, unix seconds.
    pub last_opened: u64,
}

/// In-memory recents list. Newest first. Capped at [`Self::MAX`].
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Recents {
    entries: Vec<RecentsEntry>,
}

impl Recents {
    /// Maximum entries kept. Older entries fall off when [`Self::push`]
    /// crosses the cap.
    pub const MAX: usize = 10;

    /// New empty list.
    pub fn new() -> Self {
        Self::default()
    }

    /// Insert or bump `(path, name)` to the front. If an entry with
    /// the same path already exists, its timestamp is refreshed and
    /// the entry moves to position 0; otherwise a new entry is pushed
    /// and the list is trimmed to [`Self::MAX`].
    pub fn push(&mut self, path: impl Into<PathBuf>, name: impl Into<String>) {
        let path = path.into();
        let name = name.into();
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_secs())
            .unwrap_or(0);
        // Remove any existing match to avoid duplicates.
        self.entries.retain(|e| e.path != path);
        self.entries.insert(
            0,
            RecentsEntry {
                path,
                name,
                last_opened: now,
            },
        );
        if self.entries.len() > Self::MAX {
            self.entries.truncate(Self::MAX);
        }
    }

    /// All entries, newest first.
    pub fn entries(&self) -> &[RecentsEntry] {
        &self.entries
    }

    /// Drop the entry whose `path` matches. Useful when the user
    /// removed the project from disk; we let the UI prune on click.
    pub fn forget(&mut self, path: &Path) {
        self.entries.retain(|e| e.path != path);
    }

    /// Number of entries currently tracked.
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    /// Whether the list is empty.
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    /// Drop everything.
    pub fn clear(&mut self) {
        self.entries.clear();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn push_newest_first() {
        let mut r = Recents::new();
        r.push("/a", "a");
        r.push("/b", "b");
        assert_eq!(r.entries()[0].path, PathBuf::from("/b"));
        assert_eq!(r.entries()[1].path, PathBuf::from("/a"));
    }

    #[test]
    fn push_existing_moves_to_front() {
        let mut r = Recents::new();
        r.push("/a", "a");
        r.push("/b", "b");
        r.push("/a", "a-renamed");
        assert_eq!(r.entries().len(), 2);
        assert_eq!(r.entries()[0].path, PathBuf::from("/a"));
        assert_eq!(r.entries()[0].name, "a-renamed");
    }

    #[test]
    fn caps_at_max() {
        let mut r = Recents::new();
        for i in 0..(Recents::MAX + 5) {
            r.push(format!("/p{i}"), format!("p{i}"));
        }
        assert_eq!(r.entries().len(), Recents::MAX);
        // Oldest pushed (p0..p4) fell off; newest (last push) is at front.
        assert_eq!(
            r.entries()[0].path,
            PathBuf::from(format!("/p{}", Recents::MAX + 4))
        );
    }

    #[test]
    fn forget_removes_matching_path() {
        let mut r = Recents::new();
        r.push("/a", "a");
        r.push("/b", "b");
        r.forget(Path::new("/a"));
        assert_eq!(r.entries().len(), 1);
        assert_eq!(r.entries()[0].path, PathBuf::from("/b"));
    }

    #[test]
    fn json_round_trip() {
        let mut r = Recents::new();
        r.push("/foo", "foo");
        r.push("/bar", "bar");
        let json = serde_json::to_string(&r).unwrap();
        let back: Recents = serde_json::from_str(&json).unwrap();
        assert_eq!(back.entries().len(), 2);
        assert_eq!(back.entries()[0].path, PathBuf::from("/bar"));
    }

    #[test]
    fn clear_empties() {
        let mut r = Recents::new();
        r.push("/a", "a");
        r.clear();
        assert!(r.is_empty());
    }
}
