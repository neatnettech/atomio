//! atomio — entry point.
//!
//! v0.0 placeholder: prints a banner and exercises `editor_core` so the
//! workspace builds end-to-end. The next commit replaces this with a real
//! gpui window (the v0.0 go/no-go checkpoint — see docs/ROADMAP.md).

use editor_core::{Buffer, Selection};

fn main() {
    println!("atomio v{}", env!("CARGO_PKG_VERSION"));
    println!("hackable to the core. again.");

    let mut buf: Buffer = "hello".parse().unwrap();
    buf.insert(5, ", atomio");
    let _caret = Selection::caret(buf.len_chars());

    println!("buffer: {buf}");
}
