//! The top-level [`Workspace`] handle.
//!
//! Combines [`Manifest`](crate::Manifest) + scanned files into the
//! single value the UI shell holds while a project is open.

use std::io;
use std::path::{Path, PathBuf};

use crate::files::{scan, FileEntry, DEFAULT_MAX_ENTRIES};
use crate::manifest::{detect_kind, Manifest, ProjectKind};

/// Opened project root + cached snapshot of the file tree + manifest.
///
/// Cheap to construct (one stat + one walk). Re-create or call
/// [`Self::refresh`] when the on-disk state changes -- the `notify`
/// watcher hookup lives in the atomio shell, not here.
#[derive(Debug, Clone)]
pub struct Workspace {
    root: PathBuf,
    manifest: Manifest,
    files: Vec<FileEntry>,
}

impl Workspace {
    /// Open `root` as a project. Returns `Err` only when `root` does
    /// not exist or is not a directory; missing / malformed manifests
    /// degrade gracefully (the project opens as
    /// [`ProjectKind::Unknown`] / [`ProjectKind::Generic`]).
    pub fn open(root: impl Into<PathBuf>) -> io::Result<Self> {
        let root = root.into();
        let meta = std::fs::metadata(&root)?;
        if !meta.is_dir() {
            return Err(io::Error::new(
                io::ErrorKind::NotADirectory,
                format!("{} is not a directory", root.display()),
            ));
        }
        let manifest = detect_kind(&root);
        let files = scan(&root, DEFAULT_MAX_ENTRIES);
        Ok(Self {
            root,
            manifest,
            files,
        })
    }

    /// Re-scan the file tree and re-read the manifest in place.
    /// Returns the new file count for the caller to compare against
    /// the previous snapshot.
    pub fn refresh(&mut self) -> usize {
        self.manifest = detect_kind(&self.root);
        self.files = scan(&self.root, DEFAULT_MAX_ENTRIES);
        self.files.len()
    }

    /// Project root on disk.
    pub fn root(&self) -> &Path {
        &self.root
    }

    /// Parsed manifest (kind + name + version).
    pub fn manifest(&self) -> &Manifest {
        &self.manifest
    }

    /// Detected project flavour.
    pub fn kind(&self) -> ProjectKind {
        self.manifest.kind
    }

    /// Scanned file list, relative paths, depth-ordered.
    pub fn files(&self) -> &[FileEntry] {
        &self.files
    }

    /// Display name: manifest `name` if present, else the root
    /// directory's basename, else the full path. Used in the title
    /// bar + recents.
    pub fn display_name(&self) -> String {
        if let Some(name) = self.manifest.name.as_deref() {
            return name.to_string();
        }
        self.root
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_else(|| self.root.to_string_lossy().into_owned())
    }

    /// Resolve a tree-relative path back to an absolute on-disk path.
    /// Returns `None` when `rel` escapes the root via `..` -- callers
    /// should treat that as a programming error.
    pub fn absolute(&self, rel: &Path) -> Option<PathBuf> {
        if rel.components().any(|c| c.as_os_str() == "..") {
            return None;
        }
        Some(self.root.join(rel))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::TempDir;

    #[test]
    fn open_errors_on_missing_root() {
        assert!(Workspace::open("/this/does/not/exist/atomio/test").is_err());
    }

    #[test]
    fn open_errors_on_file_not_dir() {
        let tmp = TempDir::new().unwrap();
        let p = tmp.path().join("a-file.txt");
        fs::write(&p, "").unwrap();
        assert!(Workspace::open(&p).is_err());
    }

    #[test]
    fn open_succeeds_on_empty_dir() {
        let tmp = TempDir::new().unwrap();
        let ws = Workspace::open(tmp.path()).unwrap();
        assert_eq!(ws.kind(), ProjectKind::Unknown);
        assert!(ws.files().is_empty());
    }

    #[test]
    fn display_name_prefers_manifest_name() {
        let tmp = TempDir::new().unwrap();
        fs::write(
            tmp.path().join("package.json"),
            r#"{"name":"the-app","dependencies":{"expo":"^51"}}"#,
        )
        .unwrap();
        let ws = Workspace::open(tmp.path()).unwrap();
        assert_eq!(ws.display_name(), "the-app");
        assert_eq!(ws.kind(), ProjectKind::Expo);
    }

    #[test]
    fn display_name_falls_back_to_basename() {
        let tmp = TempDir::new().unwrap();
        let ws = Workspace::open(tmp.path()).unwrap();
        let basename = tmp.path().file_name().unwrap().to_string_lossy();
        assert_eq!(ws.display_name(), basename);
    }

    #[test]
    fn absolute_rejects_path_traversal() {
        let tmp = TempDir::new().unwrap();
        let ws = Workspace::open(tmp.path()).unwrap();
        assert!(ws.absolute(Path::new("../secret")).is_none());
        assert!(ws.absolute(Path::new("src/main.rs")).is_some());
    }

    #[test]
    fn refresh_picks_up_new_files() {
        let tmp = TempDir::new().unwrap();
        let mut ws = Workspace::open(tmp.path()).unwrap();
        assert_eq!(ws.files().len(), 0);
        fs::write(tmp.path().join("hello.txt"), "hi").unwrap();
        let n = ws.refresh();
        assert_eq!(n, 1);
        assert_eq!(ws.files()[0].path.to_string_lossy(), "hello.txt");
    }
}
