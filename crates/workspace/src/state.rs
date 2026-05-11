//! Persisted app state.
//!
//! Mirror of the bits of [`crate::Workspace`] + [`crate::Recents`] that
//! survive a restart. Lives as a single JSON file the atomio shell
//! reads on launch and writes on every project open / close.
//!
//! Path resolution stays in the shell -- this module is purely the
//! struct + serde + atomic on-disk write. Keeps `workspace` plain
//! `cargo test` runnable without `dirs::data_dir` hacks.

use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::recents::Recents;

/// Top-level persisted state. Add new fields with `#[serde(default)]`
/// so older state files keep deserialising after a schema bump.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AppState {
    /// Schema version. Bump when a field's meaning changes incompatibly.
    #[serde(default = "default_version")]
    pub version: u32,
    /// Recently opened projects, newest first.
    #[serde(default)]
    pub recents: Recents,
    /// Project to auto-reopen on next launch. `None` keeps the
    /// "single-file mode" path on next start.
    #[serde(default)]
    pub last_project: Option<PathBuf>,
}

fn default_version() -> u32 {
    1
}

impl Default for AppState {
    fn default() -> Self {
        Self {
            version: Self::VERSION,
            recents: Recents::default(),
            last_project: None,
        }
    }
}

impl AppState {
    /// Current schema version this build writes.
    pub const VERSION: u32 = 1;

    /// Load state from `path`. Missing file or malformed JSON yields
    /// [`AppState::default`] rather than an error -- the user can't
    /// recover from a corrupt state file, so silently starting fresh
    /// is the least surprising behaviour.
    pub fn load(path: &Path) -> Self {
        let Ok(text) = fs::read_to_string(path) else {
            return Self::default();
        };
        serde_json::from_str::<Self>(&text).unwrap_or_default()
    }

    /// Write state to `path` atomically (temp file + rename). Creates
    /// parent directories if missing. Bumps `version` to
    /// [`Self::VERSION`] on every save so older builds reading a
    /// newer file can tell.
    pub fn save(&self, path: &Path) -> io::Result<()> {
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
        }
        let mut snapshot = self.clone();
        snapshot.version = Self::VERSION;
        let json = serde_json::to_string_pretty(&snapshot)
            .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))?;
        // Atomic-ish: write to a sibling temp file, then rename.
        // Same-filesystem so rename is atomic on POSIX. macOS-only
        // build means we don't worry about cross-volume edge cases.
        let tmp = path.with_extension("json.tmp");
        fs::write(&tmp, json)?;
        fs::rename(&tmp, path)?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn load_missing_returns_default() {
        let s = AppState::load(Path::new("/this/does/not/exist/atomio/state.json"));
        assert_eq!(s.version, 1);
        assert!(s.recents.is_empty());
        assert!(s.last_project.is_none());
    }

    #[test]
    fn save_then_load_round_trip() {
        let tmp = TempDir::new().unwrap();
        let path = tmp.path().join("state.json");
        let mut s = AppState::default();
        s.recents.push("/p1", "p1");
        s.recents.push("/p2", "p2");
        s.last_project = Some(PathBuf::from("/p2"));
        s.save(&path).unwrap();

        let back = AppState::load(&path);
        assert_eq!(back.version, AppState::VERSION);
        assert_eq!(back.recents.len(), 2);
        assert_eq!(back.last_project.as_deref(), Some(Path::new("/p2")));
    }

    #[test]
    fn save_creates_parent_dirs() {
        let tmp = TempDir::new().unwrap();
        let path = tmp.path().join("nested/dir/state.json");
        let s = AppState::default();
        s.save(&path).unwrap();
        assert!(path.exists());
    }

    #[test]
    fn load_malformed_json_returns_default() {
        let tmp = TempDir::new().unwrap();
        let path = tmp.path().join("state.json");
        fs::write(&path, "{not valid").unwrap();
        let s = AppState::load(&path);
        assert!(s.recents.is_empty());
    }

    #[test]
    fn missing_fields_use_defaults() {
        // Older state file that only carries recents.
        let tmp = TempDir::new().unwrap();
        let path = tmp.path().join("state.json");
        fs::write(&path, r#"{"recents":{"entries":[]}}"#).unwrap();
        let s = AppState::load(&path);
        assert_eq!(s.version, 1);
        assert!(s.last_project.is_none());
    }
}
