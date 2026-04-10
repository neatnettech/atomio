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

use editor_core::{Buffer, CommandRegistry, EditorState};
use gpui::{
    actions, div, prelude::*, px, rgb, size, Application, Bounds, ClipboardItem, Context,
    FocusHandle, Focusable, KeyBinding, KeyDownEvent, Render, SharedString, Window, WindowBounds,
    WindowOptions,
};
use language::{highlight_rust, HighlightKind, Span};

actions!(
    atomio,
    [
        OpenFile,
        SaveFile,
        MoveLeft,
        MoveRight,
        MoveUp,
        MoveDown,
        MoveLineStart,
        MoveLineEnd,
        MoveLeftExtending,
        MoveRightExtending,
        MoveUpExtending,
        MoveDownExtending,
        MoveLineStartExtending,
        MoveLineEndExtending,
        SelectAll,
        Copy,
        Cut,
        Paste,
        Backspace,
        DeleteForward,
        Undo,
        Redo,
        TogglePalette,
        ConfirmPalette,
        DismissPalette,
    ]
);

struct LineView {
    number: usize,
    text: String,
    caret: Option<usize>,
    /// `(start_col, end_col)` character range selected on this line, if any.
    selection: Option<(usize, usize)>,
    /// Syntax highlight runs on this line, in char-column space. Sorted and
    /// non-overlapping. Empty when the file has no recognised language.
    highlights: Vec<(usize, usize, HighlightKind)>,
}

/// Map a [`HighlightKind`] to an RGB foreground colour. This is where the
/// "theme" lives for now — a single built-in palette loosely modelled on
/// Catppuccin Mocha, matching the rest of the window chrome.
fn highlight_color(kind: HighlightKind) -> u32 {
    match kind {
        HighlightKind::Keyword => 0xcba6f7,   // mauve
        HighlightKind::String => 0xa6e3a1,    // green
        HighlightKind::Number => 0xfab387,    // peach
        HighlightKind::Comment => 0x6c7086,   // overlay0
        HighlightKind::Type => 0xf9e2af,      // yellow
        HighlightKind::Function => 0x89b4fa,  // blue
        HighlightKind::Attribute => 0xf38ba8, // red
    }
}

const DEFAULT_FG: u32 = 0xcdd6f4;

/// Split global byte-indexed [`Span`]s into per-line char-indexed runs.
///
/// `src` is the full buffer text; `spans` are byte offsets into that text.
/// The returned vector has one entry per line in `src`, each containing the
/// `(char_start, char_end, kind)` runs that fall on that line (multi-line
/// spans are clipped to each line they touch).
fn spans_to_line_runs(src: &str, spans: &[Span]) -> Vec<Vec<(usize, usize, HighlightKind)>> {
    // Byte offsets of each line start. Always at least one entry (line 0).
    let mut line_starts: Vec<usize> = vec![0];
    for (i, b) in src.bytes().enumerate() {
        if b == b'\n' {
            line_starts.push(i + 1);
        }
    }
    let line_count = line_starts.len();
    let mut out: Vec<Vec<(usize, usize, HighlightKind)>> = vec![Vec::new(); line_count];

    for span in spans {
        // Find the starting line via binary search.
        let start_line = match line_starts.binary_search(&span.start) {
            Ok(i) => i,
            Err(i) => i.saturating_sub(1),
        };
        let mut line = start_line;
        while line < line_count {
            let line_start = line_starts[line];
            // Line end excludes the trailing newline byte so we don't try
            // to highlight it.
            let line_end_byte = if line + 1 < line_count {
                line_starts[line + 1] - 1
            } else {
                src.len()
            };
            if span.start >= line_end_byte && line + 1 < line_count {
                line += 1;
                continue;
            }
            let lo = span.start.max(line_start);
            let hi = span.end.min(line_end_byte);
            if lo < hi {
                let line_text = &src[line_start..line_end_byte];
                let char_start = line_text[..lo - line_start].chars().count();
                let char_end = line_text[..hi - line_start].chars().count();
                out[line].push((char_start, char_end, span.kind));
            }
            if span.end <= line_end_byte {
                break;
            }
            line += 1;
        }
    }
    out
}

struct AtomioWindow {
    state: EditorState,
    commands: CommandRegistry,
    status: SharedString,
    focus_handle: FocusHandle,
    /// When `Some`, the command palette overlay is visible and this holds
    /// the current query string.
    palette_query: Option<String>,
    /// Index of the currently highlighted item in the filtered results.
    palette_selected: usize,
}

fn build_command_registry() -> CommandRegistry {
    let mut reg = CommandRegistry::new();
    reg.register("File: Open", "open_file");
    reg.register("File: Save", "save_file");
    reg.register("Edit: Undo", "undo");
    reg.register("Edit: Redo", "redo");
    reg.register("Edit: Copy", "copy");
    reg.register("Edit: Cut", "cut");
    reg.register("Edit: Paste", "paste");
    reg.register("Edit: Select All", "select_all");
    reg.register("View: Toggle Command Palette", "toggle_palette");
    reg
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
    /// `LineView` holding the 1-based line number, the raw text, an optional
    /// caret column, and an optional selection column range on this line.
    fn buffer_line_views(&self) -> Vec<LineView> {
        let buffer = &self.state.buffer;
        let (cursor_line, cursor_col) = self.state.cursor_line_col();
        let line_count = buffer.len_lines().max(1);
        let sel = &self.state.selection;
        let sel_range = sel.range();
        let sel_active = !sel.is_caret();

        // Run the syntax highlighter once per render when the buffer looks
        // like Rust. We feed it the full text because tree-sitter wants a
        // contiguous slice and our buffers are small in v0.0. Incremental
        // reparse is a v0.2 concern.
        let is_rust = buffer
            .path()
            .and_then(|p| p.extension().and_then(|e| e.to_str()))
            .map(|ext| ext.eq_ignore_ascii_case("rs"))
            .unwrap_or(false);
        let line_runs: Vec<Vec<(usize, usize, HighlightKind)>> = if is_rust {
            let src = buffer.slice_to_string(0..buffer.len_chars());
            let spans = highlight_rust(&src);
            spans_to_line_runs(&src, &spans)
        } else {
            Vec::new()
        };

        (0..line_count)
            .map(|i| {
                let text = buffer.line_text(i);
                let caret = (i == cursor_line).then_some(cursor_col);
                let selection = if sel_active {
                    let line_start = buffer.line_to_char(i);
                    let line_end = line_start + buffer.line_len(i);
                    let lo = sel_range.start.max(line_start);
                    let hi = sel_range.end.min(line_end);
                    if lo < hi {
                        Some((lo - line_start, hi - line_start))
                    } else {
                        None
                    }
                } else {
                    None
                };
                let highlights = line_runs.get(i).cloned().unwrap_or_default();
                LineView {
                    number: i + 1,
                    text,
                    caret,
                    selection,
                    highlights,
                }
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

    /// True when the command palette overlay owns keyboard focus.
    fn palette_is_open(&self) -> bool {
        self.palette_query.is_some()
    }

    fn palette_move_up(&mut self, cx: &mut Context<Self>) {
        self.palette_selected = self.palette_selected.saturating_sub(1);
        cx.notify();
    }

    fn palette_move_down(&mut self, cx: &mut Context<Self>) {
        if let Some(query) = &self.palette_query {
            let count = self.commands.search(query).len();
            if self.palette_selected + 1 < count {
                self.palette_selected += 1;
            }
        }
        cx.notify();
    }

    fn palette_backspace(&mut self, cx: &mut Context<Self>) {
        if let Some(query) = &mut self.palette_query {
            query.pop();
            self.palette_selected = 0;
        }
        cx.notify();
    }

    fn on_move_left(&mut self, _: &MoveLeft, _: &mut Window, cx: &mut Context<Self>) {
        if self.palette_is_open() {
            return;
        }
        self.state.move_left();
        cx.notify();
    }
    fn on_move_right(&mut self, _: &MoveRight, _: &mut Window, cx: &mut Context<Self>) {
        if self.palette_is_open() {
            return;
        }
        self.state.move_right();
        cx.notify();
    }
    fn on_move_up(&mut self, _: &MoveUp, _: &mut Window, cx: &mut Context<Self>) {
        if self.palette_is_open() {
            self.palette_move_up(cx);
            return;
        }
        self.state.move_up();
        cx.notify();
    }
    fn on_move_down(&mut self, _: &MoveDown, _: &mut Window, cx: &mut Context<Self>) {
        if self.palette_is_open() {
            self.palette_move_down(cx);
            return;
        }
        self.state.move_down();
        cx.notify();
    }
    fn on_move_line_start(&mut self, _: &MoveLineStart, _: &mut Window, cx: &mut Context<Self>) {
        if self.palette_is_open() {
            return;
        }
        self.state.move_line_start();
        cx.notify();
    }
    fn on_move_line_end(&mut self, _: &MoveLineEnd, _: &mut Window, cx: &mut Context<Self>) {
        if self.palette_is_open() {
            return;
        }
        self.state.move_line_end();
        cx.notify();
    }
    fn on_move_left_ext(&mut self, _: &MoveLeftExtending, _: &mut Window, cx: &mut Context<Self>) {
        if self.palette_is_open() {
            return;
        }
        self.state.move_left_extending();
        cx.notify();
    }
    fn on_move_right_ext(
        &mut self,
        _: &MoveRightExtending,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.palette_is_open() {
            return;
        }
        self.state.move_right_extending();
        cx.notify();
    }
    fn on_move_up_ext(&mut self, _: &MoveUpExtending, _: &mut Window, cx: &mut Context<Self>) {
        if self.palette_is_open() {
            return;
        }
        self.state.move_up_extending();
        cx.notify();
    }
    fn on_move_down_ext(&mut self, _: &MoveDownExtending, _: &mut Window, cx: &mut Context<Self>) {
        if self.palette_is_open() {
            return;
        }
        self.state.move_down_extending();
        cx.notify();
    }
    fn on_move_line_start_ext(
        &mut self,
        _: &MoveLineStartExtending,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.palette_is_open() {
            return;
        }
        self.state.move_line_start_extending();
        cx.notify();
    }
    fn on_move_line_end_ext(
        &mut self,
        _: &MoveLineEndExtending,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.palette_is_open() {
            return;
        }
        self.state.move_line_end_extending();
        cx.notify();
    }
    fn on_select_all(&mut self, _: &SelectAll, _: &mut Window, cx: &mut Context<Self>) {
        if self.palette_is_open() {
            return;
        }
        self.state.select_all();
        cx.notify();
    }
    fn on_copy(&mut self, _: &Copy, _: &mut Window, cx: &mut Context<Self>) {
        if self.palette_is_open() {
            return;
        }
        if let Some(text) = self.state.selected_text() {
            cx.write_to_clipboard(ClipboardItem::new_string(text));
        }
    }
    fn on_cut(&mut self, _: &Cut, _: &mut Window, cx: &mut Context<Self>) {
        if self.palette_is_open() {
            return;
        }
        if let Some(text) = self.state.cut_selection() {
            cx.write_to_clipboard(ClipboardItem::new_string(text));
            cx.notify();
        }
    }
    fn on_paste(&mut self, _: &Paste, _: &mut Window, cx: &mut Context<Self>) {
        if self.palette_is_open() {
            return;
        }
        if let Some(item) = cx.read_from_clipboard() {
            if let Some(text) = item.text() {
                if !text.is_empty() {
                    self.state.insert_str(&text);
                    cx.notify();
                }
            }
        }
    }
    fn on_backspace(&mut self, _: &Backspace, _: &mut Window, cx: &mut Context<Self>) {
        if self.palette_is_open() {
            self.palette_backspace(cx);
            return;
        }
        self.state.backspace();
        cx.notify();
    }
    fn on_delete_forward(&mut self, _: &DeleteForward, _: &mut Window, cx: &mut Context<Self>) {
        if self.palette_is_open() {
            return;
        }
        self.state.delete_forward();
        cx.notify();
    }
    fn on_undo(&mut self, _: &Undo, _: &mut Window, cx: &mut Context<Self>) {
        if self.palette_is_open() {
            return;
        }
        self.state.undo();
        cx.notify();
    }
    fn on_redo(&mut self, _: &Redo, _: &mut Window, cx: &mut Context<Self>) {
        if self.palette_is_open() {
            return;
        }
        self.state.redo();
        cx.notify();
    }
    fn on_toggle_palette(&mut self, _: &TogglePalette, _: &mut Window, cx: &mut Context<Self>) {
        if self.palette_query.is_some() {
            self.palette_query = None;
        } else {
            self.palette_query = Some(String::new());
            self.palette_selected = 0;
        }
        cx.notify();
    }
    fn on_dismiss_palette(&mut self, _: &DismissPalette, _: &mut Window, cx: &mut Context<Self>) {
        if self.palette_query.is_some() {
            self.palette_query = None;
            cx.notify();
        }
    }
    fn on_confirm_palette(
        &mut self,
        _: &ConfirmPalette,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(query) = self.palette_query.take() else {
            return;
        };
        let matches = self.commands.search(&query);
        if let Some(m) = matches.get(self.palette_selected) {
            self.dispatch_command(&m.command.id, window, cx);
        }
        self.palette_selected = 0;
        cx.notify();
    }

    fn dispatch_command(&mut self, id: &str, window: &mut Window, cx: &mut Context<Self>) {
        match id {
            "open_file" => self.on_open(&OpenFile, window, cx),
            "save_file" => self.on_save(&SaveFile, window, cx),
            "undo" => self.on_undo(&Undo, window, cx),
            "redo" => self.on_redo(&Redo, window, cx),
            "copy" => self.on_copy(&Copy, window, cx),
            "cut" => self.on_cut(&Cut, window, cx),
            "paste" => self.on_paste(&Paste, window, cx),
            "select_all" => self.on_select_all(&SelectAll, window, cx),
            "toggle_palette" => self.on_toggle_palette(&TogglePalette, window, cx),
            _ => {}
        }
    }

    /// Catch-all: any key_down the action system did not consume.
    ///
    /// When the palette is open, printable chars feed the query. When it's
    /// closed, printable chars insert into the buffer. Up/down/backspace
    /// are handled by the action handlers (MoveUp, MoveDown, Backspace)
    /// which check `palette_is_open()` and reroute accordingly.
    fn on_key_down(&mut self, event: &KeyDownEvent, _: &mut Window, cx: &mut Context<Self>) {
        let keystroke = &event.keystroke;
        let m = &keystroke.modifiers;

        // When the palette is open, only accept printable chars for the query.
        if let Some(query) = &mut self.palette_query {
            if m.control || m.alt || m.function || m.platform {
                return;
            }
            if let Some(text) = keystroke.key_char.as_deref() {
                if !text.is_empty() {
                    query.push_str(text);
                    self.palette_selected = 0;
                    cx.notify();
                }
            }
            return;
        }

        // Normal mode: feed printable chars to the buffer.
        if m.control || m.alt || m.platform || m.function {
            return;
        }
        let Some(text) = keystroke.key_char.as_deref() else {
            return;
        };
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
            let digits = line_views.len().max(1).to_string().len();
            px(((digits + 1) * 10) as f32)
        };
        let subtitle = self.subtitle();
        let cursor = self.cursor_label();
        let status = self.status.clone();
        let title = self.title();

        // Build palette overlay if visible. Rendered as a Spotlight-style
        // floating panel: absolutely positioned, horizontally centred,
        // anchored near the top of the window so it doesn't shift the
        // editor content underneath.
        let palette_overlay: Option<gpui::Div> = self.palette_query.as_ref().map(|query| {
            let matches = self.commands.search(query);
            let selected = self.palette_selected;
            let mut inner = div()
                .flex()
                .flex_col()
                .w(px(440.0))
                .bg(rgb(0x313244))
                .rounded(px(8.0))
                .py_1()
                .shadow_lg()
                .border_1()
                .border_color(rgb(0x45475a))
                .child(
                    div()
                        .px_3()
                        .py_2()
                        .text_sm()
                        .text_color(rgb(0xcdd6f4))
                        .child(if query.is_empty() {
                            "> type to search...".to_string()
                        } else {
                            format!("> {query}")
                        }),
                );
            if !matches.is_empty() {
                inner = inner.child(div().h(px(1.0)).mx_2().bg(rgb(0x45475a)));
            }
            for (i, m) in matches.iter().take(10).enumerate() {
                let bg = if i == selected { 0x45475a } else { 0x313244 };
                inner = inner.child(
                    div()
                        .px_3()
                        .py_1()
                        .text_sm()
                        .text_color(rgb(0xcdd6f4))
                        .bg(rgb(bg))
                        .rounded(px(4.0))
                        .mx_1()
                        .child(m.command.label.clone()),
                );
            }
            // Outer container: takes the full window area, absolutely
            // positioned at top-left, centres the inner panel horizontally
            // and pushes it down from the top with padding.
            div()
                .absolute()
                .top_0()
                .left_0()
                .size_full()
                .flex()
                .justify_center()
                .pt(px(80.0))
                .child(inner)
        });

        div()
            .key_context("atomio")
            .track_focus(&self.focus_handle)
            .on_action(cx.listener(Self::on_open))
            .on_action(cx.listener(Self::on_save))
            .on_action(cx.listener(Self::on_move_left))
            .on_action(cx.listener(Self::on_move_right))
            .on_action(cx.listener(Self::on_move_up))
            .on_action(cx.listener(Self::on_move_down))
            .on_action(cx.listener(Self::on_move_line_start))
            .on_action(cx.listener(Self::on_move_line_end))
            .on_action(cx.listener(Self::on_move_left_ext))
            .on_action(cx.listener(Self::on_move_right_ext))
            .on_action(cx.listener(Self::on_move_up_ext))
            .on_action(cx.listener(Self::on_move_down_ext))
            .on_action(cx.listener(Self::on_move_line_start_ext))
            .on_action(cx.listener(Self::on_move_line_end_ext))
            .on_action(cx.listener(Self::on_select_all))
            .on_action(cx.listener(Self::on_copy))
            .on_action(cx.listener(Self::on_cut))
            .on_action(cx.listener(Self::on_paste))
            .on_action(cx.listener(Self::on_backspace))
            .on_action(cx.listener(Self::on_delete_forward))
            .on_action(cx.listener(Self::on_undo))
            .on_action(cx.listener(Self::on_redo))
            .on_action(cx.listener(Self::on_toggle_palette))
            .on_action(cx.listener(Self::on_confirm_palette))
            .on_action(cx.listener(Self::on_dismiss_palette))
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
                    .children(line_views.into_iter().map(move |lv| {
                        let LineView {
                            number,
                            text,
                            caret,
                            selection,
                            highlights,
                        } = lv;
                        let gutter = div()
                            .w(gutter_width)
                            .pr_2()
                            .flex()
                            .justify_end()
                            .text_color(rgb(0x6c7086))
                            .child(format!("{number}"));

                        let runs = build_runs(&text, caret, selection, &highlights);
                        let mut row = div().flex().flex_row();
                        for run in runs {
                            match run {
                                Run::Caret => {
                                    row = row.child(div().w(px(2.0)).h(px(18.0)).bg(rgb(0xf5e0dc)));
                                }
                                Run::Text { text, fg, selected } => {
                                    if text.is_empty() {
                                        continue;
                                    }
                                    let bg = if selected { 0x45475a } else { 0x1e1e2e };
                                    row = row
                                        .child(div().bg(rgb(bg)).text_color(rgb(fg)).child(text));
                                }
                            }
                        }

                        // Empty line still needs height — emit a blank space
                        // so the row doesn't collapse.
                        if text.is_empty() && caret.is_none() {
                            row = row.child(div().child(" "));
                        }

                        div().flex().flex_row().child(gutter).child(row)
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
            // Command palette — absolutely positioned, floats above the
            // editor like macOS Spotlight.
            .children(palette_overlay)
    }
}

/// A rendered sub-segment of a single line. `build_runs` emits these in
/// order; the renderer converts each into a gpui `div`.
#[derive(Debug, PartialEq, Eq)]
enum Run {
    Text {
        text: String,
        fg: u32,
        selected: bool,
    },
    Caret,
}

/// Flatten a line into a list of [`Run`]s by merging the three independent
/// per-character attributes that can apply: foreground colour (from syntax
/// highlights), selection background, and the caret overlay. We do this by
/// computing a style tag per character and then collapsing runs of identical
/// tags, which is straightforward to unit-test without a GPU.
fn build_runs(
    text: &str,
    caret: Option<usize>,
    selection: Option<(usize, usize)>,
    highlights: &[(usize, usize, HighlightKind)],
) -> Vec<Run> {
    let chars: Vec<char> = text.chars().collect();
    let n = chars.len();

    // Per-character foreground colour.
    let mut fg: Vec<u32> = vec![DEFAULT_FG; n];
    for (start, end, kind) in highlights {
        let s = (*start).min(n);
        let e = (*end).min(n);
        let color = highlight_color(*kind);
        for slot in &mut fg[s..e] {
            *slot = color;
        }
    }

    // Per-character selection flag.
    let mut sel: Vec<bool> = vec![false; n];
    if let Some((s, e)) = selection {
        let s = s.min(n);
        let e = e.min(n);
        for slot in &mut sel[s..e] {
            *slot = true;
        }
    }

    let mut runs: Vec<Run> = Vec::new();
    let mut i = 0;
    while i < n {
        // If the caret sits at this column, emit it before the glyph.
        if caret == Some(i) {
            runs.push(Run::Caret);
        }
        let start = i;
        let start_fg = fg[i];
        let start_sel = sel[i];
        i += 1;
        while i < n && fg[i] == start_fg && sel[i] == start_sel && caret != Some(i) {
            i += 1;
        }
        runs.push(Run::Text {
            text: chars[start..i].iter().collect(),
            fg: start_fg,
            selected: start_sel,
        });
    }
    // Caret at end-of-line.
    if caret == Some(n) {
        runs.push(Run::Caret);
    }
    runs
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
            KeyBinding::new("home", MoveLineStart, Some("atomio")),
            KeyBinding::new("end", MoveLineEnd, Some("atomio")),
            KeyBinding::new("cmd-left", MoveLineStart, Some("atomio")),
            KeyBinding::new("cmd-right", MoveLineEnd, Some("atomio")),
            KeyBinding::new("shift-left", MoveLeftExtending, Some("atomio")),
            KeyBinding::new("shift-right", MoveRightExtending, Some("atomio")),
            KeyBinding::new("shift-up", MoveUpExtending, Some("atomio")),
            KeyBinding::new("shift-down", MoveDownExtending, Some("atomio")),
            KeyBinding::new("shift-home", MoveLineStartExtending, Some("atomio")),
            KeyBinding::new("shift-end", MoveLineEndExtending, Some("atomio")),
            KeyBinding::new("cmd-shift-left", MoveLineStartExtending, Some("atomio")),
            KeyBinding::new("cmd-shift-right", MoveLineEndExtending, Some("atomio")),
            KeyBinding::new("cmd-a", SelectAll, Some("atomio")),
            KeyBinding::new("cmd-c", Copy, Some("atomio")),
            KeyBinding::new("cmd-x", Cut, Some("atomio")),
            KeyBinding::new("cmd-v", Paste, Some("atomio")),
            KeyBinding::new("backspace", Backspace, Some("atomio")),
            KeyBinding::new("delete", DeleteForward, Some("atomio")),
            KeyBinding::new("cmd-z", Undo, Some("atomio")),
            KeyBinding::new("cmd-shift-z", Redo, Some("atomio")),
            KeyBinding::new("cmd-shift-p", TogglePalette, Some("atomio")),
            KeyBinding::new("enter", ConfirmPalette, Some("atomio")),
            KeyBinding::new("escape", DismissPalette, Some("atomio")),
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
                    let commands = build_command_registry();
                    cx.new(|cx| AtomioWindow {
                        state,
                        commands,
                        status: "type to edit · cmd+o open · cmd+s save · cmd+shift+p palette"
                            .into(),
                        focus_handle: cx.focus_handle(),
                        palette_query: None,
                        palette_selected: 0,
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
    use super::*;

    fn text_run(text: &str, fg: u32, selected: bool) -> Run {
        Run::Text {
            text: text.to_string(),
            fg,
            selected,
        }
    }

    #[test]
    fn runs_plain_text_single_segment() {
        let runs = build_runs("hello", None, None, &[]);
        assert_eq!(runs, vec![text_run("hello", DEFAULT_FG, false)]);
    }

    #[test]
    fn runs_caret_at_middle_splits() {
        let runs = build_runs("hello", Some(2), None, &[]);
        assert_eq!(
            runs,
            vec![
                text_run("he", DEFAULT_FG, false),
                Run::Caret,
                text_run("llo", DEFAULT_FG, false),
            ]
        );
    }

    #[test]
    fn runs_caret_at_end_of_line() {
        let runs = build_runs("hi", Some(2), None, &[]);
        assert_eq!(runs, vec![text_run("hi", DEFAULT_FG, false), Run::Caret]);
    }

    #[test]
    fn runs_highlight_colors_keyword() {
        let hl = vec![(0, 2, HighlightKind::Keyword)];
        let runs = build_runs("fn main", None, None, &hl);
        let kw = highlight_color(HighlightKind::Keyword);
        assert_eq!(
            runs,
            vec![
                text_run("fn", kw, false),
                text_run(" main", DEFAULT_FG, false),
            ]
        );
    }

    #[test]
    fn runs_selection_and_highlight_combine() {
        // "fn" is a keyword, and chars 1..4 are selected. The overlap
        // means column 1 (n) should be both selected AND keyword-coloured.
        let hl = vec![(0, 2, HighlightKind::Keyword)];
        let runs = build_runs("fn ab", None, Some((1, 4)), &hl);
        let kw = highlight_color(HighlightKind::Keyword);
        assert_eq!(
            runs,
            vec![
                text_run("f", kw, false),
                text_run("n", kw, true),
                text_run(" a", DEFAULT_FG, true),
                text_run("b", DEFAULT_FG, false),
            ]
        );
    }

    #[test]
    fn spans_to_line_runs_clips_multiline_span() {
        // A block comment that straddles two lines. Byte offsets in src:
        //   line 0: "/* hi\n"  (bytes 0..5, newline at 5)
        //   line 1: "more */"  (bytes 6..13)
        let src = "/* hi\nmore */";
        let span = Span {
            start: 0,
            end: 13,
            kind: HighlightKind::Comment,
        };
        let runs = spans_to_line_runs(src, &[span]);
        assert_eq!(runs.len(), 2);
        assert_eq!(runs[0], vec![(0, 5, HighlightKind::Comment)]);
        assert_eq!(runs[1], vec![(0, 7, HighlightKind::Comment)]);
    }
}
