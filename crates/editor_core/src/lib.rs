//! editor_core — the buffer + selection + editor-state model that backs
//! every atomio editor view.
//!
//! The crate is split into small modules to keep responsibilities clear:
//!
//! - [`buffer`] wraps `ropey` with on-disk identity (path, dirty flag).
//! - [`selection`] is the cursor/selection primitive.
//! - [`state`] is the "interactive buffer": a buffer + a caret + an
//!   undo/redo history. All user-facing editing operations live here so
//!   they can be exercised without a GUI.
//!
//! A CRDT layer will slot in beneath [`state::EditorState`] post-v1.0
//! (see docs/ROADMAP.md — collab is out of scope for year one).

pub mod buffer;
pub mod command;
pub mod selection;
pub mod state;

pub use buffer::Buffer;
pub use command::CommandRegistry;
pub use selection::Selection;
pub use state::EditorState;
