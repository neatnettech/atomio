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

use std::cmp::Ordering;
use std::collections::HashMap;
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
/// Entries are emitted in **DFS pre-order with directory-first,
/// case-insensitive alphabetical sibling sort**: at each level
/// directories come before files, and within each kind names are
/// ordered alphabetically. This matches the conventional file-tree
/// layout users expect from Finder / VS Code.
///
/// `max_entries` caps the result length; the walk stops early without
/// erroring once the cap is hit. Use [`DEFAULT_MAX_ENTRIES`] for the
/// common case.
pub fn scan(root: &Path, max_entries: usize) -> Vec<FileEntry> {
    let walker = WalkBuilder::new(root)
        .hidden(true)
        .git_ignore(true)
        .git_global(true)
        .git_exclude(true)
        .parents(true)
        .follow_links(false)
        .build();

    let mut raw: Vec<FileEntry> = Vec::new();
    for result in walker {
        if raw.len() >= max_entries {
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
        raw.push(FileEntry {
            path: rel.to_path_buf(),
            kind,
            depth,
        });
    }
    sort_dfs_dirs_first(raw)
}

/// Re-emit `entries` in DFS pre-order with dirs-first, alpha sibling
/// sort. Walks the parent->children map built from the input list so
/// the result preserves the same set of entries but in tree-UI order.
fn sort_dfs_dirs_first(entries: Vec<FileEntry>) -> Vec<FileEntry> {
    // Group entries by parent (root = empty PathBuf).
    let mut children: HashMap<PathBuf, Vec<usize>> = HashMap::new();
    for (i, e) in entries.iter().enumerate() {
        let parent = e.path.parent().map(|p| p.to_path_buf()).unwrap_or_default();
        children.entry(parent).or_default().push(i);
    }
    // Sort each parent's children: directories before files/symlinks,
    // then case-insensitive alpha by file_name.
    for kids in children.values_mut() {
        kids.sort_by(|&a, &b| {
            let ea = &entries[a];
            let eb = &entries[b];
            let ord = type_rank(ea.kind).cmp(&type_rank(eb.kind));
            if ord != Ordering::Equal {
                return ord;
            }
            file_name_lower(&ea.path).cmp(&file_name_lower(&eb.path))
        });
    }
    // DFS from the root.
    let mut out = Vec::with_capacity(entries.len());
    let mut stack: Vec<usize> = children
        .get(Path::new(""))
        .cloned()
        .unwrap_or_default()
        .into_iter()
        .rev()
        .collect();
    while let Some(idx) = stack.pop() {
        out.push(entries[idx].clone());
        if let Some(kids) = children.get(&entries[idx].path) {
            for &child in kids.iter().rev() {
                stack.push(child);
            }
        }
    }
    out
}

fn type_rank(kind: FileKind) -> u8 {
    match kind {
        FileKind::Directory => 0,
        FileKind::Symlink => 1,
        FileKind::File => 2,
    }
}

fn file_name_lower(p: &Path) -> String {
    p.file_name()
        .map(|n| n.to_string_lossy().to_ascii_lowercase())
        .unwrap_or_default()
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
    fn scan_orders_dirs_before_files_alphabetically() {
        let tmp = TempDir::new().unwrap();
        // Mixed siblings at the root and inside a subdir. The
        // filesystem creation order is intentionally not alphabetical
        // and interleaves dirs + files so we exercise the sort.
        touch(tmp.path(), "Zfile.txt");
        touch(tmp.path(), "afile.txt");
        touch(tmp.path(), "Mid/inner.txt");
        touch(tmp.path(), "Mid/aaa.txt");
        touch(tmp.path(), "Adir/leaf.txt");
        let entries = scan(tmp.path(), DEFAULT_MAX_ENTRIES);
        let paths: Vec<String> = entries
            .iter()
            .map(|e| e.path.to_string_lossy().to_string())
            .collect();
        // Root level: Adir, Mid (dirs alpha), then afile.txt, Zfile.txt
        // (files alpha case-insensitive). Each dir's children follow
        // immediately in DFS pre-order.
        assert_eq!(
            paths,
            vec![
                "Adir",
                "Adir/leaf.txt",
                "Mid",
                "Mid/aaa.txt",
                "Mid/inner.txt",
                "afile.txt",
                "Zfile.txt",
            ]
        );
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
