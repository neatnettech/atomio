//! Embedded terminal for atomio.
//!
//! Pure model crate: PTY spawn + ANSI parsing + grid + scrollback.
//! No gpui dependency. Same `editor_core`-style split that keeps the
//! interesting logic plain-`cargo test` runnable, with the UI crate
//! holding only a grid renderer + keyboard forwarder.
//!
//! v0.4 ceiling per `docs/ROADMAP.md`: PTY + ANSI grid + scrollback +
//! tabs. No shell integration features, no ligatures, no transparency.

#![deny(missing_docs)]

mod ansi;
mod grid;
mod terminal;

pub use ansi::Parser;
pub use grid::{Cell, Color, Grid, GridSnapshot};
pub use terminal::Terminal;
