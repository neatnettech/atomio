//! Recursive file listing for the file tree pane.
//!
//! Built on top of the `ignore` crate so `.gitignore` + `.ignore` +
//! global hidden-file rules are respected without us re-implementing
//! them. Each [`FileEntry`] holds the path relative to the project
//! root + a [`FileKind`] for the tree render.
//!
//! Scans are bounded by a `max_entries` cap so a pathological repo
//! (`node_modules` un-ignored, multi-GB monorepo) can't lock the UI
//! thread. The cap defaults to 50_000 -- enough for most RN apps while
//! staying snappy.

use std::path::{Path, PathBuf};

use ignore::WalkBuilder;

/// What sort of entry the tree row should render.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FileKind {
    /// Directory. Render with disclosure arrow.
    Directory,
    /// Regular file. Render with extension-derived icon.
    File,
    /// Symlink. Followed for stat, displayed with link glyph.
    Symlink,
}

/// One entry in the recursive scan. Path is **relative to the project
/// root** so the UI can render it without exposing the host filesystem.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FileEntry {
    /// Path relative to project root. Never starts with `/` or `./`.
    pub path: PathBuf,
    /// Directory / file / symlink.
    pub kind: FileKind,
    /// Depth from root. Root itself is depth 0; immediate children 1.
    /// Cheap to render-time indentation without re-walking ancestors.
    pub depth: usize,
}

/// Default cap for `scan`. Plays nice with most RN repos without
/// risking a runaway walk on misconfigured ignore rules.
pub const DEFAULT_MAX_ENTRIES: usize = 50_000;

/// Recursively list files under `root`, honouring `.gitignore` +
/// `.ignore` rules and skipping hidden files by default.
///
/// Entries are sorted by parent-first depth-first order so the tree
/// renderer can stream them straight into rows.
///
/// `max_entries` caps the result length; the walk stops early without
/// erroring once the cap is hit. Use [`DEFAULT_MAX_ENTRIES`] for the
/// common case.
pub fn scan(root: &Path, max_entries: usize) -> Vec<FileEntry> {
    let mut out = Vec::new();
    let walker = WalkBuilder::new(root)
        .hidden(true)
        .git_ignore(true)
        .git_global(true)
        .git_exclude(true)
        .parents(true)
        .follow_links(false)
        .build();

    for result in walker {
        if out.len() >= max_entries {
            break;
        }
        let Ok(entry) = result else { continue };
        let path = entry.path();
        // Skip the root itself; the tree pane already shows it as
        // the header.
        if path == root {
            continue;
        }
        let Ok(rel) = path.strip_prefix(root) else {
            continue;
        };
        let file_type = entry.file_type();
        let kind = match file_type {
            Some(ft) if ft.is_dir() => FileKind::Directory,
            Some(ft) if ft.is_symlink() => FileKind::Symlink,
            Some(_) => FileKind::File,
            None => continue,
        };
        let depth = rel.components().count();
        out.push(FileEntry {
            path: rel.to_path_buf(),
            kind,
            depth,
        });
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::TempDir;

    fn touch(dir: &Path, rel: &str) {
        let p = dir.join(rel);
        if let Some(parent) = p.parent() {
            fs::create_dir_all(parent).unwrap();
        }
        fs::File::create(p).unwrap();
    }

    #[test]
    fn scan_lists_files_and_dirs() {
        let tmp = TempDir::new().unwrap();
        touch(tmp.path(), "src/main.rs");
        touch(tmp.path(), "src/lib.rs");
        touch(tmp.path(), "Cargo.toml");
        let entries = scan(tmp.path(), DEFAULT_MAX_ENTRIES);
        let paths: Vec<String> = entries
            .iter()
            .map(|e| e.path.to_string_lossy().to_string())
            .collect();
        assert!(paths.iter().any(|p| p == "src"));
        assert!(paths.iter().any(|p| p == "src/main.rs"));
        assert!(paths.iter().any(|p| p == "Cargo.toml"));
        // Root itself should not appear.
        assert!(!paths.iter().any(|p| p.is_empty()));
    }

    #[test]
    fn scan_respects_gitignore() {
        let tmp = TempDir::new().unwrap();
        // `.gitignore` needs a git repo marker for `git_ignore` to
        // kick in; alternatively `.ignore` works without.
        touch(tmp.path(), ".ignore");
        fs::write(tmp.path().join(".ignore"), "ignored_dir/\n").unwrap();
        touch(tmp.path(), "ignored_dir/secret.txt");
        touch(tmp.path(), "kept.txt");
        let entries = scan(tmp.path(), DEFAULT_MAX_ENTRIES);
        let paths: Vec<String> = entries
            .iter()
            .map(|e| e.path.to_string_lossy().to_string())
            .collect();
        assert!(paths.iter().any(|p| p == "kept.txt"));
        assert!(!paths.iter().any(|p| p.starts_with("ignored_dir")));
    }

    #[test]
    fn scan_caps_at_max_entries() {
        let tmp = TempDir::new().unwrap();
        for i in 0..20 {
            touch(tmp.path(), &format!("f{i}.txt"));
        }
        let entries = scan(tmp.path(), 5);
        assert_eq!(entries.len(), 5);
    }

    #[test]
    fn depth_is_relative_to_root() {
        let tmp = TempDir::new().unwrap();
        touch(tmp.path(), "a.txt");
        touch(tmp.path(), "dir/b.txt");
        touch(tmp.path(), "dir/sub/c.txt");
        let entries = scan(tmp.path(), DEFAULT_MAX_ENTRIES);
        let depth_of = |s: &str| {
            entries
                .iter()
                .find(|e| e.path.to_string_lossy() == s)
                .map(|e| e.depth)
        };
        assert_eq!(depth_of("a.txt"), Some(1));
        assert_eq!(depth_of("dir/b.txt"), Some(2));
        assert_eq!(depth_of("dir/sub/c.txt"), Some(3));
    }
}
