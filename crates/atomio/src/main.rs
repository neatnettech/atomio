//! atomio — entry point.
//!
//! v0.0 spike: opens a single gpui window backed by an `editor_core::Buffer`.
//! The initial buffer comes from a CLI path if given, otherwise a greeting.
//! Cmd+O opens a native file dialog; Cmd+S saves (falling back to save-as
//! when the buffer has no path yet).
//!
//! Actual text editing / cursor movement lands with the next milestone.

use std::path::PathBuf;

use editor_core::Buffer;
use gpui::{
    actions, div, prelude::*, px, rgb, size, Application, Bounds, Context, FocusHandle, Focusable,
    KeyBinding, Render, SharedString, Window, WindowBounds, WindowOptions,
};

actions!(atomio, [OpenFile, SaveFile]);

struct AtomioWindow {
    buffer: Buffer,
    status: SharedString,
    focus_handle: FocusHandle,
}

impl AtomioWindow {
    fn title(&self) -> SharedString {
        format!("atomio v{}", env!("CARGO_PKG_VERSION")).into()
    }

    fn subtitle(&self) -> SharedString {
        match self.buffer.path() {
            Some(p) => {
                let suffix = if self.buffer.is_dirty() { " •" } else { "" };
                format!("{}{suffix}", p.display()).into()
            }
            None => "hackable to the core. again.".into(),
        }
    }

    fn preview(&self) -> SharedString {
        const MAX: usize = 240;
        let full = self.buffer.to_string();
        if full.chars().count() <= MAX {
            full.into()
        } else {
            let truncated: String = full.chars().take(MAX).collect();
            format!("{truncated}...").into()
        }
    }

    fn on_open(&mut self, _: &OpenFile, _window: &mut Window, cx: &mut Context<Self>) {
        self.status = "opening…".into();
        cx.spawn(async move |this, cx| {
            let picked = rfd::AsyncFileDialog::new()
                .set_title("Open file")
                .pick_file()
                .await;
            let Some(file) = picked else {
                let _ = this.update(cx, |this, cx| {
                    this.status = "open cancelled".into();
                    cx.notify();
                });
                return;
            };
            let path = file.path().to_path_buf();
            let result = Buffer::open(&path);
            let _ = this.update(cx, |this, cx| {
                match result {
                    Ok(buf) => {
                        this.buffer = buf;
                        this.status = format!("opened {}", path.display()).into();
                    }
                    Err(e) => {
                        this.status = format!("open failed: {e}").into();
                    }
                }
                cx.notify();
            });
        })
        .detach();
    }

    fn on_save(&mut self, _: &SaveFile, _window: &mut Window, cx: &mut Context<Self>) {
        // If we already have a path, write synchronously — the call is
        // just an fs::write and cannot pump AppKit.
        if self.buffer.path().is_some() {
            match self.buffer.save() {
                Ok(()) => {
                    self.status = "saved".into();
                    cx.notify();
                }
                Err(e) => {
                    self.status = format!("save failed: {e}").into();
                    cx.notify();
                }
            }
            return;
        }

        // Otherwise route through the native save dialog, async.
        self.status = "saving…".into();
        cx.spawn(async move |this, cx| {
            let picked = rfd::AsyncFileDialog::new()
                .set_title("Save as")
                .save_file()
                .await;
            let Some(file) = picked else {
                let _ = this.update(cx, |this, cx| {
                    this.status = "save cancelled".into();
                    cx.notify();
                });
                return;
            };
            let path = file.path().to_path_buf();
            let _ = this.update(cx, |this, cx| {
                match this.buffer.save_as(&path) {
                    Ok(()) => {
                        this.status = format!("saved to {}", path.display()).into();
                    }
                    Err(e) => {
                        this.status = format!("save failed: {e}").into();
                    }
                }
                cx.notify();
            });
        })
        .detach();
    }
}

impl Focusable for AtomioWindow {
    fn focus_handle(&self, _cx: &gpui::App) -> FocusHandle {
        self.focus_handle.clone()
    }
}

impl Render for AtomioWindow {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        div()
            .key_context("atomio")
            .track_focus(&self.focus_handle)
            .on_action(cx.listener(Self::on_open))
            .on_action(cx.listener(Self::on_save))
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
                    .child(self.title()),
            )
            .child(div().text_sm().child(self.subtitle()))
            .child(
                div()
                    .mt_4()
                    .px_4()
                    .py_2()
                    .bg(rgb(0x313244))
                    .text_color(rgb(0xa6e3a1))
                    .child(self.preview()),
            )
            .child(
                div()
                    .text_xs()
                    .text_color(rgb(0x6c7086))
                    .child(self.status.clone()),
            )
    }
}

fn load_initial_buffer() -> Buffer {
    if let Some(path) = std::env::args().nth(1).map(PathBuf::from) {
        match Buffer::open(&path) {
            Ok(buf) => return buf,
            Err(e) => eprintln!("atomio: failed to open {}: {e}", path.display()),
        }
    }
    let mut buf: Buffer = "hello".parse().expect("FromStr<Buffer> is infallible");
    buf.insert(5, ", atomio");
    buf
}

fn main() {
    let buffer = load_initial_buffer();

    Application::new().run(move |cx| {
        cx.bind_keys([
            KeyBinding::new("cmd-o", OpenFile, Some("atomio")),
            KeyBinding::new("cmd-s", SaveFile, Some("atomio")),
        ]);

        let bounds = Bounds::centered(None, size(px(720.0), px(480.0)), cx);
        let window = cx
            .open_window(
                WindowOptions {
                    window_bounds: Some(WindowBounds::Windowed(bounds)),
                    ..Default::default()
                },
                |_window, cx| {
                    cx.new(|cx| AtomioWindow {
                        buffer: buffer.clone(),
                        status: "cmd+o to open · cmd+s to save".into(),
                        focus_handle: cx.focus_handle(),
                    })
                },
            )
            .expect("failed to open atomio window");

        // Give the view keyboard focus so key bindings actually fire.
        window
            .update(cx, |view, window, _cx| {
                window.focus(&view.focus_handle);
            })
            .ok();

        cx.activate(true);
    });
}
