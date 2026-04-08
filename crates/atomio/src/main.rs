//! atomio — entry point.
//!
//! v0.0 spike: opens a single gpui window that displays a file loaded from
//! disk. The file comes from (in priority order):
//!   1. a path passed on the command line (`atomio path/to/file`),
//!   2. a built-in greeting buffer, used when no path is given.
//!
//! The native open/save dialogs land once we have keybindings and can
//! trigger `rfd` from inside the gpui run loop — calling it before gpui
//! initializes NSApplication crashes AppKit on macOS.

use std::path::PathBuf;

use editor_core::Buffer;
use gpui::{
    div, prelude::*, px, rgb, size, Application, Bounds, Context, Render, SharedString, Window,
    WindowBounds, WindowOptions,
};

struct AtomioWindow {
    title: SharedString,
    subtitle: SharedString,
    buffer_preview: SharedString,
}

impl Render for AtomioWindow {
    fn render(&mut self, _window: &mut Window, _cx: &mut Context<Self>) -> impl IntoElement {
        div()
            .flex()
            .flex_col()
            .gap_4()
            .size_full()
            .bg(rgb(0x1e1e2e))
            .justify_center()
            .items_center()
            .text_color(rgb(0xcdd6f4))
            .child(
                div()
                    .text_2xl()
                    .text_color(rgb(0xf5e0dc))
                    .child(self.title.clone()),
            )
            .child(div().text_sm().child(self.subtitle.clone()))
            .child(
                div()
                    .mt_4()
                    .px_4()
                    .py_2()
                    .bg(rgb(0x313244))
                    .text_color(rgb(0xa6e3a1))
                    .child(self.buffer_preview.clone()),
            )
    }
}

/// Resolve the buffer atomio should open on startup.
fn load_initial_buffer() -> Buffer {
    if let Some(path) = std::env::args().nth(1).map(PathBuf::from) {
        match Buffer::open(&path) {
            Ok(buf) => return buf,
            Err(e) => eprintln!("atomio: failed to open {}: {e}", path.display()),
        }
    }

    // Fallback — show the banner buffer so v0.0 is still runnable without
    // an on-disk file, useful for smoke-testing the renderer.
    let mut buf: Buffer = "hello".parse().expect("FromStr<Buffer> is infallible");
    buf.insert(5, ", atomio");
    buf
}

/// Truncate long files to a preview the v0.0 label can render. A real editor
/// view lands in v0.1.
fn preview_of(buf: &Buffer) -> String {
    const MAX: usize = 240;
    let full = buf.to_string();
    if full.chars().count() <= MAX {
        full
    } else {
        let truncated: String = full.chars().take(MAX).collect();
        format!("{truncated}...")
    }
}

fn main() {
    let buf = load_initial_buffer();
    let preview = preview_of(&buf);
    let subtitle = match buf.path() {
        Some(p) => format!("{}", p.display()),
        None => "hackable to the core. again.".into(),
    };

    Application::new().run(move |cx| {
        let bounds = Bounds::centered(None, size(px(720.0), px(480.0)), cx);
        cx.open_window(
            WindowOptions {
                window_bounds: Some(WindowBounds::Windowed(bounds)),
                ..Default::default()
            },
            |_, cx| {
                cx.new(|_| AtomioWindow {
                    title: format!("atomio v{}", env!("CARGO_PKG_VERSION")).into(),
                    subtitle: subtitle.clone().into(),
                    buffer_preview: preview.clone().into(),
                })
            },
        )
        .expect("failed to open atomio window");
        cx.activate(true);
    });
}
