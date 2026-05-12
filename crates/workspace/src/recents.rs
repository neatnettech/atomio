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

    /// Drop every entry whose `path` is no longer an existing
    /// directory on disk. Returns the number of entries removed so
    /// callers can decide whether to persist + log.
    ///
    /// Permission / I/O errors that aren't `NotFound` are treated
    /// conservatively as "still present" -- a transient EACCES on a
    /// remote mount shouldn't wipe a user's recents. Only outright
    /// missing paths or paths that have been replaced by non-dirs
    /// (a file at the same name) are dropped.
    pub fn prune_missing(&mut self) -> usize {
        let before = self.entries.len();
        self.entries.retain(|e| match std::fs::metadata(&e.path) {
            Ok(meta) => meta.is_dir(),
            Err(err) => err.kind() != std::io::ErrorKind::NotFound,
        });
        before - self.entries.len()
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

    #[test]
    fn prune_missing_drops_nonexistent_paths_keeps_real_dirs() {
        use tempfile::TempDir;
        let real = TempDir::new().unwrap();
        let mut r = Recents::new();
        r.push("/does/not/exist/atomio-tests", "ghost");
        r.push(real.path(), "alive");
        let dropped = r.prune_missing();
        assert_eq!(dropped, 1);
        assert_eq!(r.entries().len(), 1);
        assert_eq!(r.entries()[0].name, "alive");
    }

    #[test]
    fn prune_missing_drops_paths_that_are_now_files_not_dirs() {
        // A path that used to be a project dir but has since been
        // replaced by a regular file is just as stale as one that's
        // disappeared entirely.
        use tempfile::TempDir;
        let tmp = TempDir::new().unwrap();
        let file = tmp.path().join("was-a-project");
        std::fs::write(&file, "").unwrap();
        let mut r = Recents::new();
        r.push(&file, "ex-project");
        assert_eq!(r.prune_missing(), 1);
        assert!(r.is_empty());
    }

    #[test]
    fn prune_missing_returns_zero_when_nothing_to_drop() {
        use tempfile::TempDir;
        let tmp = TempDir::new().unwrap();
        let mut r = Recents::new();
        r.push(tmp.path(), "alive");
        assert_eq!(r.prune_missing(), 0);
        assert_eq!(r.entries().len(), 1);
    }
}
