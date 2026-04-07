//! atomio — entry point.
//!
//! v0.0 spike: opens a single gpui window with the project banner so we can
//! validate the renderer end-to-end on Apple Silicon. The "real" editor view
//! that backs this window will land on top of `editor_core` in subsequent
//! commits (see docs/ROADMAP.md, v0.0 → v0.1).

use editor_core::Buffer;
use gpui::{
    div, prelude::*, px, rgb, size, Application, Bounds, Context, Render, SharedString, Window,
    WindowBounds, WindowOptions,
};

struct AtomioWindow {
    title: SharedString,
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
            .child(div().text_sm().child("hackable to the core. again."))
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

fn main() {
    // Exercise editor_core so the window content reflects a real Buffer
    // round-trip — the smallest possible "editor + view" handshake.
    let mut buf: Buffer = "hello".parse().expect("FromStr<Buffer> is infallible");
    buf.insert(5, ", atomio");
    let preview = buf.to_string();

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
                    buffer_preview: preview.clone().into(),
                })
            },
        )
        .expect("failed to open atomio window");
        cx.activate(true);
    });
}
