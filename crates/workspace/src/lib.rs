//! Workspace model for atomio.
//!
//! The `workspace` crate owns everything about "what project is open":
//! the root directory, the parsed manifest (Expo / RN / generic
//! `package.json`), the recursive file listing (`.gitignore`-respecting),
//! and the recents list persisted to disk.
//!
//! No GUI dependency. Like `editor_core`, this stays plain-`cargo test`
//! testable so the project-shell pivot lands without dragging gpui into
//! disk + manifest logic.
//!
//! ```no_run
//! use workspace::Workspace;
//! let ws = Workspace::open("/path/to/expo-app").unwrap();
//! println!("{} ({} files)", ws.display_name(), ws.files().len());
//! ```

#![deny(missing_docs)]

mod files;
mod manifest;
mod recents;
mod state;
mod watcher;
mod workspace;

pub use files::{FileEntry, FileKind};
pub use manifest::{detect_kind, Manifest, ProjectKind};
pub use recents::{Recents, RecentsEntry};
pub use state::AppState;
pub use watcher::{Tick, Watcher, DEFAULT_DEBOUNCE};
pub use workspace::Workspace;
