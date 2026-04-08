//! atomio — entry point.
//!
//! v0.0 spike: opens a single gpui window backed by an
//! [`editor_core::EditorState`]. Supports text entry, cursor movement,
//! backspace, undo/redo, file open, and save.
//!
//! All editing rules live in `editor_core::state` — this file is only
//! responsible for translating gpui input events into state ops and for
//! rendering the buffer. That split keeps the interesting logic
//! unit-testable and the UI layer trivially replaceable.

use std::path::PathBuf;

use editor_core::{Buffer, EditorState};
use gpui::{
    actions, div, prelude::*, px, rgb, size, Application, Bounds, Context, FocusHandle, Focusable,
    KeyBinding, KeyDownEvent, Render, SharedString, Window, WindowBounds, WindowOptions,
};

actions!(
    atomio,
    [
        OpenFile,
        SaveFile,
        MoveLeft,
        MoveRight,
        MoveUp,
        MoveDown,
        Backspace,
        DeleteForward,
        Undo,
        Redo,
    ]
);

struct AtomioWindow {
    state: EditorState,
    status: SharedString,
    focus_handle: FocusHandle,
}

impl AtomioWindow {
    fn title(&self) -> SharedString {
        format!("atomio v{}", env!("CARGO_PKG_VERSION")).into()
    }

    fn subtitle(&self) -> SharedString {
        match self.state.buffer.path() {
            Some(p) => {
                let suffix = if self.state.buffer.is_dirty() {
                    " •"
                } else {
                    ""
                };
                format!("{}{suffix}", p.display()).into()
            }
            None => "hackable to the core. again.".into(),
        }
    }

    fn cursor_label(&self) -> SharedString {
        let (line, col) = self.state.cursor_line_col();
        // 1-based for humans.
        format!("ln {} · col {}", line + 1, col + 1).into()
    }

    /// Build a per-line view of the buffer for rendering. Each entry is a
    /// `(line_number_1_based, text, caret_col)` triple; `caret_col` is
    /// `Some` on the single line where the caret currently is. Split as a
    /// pure helper so the rendering logic stays trivially inspectable and
    /// we can add tests if the shape ever gets hairy.
    fn buffer_line_views(&self) -> Vec<(usize, String, Option<usize>)> {
        let buffer = &self.state.buffer;
        let (cursor_line, cursor_col) = self.state.cursor_line_col();
        let line_count = buffer.len_lines().max(1);
        (0..line_count)
            .map(|i| {
                let text = buffer.line_text(i);
                let caret = (i == cursor_line).then_some(cursor_col);
                (i + 1, text, caret)
            })
            .collect()
    }

    // --- action handlers --------------------------------------------------

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
                        this.state.replace_buffer(buf);
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
        if self.state.buffer.path().is_some() {
            self.status = match self.state.buffer.save() {
                Ok(()) => "saved".into(),
                Err(e) => format!("save failed: {e}").into(),
            };
            cx.notify();
            return;
        }

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
                this.status = match this.state.buffer.save_as(&path) {
                    Ok(()) => format!("saved to {}", path.display()).into(),
                    Err(e) => format!("save failed: {e}").into(),
                };
                cx.notify();
            });
        })
        .detach();
    }

    fn on_move_left(&mut self, _: &MoveLeft, _: &mut Window, cx: &mut Context<Self>) {
        self.state.move_left();
        cx.notify();
    }
    fn on_move_right(&mut self, _: &MoveRight, _: &mut Window, cx: &mut Context<Self>) {
        self.state.move_right();
        cx.notify();
    }
    fn on_move_up(&mut self, _: &MoveUp, _: &mut Window, cx: &mut Context<Self>) {
        self.state.move_up();
        cx.notify();
    }
    fn on_move_down(&mut self, _: &MoveDown, _: &mut Window, cx: &mut Context<Self>) {
        self.state.move_down();
        cx.notify();
    }
    fn on_backspace(&mut self, _: &Backspace, _: &mut Window, cx: &mut Context<Self>) {
        self.state.backspace();
        cx.notify();
    }
    fn on_delete_forward(&mut self, _: &DeleteForward, _: &mut Window, cx: &mut Context<Self>) {
        self.state.delete_forward();
        cx.notify();
    }
    fn on_undo(&mut self, _: &Undo, _: &mut Window, cx: &mut Context<Self>) {
        self.state.undo();
        cx.notify();
    }
    fn on_redo(&mut self, _: &Redo, _: &mut Window, cx: &mut Context<Self>) {
        self.state.redo();
        cx.notify();
    }

    /// Catch-all: any key_down the action system did not consume. We use
    /// this exclusively for printable characters — the non-printable keys
    /// (arrows, backspace, cmd+*) are bound as actions and short-circuit
    /// before we get here.
    fn on_key_down(&mut self, event: &KeyDownEvent, _: &mut Window, cx: &mut Context<Self>) {
        let keystroke = &event.keystroke;
        // Ignore anything with a modifier other than shift — those are either
        // already-bound actions (cmd+*) or shortcuts we don't own yet.
        let m = &keystroke.modifiers;
        if m.control || m.alt || m.platform || m.function {
            return;
        }
        let Some(text) = keystroke.key_char.as_deref() else {
            return;
        };
        // Tab / enter arrive as key_char; printable ASCII too. Filter out
        // pure-control bytes so a stray escape doesn't sneak in.
        if text.is_empty() {
            return;
        }
        self.state.insert_str(text);
        cx.notify();
    }
}

impl Focusable for AtomioWindow {
    fn focus_handle(&self, _cx: &gpui::App) -> FocusHandle {
        self.focus_handle.clone()
    }
}

impl Render for AtomioWindow {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let line_views = self.buffer_line_views();
        let gutter_width = {
            // One extra char of padding so a 3-digit line number doesn't
            // touch the separator.
            let digits = line_views.len().max(1).to_string().len();
            px(((digits + 1) * 10) as f32)
        };
        let subtitle = self.subtitle();
        let cursor = self.cursor_label();
        let status = self.status.clone();
        let title = self.title();

        div()
            .key_context("atomio")
            .track_focus(&self.focus_handle)
            .on_action(cx.listener(Self::on_open))
            .on_action(cx.listener(Self::on_save))
            .on_action(cx.listener(Self::on_move_left))
            .on_action(cx.listener(Self::on_move_right))
            .on_action(cx.listener(Self::on_move_up))
            .on_action(cx.listener(Self::on_move_down))
            .on_action(cx.listener(Self::on_backspace))
            .on_action(cx.listener(Self::on_delete_forward))
            .on_action(cx.listener(Self::on_undo))
            .on_action(cx.listener(Self::on_redo))
            .on_key_down(cx.listener(Self::on_key_down))
            .flex()
            .flex_col()
            .size_full()
            .bg(rgb(0x1e1e2e))
            .text_color(rgb(0xcdd6f4))
            // Header
            .child(
                div()
                    .flex()
                    .flex_col()
                    .px_4()
                    .py_2()
                    .bg(rgb(0x181825))
                    .child(div().text_sm().text_color(rgb(0xf5e0dc)).child(title))
                    .child(div().text_xs().text_color(rgb(0x9399b2)).child(subtitle)),
            )
            // Buffer pane: gutter + text
            .child(
                div()
                    .flex_1()
                    .py_4()
                    .flex()
                    .flex_col()
                    .text_color(rgb(0xa6e3a1))
                    .children(line_views.into_iter().map(move |(num, text, caret)| {
                        // Each row: [ gutter ][ line text, maybe with caret ]
                        let gutter = div()
                            .w(gutter_width)
                            .pr_2()
                            .flex()
                            .justify_end()
                            .text_color(rgb(0x6c7086))
                            .child(format!("{num}"));

                        let line_body: gpui::Div = match caret {
                            Some(col) => {
                                // Split the line at the caret column and
                                // insert a thin coloured div between the
                                // halves.
                                let (before, after) = split_at_char(&text, col);
                                div()
                                    .flex()
                                    .flex_row()
                                    .child(div().child(before))
                                    .child(div().w(px(2.0)).h(px(18.0)).bg(rgb(0xf5e0dc)))
                                    .child(div().child(after))
                            }
                            None => {
                                // Render a non-breaking space for empty
                                // lines so the row still has height.
                                let display = if text.is_empty() { " ".into() } else { text };
                                div().child(display)
                            }
                        };

                        div().flex().flex_row().child(gutter).child(line_body)
                    })),
            )
            // Footer / status line
            .child(
                div()
                    .flex()
                    .justify_between()
                    .px_4()
                    .py_1()
                    .bg(rgb(0x181825))
                    .text_xs()
                    .text_color(rgb(0x6c7086))
                    .child(div().child(status))
                    .child(div().child(cursor)),
            )
    }
}

/// Split a string at a character (not byte) index and return the two halves
/// as owned `String`s. Column math in `editor_core` is char-based, so the
/// UI has to be too.
fn split_at_char(s: &str, char_idx: usize) -> (String, String) {
    let split_byte = s
        .char_indices()
        .nth(char_idx)
        .map(|(b, _)| b)
        .unwrap_or(s.len());
    (s[..split_byte].to_string(), s[split_byte..].to_string())
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
            KeyBinding::new("left", MoveLeft, Some("atomio")),
            KeyBinding::new("right", MoveRight, Some("atomio")),
            KeyBinding::new("up", MoveUp, Some("atomio")),
            KeyBinding::new("down", MoveDown, Some("atomio")),
            KeyBinding::new("backspace", Backspace, Some("atomio")),
            KeyBinding::new("delete", DeleteForward, Some("atomio")),
            KeyBinding::new("cmd-z", Undo, Some("atomio")),
            KeyBinding::new("cmd-shift-z", Redo, Some("atomio")),
        ]);

        let bounds = Bounds::centered(None, size(px(720.0), px(480.0)), cx);
        let window = cx
            .open_window(
                WindowOptions {
                    window_bounds: Some(WindowBounds::Windowed(bounds)),
                    ..Default::default()
                },
                |_window, cx| {
                    let state = EditorState::new(buffer.clone());
                    cx.new(|cx| AtomioWindow {
                        state,
                        status: "type to edit · cmd+o open · cmd+s save · cmd+z undo".into(),
                        focus_handle: cx.focus_handle(),
                    })
                },
            )
            .expect("failed to open atomio window");

        window
            .update(cx, |view, window, _cx| {
                window.focus(&view.focus_handle);
            })
            .ok();

        cx.activate(true);
    });
}

#[cfg(test)]
mod tests {
    use super::split_at_char;

    #[test]
    fn split_at_char_ascii() {
        let (a, b) = split_at_char("hello", 2);
        assert_eq!(a, "he");
        assert_eq!(b, "llo");
    }

    #[test]
    fn split_at_char_past_end_is_clamped() {
        let (a, b) = split_at_char("hi", 99);
        assert_eq!(a, "hi");
        assert_eq!(b, "");
    }

    #[test]
    fn split_at_char_multibyte() {
        // "é" is two bytes, one char — splitting at char index 1 must
        // return bytes for one codepoint, not one byte.
        let (a, b) = split_at_char("é!", 1);
        assert_eq!(a, "é");
        assert_eq!(b, "!");
    }
}
