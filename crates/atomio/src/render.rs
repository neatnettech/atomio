//! Rendering layer for [`super::AtomioWindow`].
//!
//! Split out of `main.rs` to keep that file focused on state, action
//! handlers, and entry-point wiring. Everything here is pure
//! pixel/layout logic plus the small helpers that translate model
//! types into colours and run-segments.
//!
//! Lives as a child module of `main.rs` so it has access to
//! `AtomioWindow`'s private fields and methods without having to
//! widen their visibility.

use gpui::{
    div, linear_color_stop, linear_gradient, prelude::*, px, rgb, svg, Context, FocusHandle,
    Focusable, Render, SharedString, Window,
};
use language::{HighlightKind, Language, Span};

use crate::cdp_bridge::{ConnectionState, DebuggerCommand};
use crate::theme;
use crate::{AtomioWindow, DockPane, ProfilerRefresh, SimulatorCapture};

/// Per-line view used by the editor renderer. Built by
/// [`AtomioWindow::buffer_line_views`] and consumed by
/// [`AtomioWindow::render_editor_pane`].
pub(crate) struct LineView {
    pub(crate) number: usize,
    pub(crate) text: String,
    pub(crate) caret: Option<usize>,
    /// `(start_col, end_col)` character range selected on this line, if any.
    pub(crate) selection: Option<(usize, usize)>,
    /// Syntax highlight runs on this line, in char-column space. Sorted and
    /// non-overlapping. Empty when the file has no recognised language.
    pub(crate) highlights: Vec<(usize, usize, HighlightKind)>,
    /// True when a breakpoint is set on this line in the current source.
    pub(crate) has_breakpoint: bool,
    /// True when the runtime is paused on this exact line.
    pub(crate) is_paused_here: bool,
}

/// Map a [`HighlightKind`] to an RGB foreground colour. This is where the
/// "theme" lives for now — a single built-in palette loosely modelled on
/// Catppuccin Mocha, matching the rest of the window chrome.
pub(crate) fn highlight_color(kind: HighlightKind) -> u32 {
    match kind {
        HighlightKind::Keyword => theme::SX_KW,
        HighlightKind::String => theme::SX_STR,
        HighlightKind::Number => theme::SX_NUM,
        HighlightKind::Comment => theme::SX_COM,
        HighlightKind::Type => theme::SX_TYPE,
        HighlightKind::Function => theme::SX_FN,
        HighlightKind::Attribute => theme::SX_PROP,
        HighlightKind::Property => theme::SX_PROP,
        HighlightKind::Constant => theme::SX_NUM,
    }
}

pub(crate) const DEFAULT_FG: u32 = theme::SX_PL;

/// JS keywords/operators we don't want as inline-value targets.
pub(crate) const JS_KEYWORDS: &[&str] = &[
    "var",
    "let",
    "const",
    "function",
    "if",
    "else",
    "for",
    "while",
    "do",
    "switch",
    "case",
    "default",
    "break",
    "continue",
    "return",
    "throw",
    "try",
    "catch",
    "finally",
    "new",
    "delete",
    "typeof",
    "instanceof",
    "in",
    "of",
    "void",
    "yield",
    "async",
    "await",
    "class",
    "extends",
    "super",
    "this",
    "import",
    "export",
    "from",
    "as",
    "static",
    "get",
    "set",
    "null",
    "undefined",
    "true",
    "false",
    "console",
    "log",
    "warn",
    "error",
];

/// Pull plausible identifier candidates from a single line of source.
/// Lightweight scanner: matches `[A-Za-z_$][A-Za-z0-9_$]*`, skips string
/// and line-comment regions, dedupes, drops keywords, caps at 5.
pub(crate) fn extract_idents(line: &str) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    let mut current = String::new();
    let mut in_string: Option<char> = None;
    let chars: Vec<char> = line.chars().collect();
    let mut i = 0;
    while i < chars.len() {
        let c = chars[i];
        if let Some(quote) = in_string {
            if c == '\\' {
                i = (i + 2).min(chars.len());
                continue;
            }
            if c == quote {
                in_string = None;
            }
            i += 1;
            continue;
        }
        if matches!(c, '"' | '\'' | '`') {
            in_string = Some(c);
            if !current.is_empty() {
                push_ident(&mut out, &current);
                current.clear();
            }
            i += 1;
            continue;
        }
        if c == '/' && chars.get(i + 1) == Some(&'/') {
            break;
        }
        let starts_ident = c.is_ascii_alphabetic() || c == '_' || c == '$';
        let extends_ident = c.is_ascii_digit() && !current.is_empty();
        if starts_ident || extends_ident {
            current.push(c);
        } else if !current.is_empty() {
            push_ident(&mut out, &current);
            current.clear();
        }
        i += 1;
    }
    if !current.is_empty() {
        push_ident(&mut out, &current);
    }
    out.truncate(5);
    out
}

fn push_ident(out: &mut Vec<String>, ident: &str) {
    if ident.len() < 2 {
        return;
    }
    if JS_KEYWORDS.contains(&ident) {
        return;
    }
    if ident.chars().next().is_some_and(|c| c.is_ascii_digit()) {
        return;
    }
    if !out.iter().any(|s| s == ident) {
        out.push(ident.to_string());
    }
}

/// Map a CDP `RemoteObject.type` string to a syntax colour for the
/// variables tree. Falls back to TX_2 when unknown.
pub(crate) fn value_color_for(type_str: &str) -> u32 {
    match type_str {
        "string" => theme::SX_STR,
        "number" | "bigint" => theme::SX_NUM,
        "boolean" => theme::SX_NUM,
        "function" => theme::SX_FN,
        "object" => theme::SX_PROP,
        "undefined" | "symbol" => theme::TX_4,
        _ => theme::TX_2,
    }
}

pub(crate) fn method_color_for(m: network::Method) -> u32 {
    use network::Method::*;
    match m {
        Get => theme::INFO,
        Post => theme::ACCENT,
        Put | Patch => theme::WARN,
        Delete => theme::ERROR,
        _ => theme::TX_3,
    }
}

pub(crate) fn status_color(status: u16) -> u32 {
    match status {
        100..=199 => theme::TX_4,
        200..=299 => theme::ACCENT,
        300..=399 => theme::INFO,
        400..=499 => theme::WARN,
        500..=599 => theme::ERROR,
        _ => theme::TX_4,
    }
}

/// Compact JSON preview for inline rendering. Caps at ~120 chars.
pub(crate) fn json_preview(v: &serde_json::Value) -> String {
    let s = match v {
        serde_json::Value::Object(m) if m.is_empty() => "{}".to_string(),
        serde_json::Value::Array(a) if a.is_empty() => "[]".to_string(),
        _ => v.to_string(),
    };
    if s.len() > 120 {
        let mut t: String = s.chars().take(117).collect();
        t.push_str("...");
        t
    } else {
        s
    }
}

pub(crate) fn url_basename(url: &str) -> String {
    let trimmed = url.split('?').next().unwrap_or(url);
    if let Some(idx) = trimmed.rfind('/') {
        let after = &trimmed[idx + 1..];
        if after.is_empty() {
            url.to_string()
        } else {
            after.to_string()
        }
    } else {
        url.to_string()
    }
}

/// Map a [`console::LogLevel`] to a rendering colour.
pub(crate) fn log_level_color(level: console::LogLevel) -> u32 {
    use console::LogLevel::*;
    match level {
        Log => theme::TX_1,
        Info => theme::INFO,
        Warn => theme::WARN,
        Error => theme::ERROR,
        Debug => theme::TX_2,
        Trace => theme::TX_3,
    }
}

/// Split global byte-indexed [`Span`]s into per-line char-indexed runs.
///
/// `src` is the full buffer text; `spans` are byte offsets into that text.
/// The returned vector has one entry per line in `src`, each containing the
/// `(char_start, char_end, kind)` runs that fall on that line (multi-line
/// spans are clipped to each line they touch).
pub(crate) fn spans_to_line_runs(
    src: &str,
    spans: &[Span],
) -> Vec<Vec<(usize, usize, HighlightKind)>> {
    let mut line_starts: Vec<usize> = vec![0];
    for (i, b) in src.bytes().enumerate() {
        if b == b'\n' {
            line_starts.push(i + 1);
        }
    }
    let line_count = line_starts.len();
    let mut out: Vec<Vec<(usize, usize, HighlightKind)>> = vec![Vec::new(); line_count];

    for span in spans {
        let start_line = match line_starts.binary_search(&span.start) {
            Ok(i) => i,
            Err(i) => i.saturating_sub(1),
        };
        let mut line = start_line;
        while line < line_count {
            let line_start = line_starts[line];
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

/// A rendered sub-segment of a single line. `build_runs` emits these in
/// order; the renderer converts each into a gpui `div`.
#[derive(Debug, PartialEq, Eq)]
pub(crate) enum Run {
    Text {
        text: String,
        fg: u32,
        selected: bool,
    },
    Caret,
}

/// Flatten a line into a list of [`Run`]s by merging the three independent
/// per-character attributes that can apply: foreground colour (from syntax
/// highlights), selection background, and the caret overlay.
pub(crate) fn build_runs(
    text: &str,
    caret: Option<usize>,
    selection: Option<(usize, usize)>,
    highlights: &[(usize, usize, HighlightKind)],
) -> Vec<Run> {
    let chars: Vec<char> = text.chars().collect();
    let n = chars.len();

    let mut fg: Vec<u32> = vec![DEFAULT_FG; n];
    for (start, end, kind) in highlights {
        let s = (*start).min(n);
        let e = (*end).min(n);
        let color = highlight_color(*kind);
        for slot in &mut fg[s..e] {
            *slot = color;
        }
    }

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
    if caret == Some(n) {
        runs.push(Run::Caret);
    }
    runs
}

/// Compact `host:port/path-tail` rendering of a CDP WebSocket URL for
/// the status bar. Strips the scheme, keeps host:port, and tail-trims
/// the path so the whole thing fits in ~40 chars.
pub(crate) fn short_ws_url(ws_url: &str) -> String {
    let stripped = ws_url
        .strip_prefix("ws://")
        .or_else(|| ws_url.strip_prefix("wss://"))
        .unwrap_or(ws_url);
    let (host, path) = stripped.split_once('/').unwrap_or((stripped, ""));
    if path.is_empty() {
        return host.to_string();
    }
    let path_tail: String = path
        .chars()
        .rev()
        .take(28)
        .collect::<String>()
        .chars()
        .rev()
        .collect();
    let prefix = if path.chars().count() > 28 {
        "…"
    } else {
        "/"
    };
    format!("{host}{prefix}{path_tail}")
}

pub(crate) fn placeholder(label: impl Into<SharedString>) -> gpui::Div {
    div()
        .flex_1()
        .flex()
        .items_center()
        .justify_center()
        .text_xs()
        .text_color(rgb(theme::TX_4))
        .child(label.into())
}

impl AtomioWindow {
    /// Activity bar (left rail). Per `docs/design.md`: 48px wide, 36x36
    /// items, 2px accent indicator on the left edge of the active row,
    /// hover lifts to BG_3. Each row is clickable to switch the dock.
    pub(crate) fn render_activity_bar(&self, cx: &mut Context<Self>) -> gpui::Div {
        let active = self.dock;
        let items = [
            DockPane::Files,
            DockPane::Debugger,
            DockPane::Network,
            DockPane::Simulator,
            DockPane::Components,
            DockPane::Profiler,
            DockPane::Console,
        ];
        let mut bar = div()
            .w(px(48.0))
            .flex()
            .flex_col()
            .items_center()
            .py_2()
            .gap_1()
            .bg(rgb(theme::BG_2))
            .border_r_1()
            .border_color(rgb(theme::LINE_1));
        for pane in items {
            let is_active = pane == active;
            let (fg, bg) = if is_active {
                (theme::ACCENT, theme::ACCENT_SOFT)
            } else {
                (theme::TX_3, theme::BG_2)
            };
            // Per-row layout: 2px accent strip + 4px gap + 36x36 icon
            // cell. The strip is invisible (BG_2) on inactive rows so the
            // icons stay vertically aligned regardless of selection.
            let indicator_color = if is_active {
                theme::ACCENT
            } else {
                theme::BG_2
            };
            let pane_for_click = pane;
            let row = div()
                .id(SharedString::from(format!("act-{}", pane.label())))
                .flex()
                .flex_row()
                .items_center()
                .child(
                    div()
                        .w(px(2.0))
                        .h(px(20.0))
                        .rounded(px(1.0))
                        .bg(rgb(indicator_color)),
                )
                .child(
                    div()
                        .ml(px(4.0))
                        .w(px(36.0))
                        .h(px(36.0))
                        .flex()
                        .items_center()
                        .justify_center()
                        .rounded(px(6.0))
                        .text_color(rgb(fg))
                        .bg(rgb(bg))
                        .hover(|s| if is_active { s } else { s.bg(rgb(theme::BG_3)) })
                        .child(
                            svg()
                                .path(pane.icon_path())
                                .size(px(18.0))
                                .text_color(rgb(fg)),
                        ),
                )
                .on_click(cx.listener(move |this, _ev, _win, cx| {
                    this.show_dock(pane_for_click, cx);
                }));
            bar = bar.child(row);
        }
        bar
    }

    /// Editor pane (centre). Builds the per-line layout: clickable gutter
    /// (line number + breakpoint dot + paused arrow), then the syntax-coloured
    /// text row, with a paused-line full-row tint when applicable.
    pub(crate) fn render_editor_pane(
        &self,
        line_views: Vec<LineView>,
        gutter_width: gpui::Pixels,
        cx: &mut Context<Self>,
    ) -> gpui::Div {
        // Pre-compute minimap stats before we consume `line_views` in the
        // main render loop. Each entry: (chars_in_line, has_bp, paused).
        // Char count drives bar width; the flags pick the tint.
        let minimap_rows: Vec<(usize, bool, bool)> = line_views
            .iter()
            .map(|lv| {
                (
                    lv.text.chars().count(),
                    lv.has_breakpoint,
                    lv.is_paused_here,
                )
            })
            .collect();

        let mut content = div()
            .flex_1()
            .py_4()
            .flex()
            .flex_col()
            .text_color(rgb(theme::TX_1));
        for lv in line_views {
            let LineView {
                number,
                text,
                caret,
                selection,
                highlights,
                has_breakpoint,
                is_paused_here,
            } = lv;
            let line_idx = (number - 1) as u32;

            let bp_indicator = if has_breakpoint {
                div()
                    .w(px(8.0))
                    .h(px(8.0))
                    .rounded(px(4.0))
                    .bg(rgb(theme::ACCENT))
            } else {
                div().w(px(8.0)).h(px(8.0))
            };
            let arrow = if is_paused_here { "▶" } else { " " };
            let gutter = div()
                .id(SharedString::from(format!("gutter-{line_idx}")))
                .w(gutter_width)
                .pr_2()
                .flex()
                .flex_row()
                .items_center()
                .gap_1()
                .text_color(rgb(theme::TX_5))
                .child(div().w(px(8.0)).child(arrow))
                .child(bp_indicator)
                .child(
                    div()
                        .flex_1()
                        .flex()
                        .justify_end()
                        .child(format!("{number}")),
                )
                .on_click(cx.listener(move |this, _ev, _win, cx| {
                    this.toggle_breakpoint_at(line_idx, cx);
                }));

            let runs = build_runs(&text, caret, selection, &highlights);
            let mut row = div().flex().flex_row();
            for run in runs {
                match run {
                    Run::Caret => {
                        row = row.child(div().w(px(2.0)).h(px(18.0)).bg(rgb(theme::ACCENT)));
                    }
                    Run::Text { text, fg, selected } => {
                        if text.is_empty() {
                            continue;
                        }
                        let bg = if selected || is_paused_here {
                            theme::ACCENT_SOFT
                        } else {
                            theme::BG_1
                        };
                        row = row.child(div().bg(rgb(bg)).text_color(rgb(fg)).child(text));
                    }
                }
            }
            if text.is_empty() && caret.is_none() {
                row = row.child(div().child(" "));
            }

            // Inline value pills: shown after expressions on the paused
            // line. Per docs/design.md the pill background is BG_4; we
            // additionally tint the value text by its CDP `type` so the
            // user can tell number/string/object apart at a glance. The
            // ident label sits in TX_3 for contrast against the value.
            if is_paused_here && !self.inline_values.is_empty() {
                for ident in extract_idents(&text) {
                    if let Some(result) = self.inline_values.get(&ident) {
                        let (display, color) = match result {
                            Ok(rv) => (rv.display.clone(), value_color_for(rv.type_str.as_str())),
                            Err(_) => continue,
                        };
                        row = row.child(
                            div()
                                .ml_2()
                                .px_2()
                                .py(px(1.0))
                                .rounded(px(4.0))
                                .bg(rgb(theme::BG_4))
                                .border_1()
                                .border_color(rgb(theme::LINE_2))
                                .text_xs()
                                .flex()
                                .flex_row()
                                .gap_1()
                                .child(div().text_color(rgb(theme::TX_3)).child(ident.clone()))
                                .child(div().text_color(rgb(theme::TX_4)).child("="))
                                .child(div().text_color(rgb(color)).child(display)),
                        );
                    }
                }
            }

            let row_bg = if is_paused_here {
                theme::ACCENT_SOFT
            } else {
                theme::BG_1
            };
            content = content.child(
                div()
                    .flex()
                    .flex_row()
                    .bg(rgb(row_bg))
                    .child(gutter)
                    .child(row),
            );
        }
        // Minimap: 60px right-edge column per docs/design.md. Each
        // buffer line becomes a 2px bar; width scales with char count.
        // Breakpoint and paused-line rows get accent tints so the
        // overview matches the editor state at a glance.
        let minimap = self.render_minimap(&minimap_rows);
        div()
            .flex()
            .flex_row()
            .flex_1()
            .child(content)
            .child(minimap)
    }

    /// Right-edge minimap column. Per `docs/design.md`: 60px wide, dim
    /// overlay. Width per row scales with line length; max bar width is
    /// 56px to leave a 2px margin from the column edge.
    fn render_minimap(&self, rows: &[(usize, bool, bool)]) -> gpui::Div {
        // Longest line in the visible buffer drives the scale. Empty
        // buffer would divide by zero — clamp to 1.
        let max_chars = rows.iter().map(|(n, _, _)| *n).max().unwrap_or(1).max(1) as f32;
        let mut col = div()
            .w(px(60.0))
            .py_4()
            .pl(px(2.0))
            .pr(px(2.0))
            .flex()
            .flex_col()
            .bg(rgb(theme::BG_1))
            .border_l_1()
            .border_color(rgb(theme::LINE_1));
        for (chars, has_bp, is_paused) in rows {
            // 2px-tall bar; width proportional, min 1px so empty lines
            // still register as a visible tick.
            let frac = (*chars as f32 / max_chars).clamp(0.0, 1.0);
            let bar_w = (frac * 56.0).max(1.0);
            let color = if *is_paused {
                theme::ACCENT
            } else if *has_bp {
                theme::ACCENT_LINE
            } else {
                // TX_5 = the very-dim gutter colour, matches the
                // ~8% overlay feel without needing an alpha channel.
                theme::TX_5
            };
            col = col.child(
                div()
                    .h(px(2.0))
                    .w(px(bar_w))
                    .bg(rgb(color))
                    .rounded(px(1.0))
                    .mb(px(1.0)),
            );
        }
        col
    }

    /// Right dock. Renders the active pane; non-functional ones display
    /// a placeholder pointing at the milestone where they'll land.
    pub(crate) fn render_dock(&self, cx: &mut Context<Self>) -> gpui::Div {
        let pane = self.dock;
        let header = div()
            .px_3()
            .py_1()
            .text_xs()
            .text_color(rgb(theme::TX_3))
            .bg(rgb(theme::BG_0))
            .border_b_1()
            .border_color(rgb(theme::LINE_1))
            .child(pane.label());

        let body = match pane {
            DockPane::Console => self.render_console_body(),
            DockPane::Files => placeholder("File tree ships in v0.6"),
            DockPane::Debugger => self.render_debugger_body(cx),
            DockPane::Simulator => self.render_simulator_body(cx),
            DockPane::Components => self.render_components_body(cx),
            DockPane::Profiler => self.render_profiler_body(cx),
            DockPane::Network => self.render_network_body(cx),
        };

        div()
            .w(px(360.0))
            .flex()
            .flex_col()
            .bg(rgb(theme::BG_2))
            .border_l_1()
            .border_color(rgb(theme::LINE_1))
            .child(header)
            .child(body)
    }

    /// Debugger pane body. v0.2 ships the step-control bar; variables +
    /// call stack land in v0.3.
    fn render_debugger_body(&self, cx: &mut Context<Self>) -> gpui::Div {
        let connected = matches!(self.connection, ConnectionState::Connected { .. });
        let paused = self.paused;

        let bar = div()
            .flex()
            .flex_row()
            .gap_1()
            .px_3()
            .py_2()
            .border_b_1()
            .border_color(rgb(theme::LINE_1))
            .child(
                self.step_button("Resume", "▶", "F5", connected && paused, cx, |cmd| {
                    *cmd = Some(DebuggerCommand::Resume)
                }),
            )
            .child(
                self.step_button("Step Over", "↷", "F10", connected && paused, cx, |cmd| {
                    *cmd = Some(DebuggerCommand::StepOver)
                }),
            )
            .child(
                self.step_button("Step Into", "↳", "F11", connected && paused, cx, |cmd| {
                    *cmd = Some(DebuggerCommand::StepInto)
                }),
            )
            .child(
                self.step_button("Step Out", "↰", "⇧F11", connected && paused, cx, |cmd| {
                    *cmd = Some(DebuggerCommand::StepOut)
                }),
            )
            .child(
                self.step_button("Pause", "⏸", "F6", connected && !paused, cx, |cmd| {
                    *cmd = Some(DebuggerCommand::Pause)
                }),
            );

        let state_line = div()
            .px_3()
            .py_2()
            .text_xs()
            .text_color(rgb(if paused { theme::ACCENT } else { theme::TX_4 }))
            .child(if !connected {
                "(not connected)".to_string()
            } else if paused {
                "● paused".to_string()
            } else {
                "● running".to_string()
            });

        let body = self.render_debugger_lower(cx);

        div()
            .flex()
            .flex_col()
            .child(bar)
            .child(state_line)
            .child(body)
    }

    /// Lower portion of the Debugger pane: call stack list + scopes for
    /// the selected frame.
    fn render_debugger_lower(&self, cx: &mut Context<Self>) -> gpui::Div {
        let Some(stack) = self.call_stack.as_ref() else {
            return div()
                .flex_1()
                .px_3()
                .py_4()
                .text_xs()
                .text_color(rgb(theme::TX_4))
                .child("Hit a breakpoint to see the call stack and scopes.");
        };

        let mut frames_section = div()
            .flex()
            .flex_col()
            .px_3()
            .py_2()
            .border_b_1()
            .border_color(rgb(theme::LINE_1))
            .child(
                div()
                    .text_xs()
                    .text_color(rgb(theme::TX_3))
                    .pb_1()
                    .child(format!("Call stack — {} frames", stack.frames.len())),
            );
        for (i, f) in stack.frames.iter().enumerate() {
            let name = if f.function_name.is_empty() {
                "(anonymous)"
            } else {
                f.function_name.as_str()
            };
            let url = self
                .scripts
                .get(&f.script_id)
                .map(|s| s.url.as_str())
                .unwrap_or(&f.script_id);
            let basename = url.rsplit_once('/').map(|(_, b)| b).unwrap_or(url);
            let row_color = if i == 0 { theme::TX_1 } else { theme::TX_2 };
            let script_id_for_click = f.script_id.clone();
            let line_for_click = f.line;
            frames_section = frames_section.child(
                div()
                    .id(SharedString::from(format!("frame-{i}")))
                    .flex()
                    .flex_row()
                    .justify_between()
                    .text_xs()
                    .text_color(rgb(row_color))
                    .py_1()
                    .rounded(px(3.0))
                    .hover(|s| s.bg(rgb(theme::BG_3)))
                    .child(div().child(name.to_string()))
                    .child(div().text_color(rgb(theme::TX_4)).child(format!(
                        "{}:{}",
                        basename,
                        f.line + 1
                    )))
                    .on_click(cx.listener(move |this, _ev, _win, cx| {
                        this.jump_to_frame(script_id_for_click.clone(), line_for_click, cx);
                    })),
            );
        }

        let mut scopes_section = div().flex().flex_col().px_3().py_2();
        if let Some(top) = stack.frames.first() {
            scopes_section = scopes_section.child(
                div()
                    .text_xs()
                    .text_color(rgb(theme::TX_3))
                    .pb_1()
                    .child(format!("Scopes ({})", top.scopes.len())),
            );
            for (si, scope) in top.scopes.iter().enumerate() {
                let label = scope.kind.label();
                let object_id = scope.object_id.clone();
                let is_open = self.expanded.contains(&object_id);
                let arrow = if is_open { "▾" } else { "▸" };
                let id_for_click = object_id.clone();
                let row = div()
                    .id(SharedString::from(format!("scope-{si}")))
                    .flex()
                    .flex_row()
                    .justify_between()
                    .text_xs()
                    .text_color(rgb(theme::TX_2))
                    .py_1()
                    .child(div().child(format!("{arrow} {label}")))
                    .child(
                        div()
                            .text_color(rgb(theme::TX_4))
                            .child(scope.name.clone().unwrap_or_default()),
                    )
                    .on_click(cx.listener(move |this, _ev, _win, cx| {
                        this.toggle_object(id_for_click.clone(), cx);
                    }));
                scopes_section = scopes_section.child(row);
                if is_open {
                    scopes_section =
                        scopes_section.child(self.render_property_list(&object_id, 1, cx));
                }
            }
        }

        let watch_section = self.render_watch_section();
        let breakpoints_section = self.render_breakpoints_section(cx);

        div()
            .flex_1()
            .flex()
            .flex_col()
            .child(frames_section)
            .child(scopes_section)
            .child(watch_section)
            .child(breakpoints_section)
    }

    /// Breakpoints sidebar: header + one row per registered breakpoint.
    /// Each row shows accent dot + url-basename:line and is clickable
    /// to open that file in the editor on that line. Hit counts will
    /// land once `BreakpointRegistry` tracks them — for now the row
    /// just shows the location and an enabled/disabled tint.
    fn render_breakpoints_section(&self, cx: &mut Context<Self>) -> gpui::Div {
        let mut wrap = div()
            .flex()
            .flex_col()
            .px_3()
            .py_2()
            .border_t_1()
            .border_color(rgb(theme::LINE_1))
            .child(
                div()
                    .text_xs()
                    .text_color(rgb(theme::TX_3))
                    .pb_1()
                    .child(format!("Breakpoints ({})", self.breakpoints.len())),
            );
        if self.breakpoints.is_empty() {
            wrap = wrap.child(
                div()
                    .text_xs()
                    .text_color(rgb(theme::TX_4))
                    .child("(click a line number in the gutter)"),
            );
            return wrap;
        }
        // Sort by (url, line) for stable display order; the registry
        // hashmap iteration is otherwise insertion-undefined.
        let mut entries: Vec<_> = self.breakpoints.iter().collect();
        entries.sort_by(|a, b| {
            a.url
                .cmp(&b.url)
                .then(a.resolved_line.cmp(&b.resolved_line))
        });
        for bp in entries {
            let url = bp.url.clone();
            let line = bp.resolved_line;
            let basename = url_basename(&url);
            let (dot_color, text_color) = if bp.enabled {
                (theme::ACCENT, theme::TX_2)
            } else {
                (theme::TX_5, theme::TX_4)
            };
            let key = format!("bp-{}-{}", bp.id.0, line);
            wrap = wrap.child(
                div()
                    .id(SharedString::from(key))
                    .flex()
                    .flex_row()
                    .items_center()
                    .gap_2()
                    .py(px(2.0))
                    .rounded(px(3.0))
                    .hover(|s| s.bg(rgb(theme::BG_3)))
                    .child(
                        div()
                            .w(px(8.0))
                            .h(px(8.0))
                            .rounded(px(4.0))
                            .bg(rgb(dot_color)),
                    )
                    .child(
                        div()
                            .text_xs()
                            .text_color(rgb(text_color))
                            .child(format!("{basename}:{}", line + 1)),
                    )
                    .on_click(cx.listener(move |this, _ev, _win, cx| {
                        this.open_source_at(url.clone(), line, cx);
                    })),
            );
        }
        wrap
    }

    /// Watch expressions section.
    fn render_watch_section(&self) -> gpui::Div {
        let mut wrap = div()
            .flex()
            .flex_col()
            .px_3()
            .py_2()
            .border_t_1()
            .border_color(rgb(theme::LINE_1))
            .child(
                div()
                    .text_xs()
                    .text_color(rgb(theme::TX_3))
                    .pb_1()
                    .child(format!("Watch ({})", self.watches.len())),
            );
        if self.watches.is_empty() {
            wrap = wrap.child(
                div()
                    .text_xs()
                    .text_color(rgb(theme::TX_4))
                    .child("(empty — palette: Debug: Watch 'this')"),
            );
            return wrap;
        }
        for w in self.watches.iter() {
            let (display, color) = match &w.last {
                None => ("evaluating...".to_string(), theme::TX_4),
                Some(Ok(rv)) => (rv.display.clone(), value_color_for(rv.type_str.as_str())),
                Some(Err(e)) => (e.clone(), theme::ERROR),
            };
            wrap = wrap.child(
                div()
                    .flex()
                    .flex_row()
                    .gap_2()
                    .text_xs()
                    .py(px(2.0))
                    .child(
                        div()
                            .w(px(120.0))
                            .text_color(rgb(theme::TX_2))
                            .child(w.expression.clone()),
                    )
                    .child(div().flex_1().text_color(rgb(color)).child(display)),
            );
        }
        wrap
    }

    /// Recursive render of a property list (for a given `objectId`).
    fn render_property_list(
        &self,
        object_id: &str,
        indent: u32,
        cx: &mut Context<Self>,
    ) -> gpui::Div {
        let mut wrap = div().flex().flex_col().pl(px((indent * 12) as f32));
        let Some(props) = self.properties.get(object_id) else {
            wrap = wrap.child(
                div()
                    .text_xs()
                    .text_color(rgb(theme::TX_4))
                    .py_1()
                    .child("loading..."),
            );
            return wrap;
        };
        if props.is_empty() {
            wrap = wrap.child(
                div()
                    .text_xs()
                    .text_color(rgb(theme::TX_4))
                    .py_1()
                    .child("(empty)"),
            );
            return wrap;
        }
        if indent > 6 {
            wrap = wrap.child(
                div()
                    .text_xs()
                    .text_color(rgb(theme::TX_4))
                    .py_1()
                    .child("(depth limit)"),
            );
            return wrap;
        }
        for (pi, prop) in props.iter().enumerate() {
            let value_display = prop
                .value
                .as_ref()
                .map(|v| v.display.clone())
                .unwrap_or_default();
            let value_color = prop
                .value
                .as_ref()
                .map(|v| value_color_for(v.type_str.as_str()))
                .unwrap_or(theme::TX_3);
            let inner_object_id = prop
                .value
                .as_ref()
                .filter(|v| v.expandable)
                .and_then(|v| v.object_id.clone());
            let is_open = inner_object_id
                .as_ref()
                .is_some_and(|oid| self.expanded.contains(oid));
            let arrow = if inner_object_id.is_none() {
                "  "
            } else if is_open {
                "▾ "
            } else {
                "▸ "
            };
            let row_object_id = inner_object_id.clone();
            let key = format!("{object_id}-{pi}");
            let mut row = div()
                .id(SharedString::from(key))
                .flex()
                .flex_row()
                .gap_2()
                .text_xs()
                .py(px(2.0))
                .child(div().w(px(14.0)).text_color(rgb(theme::TX_4)).child(arrow))
                .child(div().text_color(rgb(theme::TX_2)).child(prop.name.clone()))
                .child(
                    div()
                        .flex_1()
                        .text_color(rgb(value_color))
                        .child(value_display),
                );
            if let Some(oid) = row_object_id {
                let oid_clone = oid.clone();
                row = row.on_click(cx.listener(move |this, _ev, _win, cx| {
                    this.toggle_object(oid_clone.clone(), cx);
                }));
            }
            wrap = wrap.child(row);
            if is_open {
                if let Some(oid) = inner_object_id {
                    wrap = wrap.child(self.render_property_list(&oid, indent + 1, cx));
                }
            }
        }
        wrap
    }

    /// Build one step-control button.
    fn step_button(
        &self,
        label: &'static str,
        glyph: &'static str,
        shortcut: &'static str,
        enabled: bool,
        cx: &mut Context<Self>,
        pick: impl Fn(&mut Option<DebuggerCommand>) + 'static,
    ) -> gpui::Div {
        let (fg, bg) = if enabled {
            (theme::TX_1, theme::BG_3)
        } else {
            (theme::TX_5, theme::BG_2)
        };
        let inner = div()
            .id(SharedString::from(format!("step-{label}")))
            .w(px(58.0))
            .px_2()
            .py_1()
            .rounded(px(4.0))
            .bg(rgb(bg))
            .text_color(rgb(fg))
            .text_xs()
            .flex()
            .flex_col()
            .items_center()
            .gap_1()
            .border_1()
            .border_color(rgb(if enabled {
                theme::LINE_2
            } else {
                theme::LINE_1
            }))
            .hover(|s| {
                if enabled {
                    s.bg(rgb(theme::BG_4)).border_color(rgb(theme::ACCENT_LINE))
                } else {
                    s
                }
            })
            .child(div().child(glyph))
            .child(div().text_color(rgb(theme::TX_4)).child(shortcut))
            .on_click(cx.listener(move |this, _ev, _win, cx| {
                if !enabled {
                    return;
                }
                let mut slot: Option<DebuggerCommand> = None;
                pick(&mut slot);
                if let Some(cmd) = slot {
                    this.send_debug(cmd);
                    cx.notify();
                }
            }));
        div().child(inner)
    }

    /// Network pane body.
    fn render_network_body(&self, cx: &mut Context<Self>) -> gpui::Div {
        use network::{RequestState, ResourceKind};

        let total = self.network.len();
        let header = div()
            .px_3()
            .py_1()
            .text_xs()
            .text_color(rgb(theme::TX_3))
            .child(format!("{total} requests · cmd+shift+p Network: Clear"));

        let mut list = div().flex().flex_col();
        let all: Vec<_> = self.network.iter().cloned().collect();
        let last: Vec<_> = all
            .into_iter()
            .rev()
            .take(200)
            .collect::<Vec<_>>()
            .into_iter()
            .rev()
            .collect();
        if last.is_empty() {
            list = list.child(
                div()
                    .px_3()
                    .py_2()
                    .text_xs()
                    .text_color(rgb(theme::TX_4))
                    .child("(no traffic — fetch / xhr from the running app shows here)"),
            );
        } else {
            for req in last {
                let id = req.id.clone();
                let id_for_click = id.clone();
                let selected = self.selected_request.as_deref() == Some(&id);
                let bg = if selected { theme::BG_3 } else { theme::BG_2 };
                let method_color = method_color_for(req.method);
                let (status_str, status_col) = match &req.state {
                    RequestState::Pending => ("…".to_string(), theme::TX_4),
                    RequestState::Headers { status, .. } => {
                        (status.to_string(), status_color(*status))
                    }
                    RequestState::Done { status, .. } => {
                        (status.to_string(), status_color(*status))
                    }
                    RequestState::Failed { .. } => ("ERR".to_string(), theme::ERROR),
                };
                let timing = match &req.state {
                    RequestState::Done { duration_ms, .. } => format!("{:.0} ms", duration_ms),
                    _ => "—".into(),
                };
                let kind_tag = match req.kind {
                    ResourceKind::Xhr => "XHR",
                    ResourceKind::Fetch => "FETCH",
                    ResourceKind::WebSocket => "WS",
                    ResourceKind::Document => "DOC",
                    ResourceKind::Stylesheet => "CSS",
                    ResourceKind::Script => "JS",
                    ResourceKind::Image => "IMG",
                    ResourceKind::Font => "FONT",
                    ResourceKind::EventSource => "SSE",
                    ResourceKind::Other => "—",
                };
                let url_short = url_basename(&req.url);
                list = list.child(
                    div()
                        .id(SharedString::from(format!("net-{id}")))
                        .flex()
                        .flex_row()
                        .items_center()
                        .gap_2()
                        .px_3()
                        .py(px(2.0))
                        .text_xs()
                        .bg(rgb(bg))
                        .child(
                            div()
                                .w(px(40.0))
                                .text_color(rgb(method_color))
                                .child(req.method.label()),
                        )
                        .child(
                            div()
                                .w(px(34.0))
                                .text_color(rgb(theme::TX_4))
                                .child(kind_tag),
                        )
                        .child(div().flex_1().text_color(rgb(theme::TX_2)).child(url_short))
                        .child(
                            div()
                                .w(px(36.0))
                                .text_color(rgb(status_col))
                                .child(status_str),
                        )
                        .child(div().w(px(48.0)).text_color(rgb(theme::TX_4)).child(timing))
                        .on_click(cx.listener(move |this, _ev, _win, cx| {
                            this.selected_request = Some(id_for_click.clone());
                            cx.notify();
                        })),
                );
            }
        }

        let detail = if let Some(id) = self.selected_request.as_deref() {
            self.network
                .get(id)
                .map(|req| self.render_network_detail(req))
        } else {
            None
        };

        let mut wrap = div().flex().flex_col().child(header).child(list);
        if let Some(d) = detail {
            wrap = wrap.child(d);
        }
        wrap
    }

    /// Detail strip for the selected request.
    fn render_network_detail(&self, req: &network::NetworkRequest) -> gpui::Div {
        use network::WsDirection;

        let mut detail = div()
            .flex()
            .flex_col()
            .px_3()
            .py_2()
            .border_t_1()
            .border_color(rgb(theme::LINE_1))
            .text_xs()
            .text_color(rgb(theme::TX_2))
            .child(div().text_color(rgb(theme::TX_3)).pb_1().child("Request"))
            .child(div().text_color(rgb(theme::TX_2)).child(req.url.clone()));
        if !req.req_headers.is_empty() {
            for (k, v) in req.req_headers.iter().take(8) {
                detail = detail.child(
                    div()
                        .text_color(rgb(theme::TX_4))
                        .child(format!("{k}: {v}")),
                );
            }
        }
        if !req.resp_headers.is_empty() {
            detail = detail.child(div().text_color(rgb(theme::TX_3)).pt_1().child("Response"));
            for (k, v) in req.resp_headers.iter().take(8) {
                detail = detail.child(
                    div()
                        .text_color(rgb(theme::TX_4))
                        .child(format!("{k}: {v}")),
                );
            }
        }
        if !req.ws_frames.is_empty() {
            detail = detail.child(
                div()
                    .text_color(rgb(theme::TX_3))
                    .pt_1()
                    .child(format!("WebSocket frames ({})", req.ws_frames.len())),
            );
            for frame in req.ws_frames.iter().rev().take(20) {
                let arrow = match frame.direction {
                    WsDirection::Sent => "→",
                    WsDirection::Received => "←",
                };
                let preview: String = frame.payload.chars().take(120).collect();
                detail = detail.child(
                    div()
                        .text_color(rgb(theme::TX_4))
                        .child(format!("{arrow} {preview}")),
                );
            }
        }
        detail
    }

    /// Simulator pane body.
    fn render_simulator_body(&self, cx: &mut Context<Self>) -> gpui::Div {
        let header = div()
            .flex()
            .flex_row()
            .items_center()
            .justify_between()
            .px_3()
            .py_1()
            .text_xs()
            .text_color(rgb(theme::TX_3))
            .child(div().child(format!(
                "frames: {}{}",
                self.screenshot_count,
                if self.screenshot_path.is_some() {
                    ""
                } else {
                    " (no capture yet)"
                }
            )))
            .child(
                div()
                    .id(SharedString::from("sim-capture"))
                    .px_2()
                    .py(px(2.0))
                    .rounded(px(4.0))
                    .bg(rgb(theme::BG_3))
                    .text_color(rgb(theme::TX_1))
                    .child("◉ Capture")
                    .on_click(cx.listener(|this, _ev, win, cx| {
                        this.on_simulator_capture(&SimulatorCapture, win, cx);
                    })),
            );

        let mut frame = div()
            .mt_4()
            .mx_auto()
            .w(px(280.0))
            .h(px(560.0))
            .rounded(px(28.0))
            .bg(rgb(theme::BG_0))
            .border_2()
            .border_color(rgb(theme::LINE_3))
            .flex()
            .items_center()
            .justify_center();
        if let Some(path) = self.screenshot_path.as_ref() {
            frame = frame.child(
                gpui::img(path.clone())
                    .w(px(260.0))
                    .h(px(540.0))
                    .rounded(px(20.0)),
            );
        } else {
            frame = frame.child(
                div()
                    .text_xs()
                    .text_color(rgb(theme::TX_4))
                    .child("press Capture to fetch a frame"),
            );
        }

        div().flex().flex_col().child(header).child(frame).child(
            div()
                .px_3()
                .pt_2()
                .text_xs()
                .text_color(rgb(theme::TX_4))
                .child("uses Page.captureScreenshot · live stream lands in v0.5 follow-up"),
        )
    }

    /// Profiler pane body.
    fn render_profiler_body(&self, cx: &mut Context<Self>) -> gpui::Div {
        let max = self.metrics.iter().map(|(_, v)| *v).fold(0.0f64, f64::max);

        let header = div()
            .flex()
            .flex_row()
            .items_center()
            .justify_between()
            .px_3()
            .py_1()
            .text_xs()
            .text_color(rgb(theme::TX_3))
            .child(div().child(format!("{} metrics", self.metrics.len())))
            .child(
                div()
                    .id(SharedString::from("profiler-refresh"))
                    .px_2()
                    .py(px(2.0))
                    .rounded(px(4.0))
                    .bg(rgb(theme::BG_3))
                    .text_color(rgb(theme::TX_1))
                    .child("⟳ Refresh")
                    .on_click(cx.listener(|this, _ev, win, cx| {
                        this.on_profiler_refresh(&ProfilerRefresh, win, cx);
                    })),
            );

        if self.metrics.is_empty() {
            return div().flex().flex_col().child(header).child(
                div()
                    .px_3()
                    .py_4()
                    .text_xs()
                    .text_color(rgb(theme::TX_4))
                    .child("(no snapshot — palette: Profiler: Refresh Metrics)"),
            );
        }

        let mut list = div().flex().flex_col().px_3().py_1();
        for (name, value) in &self.metrics {
            let frac = if max > 0.0 {
                (*value / max).clamp(0.0, 1.0)
            } else {
                0.0
            };
            let bar_w = (frac * 200.0) as f32;
            list = list.child(
                div()
                    .flex()
                    .flex_row()
                    .items_center()
                    .gap_2()
                    .text_xs()
                    .py(px(2.0))
                    .child(
                        div()
                            .w(px(120.0))
                            .text_color(rgb(theme::TX_2))
                            .child(name.clone()),
                    )
                    .child(
                        div()
                            .h(px(6.0))
                            .w(px(bar_w))
                            .bg(rgb(theme::ACCENT_SOFT))
                            .border_1()
                            .border_color(rgb(theme::ACCENT_LINE))
                            .rounded(px(2.0)),
                    )
                    .child(
                        div()
                            .ml_2()
                            .text_color(rgb(theme::TX_4))
                            .child(format!("{value:.2}")),
                    ),
            );
        }

        div().flex().flex_col().child(header).child(list)
    }

    /// Components pane body.
    fn render_components_body(&self, cx: &mut Context<Self>) -> gpui::Div {
        if self.components.is_empty() {
            return div()
                .flex_1()
                .px_3()
                .py_4()
                .text_xs()
                .text_color(rgb(theme::TX_4))
                .child("(empty — palette: Components: Load Demo Tree)");
        }

        let header = div()
            .px_3()
            .py_1()
            .text_xs()
            .text_color(rgb(theme::TX_3))
            .child(format!("{} components", self.components.len()));

        let mut list = div().flex().flex_col();
        let flat = self.components.flatten();
        for (id, depth) in flat {
            let Some(node) = self.components.get(&id) else {
                continue;
            };
            let selected = self.selected_node.as_deref() == Some(&id);
            let bg = if selected { theme::BG_3 } else { theme::BG_2 };
            let id_for_click = id.clone();
            let key_str = node
                .key
                .as_deref()
                .map(|k| format!(" key={k}"))
                .unwrap_or_default();
            list = list.child(
                div()
                    .id(SharedString::from(format!("comp-{id}")))
                    .flex()
                    .flex_row()
                    .items_center()
                    .text_xs()
                    .px_3()
                    .py(px(2.0))
                    .pl(px((12 + depth * 12) as f32))
                    .bg(rgb(bg))
                    .child(
                        div()
                            .text_color(rgb(theme::SX_COMP))
                            .child(format!("<{}>", node.name)),
                    )
                    .child(div().ml_2().text_color(rgb(theme::TX_4)).child(key_str))
                    .on_click(cx.listener(move |this, _ev, _win, cx| {
                        this.selected_node = Some(id_for_click.clone());
                        cx.notify();
                    })),
            );
        }

        let detail = self.selected_node.as_deref().and_then(|id| {
            self.components.get(id).map(|node| {
                let mut d = div()
                    .flex()
                    .flex_col()
                    .px_3()
                    .py_2()
                    .border_t_1()
                    .border_color(rgb(theme::LINE_1))
                    .text_xs()
                    .text_color(rgb(theme::TX_2))
                    .child(
                        div()
                            .text_color(rgb(theme::TX_3))
                            .pb_1()
                            .child(format!("<{}>", node.name)),
                    );
                d = d.child(div().text_color(rgb(theme::TX_3)).pt_1().child("Props"));
                d = d.child(
                    div()
                        .text_color(rgb(theme::TX_2))
                        .child(json_preview(&node.props)),
                );
                d = d.child(div().text_color(rgb(theme::TX_3)).pt_1().child("State"));
                d = d.child(
                    div()
                        .text_color(rgb(theme::TX_2))
                        .child(json_preview(&node.state)),
                );
                d
            })
        });

        let mut wrap = div().flex().flex_col().child(header).child(list);
        if let Some(d) = detail {
            wrap = wrap.child(d);
        }
        wrap
    }

    fn render_console_body(&self) -> gpui::Div {
        let entries: Vec<_> = self
            .console
            .entries()
            .iter()
            .rev()
            .take(200)
            .cloned()
            .collect();
        let header = format!(
            "{} entries · {} scripts loaded",
            self.console.len(),
            self.scripts.len()
        );
        let mut list = div().flex().flex_col().px_3().py_1().text_xs();
        list = list.child(div().text_color(rgb(theme::TX_4)).pb_1().child(header));
        if entries.is_empty() {
            list = list.child(
                div()
                    .text_color(rgb(theme::TX_4))
                    .child("(no entries — cmd+shift+d to connect)"),
            );
        } else {
            for entry in entries.into_iter().rev() {
                let tag_color = log_level_color(entry.level);
                let row = div()
                    .flex()
                    .flex_row()
                    .gap_2()
                    .child(
                        div()
                            .w(px(36.0))
                            .text_color(rgb(tag_color))
                            .child(entry.level.tag()),
                    )
                    .child(div().text_color(rgb(theme::TX_1)).child(entry.message));
                list = list.child(row);
            }
        }
        list
    }
}

impl Focusable for AtomioWindow {
    fn focus_handle(&self, _cx: &gpui::App) -> FocusHandle {
        self.focus_handle.clone()
    }
}

impl Render for AtomioWindow {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        // Drain any debugger events that arrived since the last render.
        self.drain_bridge_events(cx);

        let line_views = self.buffer_line_views();
        let gutter_width = {
            let digits = line_views.len().max(1).to_string().len();
            px(((digits + 1) * 10) as f32)
        };
        let subtitle = self.subtitle();
        let cursor = self.cursor_label();
        let status = self.status.clone();
        let title = self.title();
        // Right-side status cluster: language label + dirty flag +
        // cursor position. Language derives from the buffer's path
        // extension; falls back to "plain" when unknown.
        let language_label: SharedString = self
            .state
            .buffer
            .path()
            .and_then(Language::from_path)
            .map(|l| l.label().into())
            .unwrap_or_else(|| "plain".into());
        let dirty_marker: Option<&'static str> = self.state.buffer.is_dirty().then_some("●");
        // Status bar connection label: dot + state + (when connected)
        // a short host:port form of the ws_url so the user can confirm
        // which Metro target the bridge picked.
        let connection_label: SharedString = match &self.connection {
            ConnectionState::Disconnected => "● disconnected".into(),
            ConnectionState::Scanning => "● scanning".into(),
            ConnectionState::Connecting { ws_url } => {
                format!("● connecting · {}", short_ws_url(ws_url)).into()
            }
            ConnectionState::Connected { ws_url } => {
                format!("● connected · {}", short_ws_url(ws_url)).into()
            }
            ConnectionState::Failed { reason } => {
                let s: String = reason.chars().take(40).collect();
                format!("● failed · {s}").into()
            }
        };
        let connection_color: u32 = match &self.connection {
            ConnectionState::Connected { .. } => theme::ACCENT,
            ConnectionState::Connecting { .. } | ConnectionState::Scanning => theme::WARN,
            ConnectionState::Failed { .. } => theme::ERROR,
            ConnectionState::Disconnected => theme::TX_4,
        };

        let palette_overlay: Option<gpui::Div> = self.palette_query.as_ref().map(|query| {
            let matches = self.commands.search(query);
            let selected = self.palette_selected;
            let mut inner = div()
                .flex()
                .flex_col()
                .w(px(440.0))
                .bg(rgb(theme::BG_4))
                .rounded(px(8.0))
                .py_1()
                .shadow_lg()
                .border_1()
                .border_color(rgb(theme::LINE_3))
                .child(
                    div()
                        .px_3()
                        .py_2()
                        .text_sm()
                        .text_color(rgb(theme::TX_1))
                        .child(if query.is_empty() {
                            "> type to search · = watch · ? cond bp · ! logpoint".to_string()
                        } else {
                            format!("> {query}")
                        }),
                );
            let prefix_active = matches!(query.chars().next(), Some('=') | Some('?') | Some('!'));
            if !matches.is_empty() && !prefix_active {
                inner = inner.child(div().h(px(1.0)).mx_2().bg(rgb(theme::LINE_2)));
            }
            let candidates: Vec<_> = if prefix_active {
                Vec::new()
            } else {
                matches.iter().take(10).cloned().collect()
            };
            for (i, m) in candidates.iter().enumerate() {
                let bg = if i == selected {
                    theme::BG_3
                } else {
                    theme::BG_4
                };
                inner = inner.child(
                    div()
                        .px_3()
                        .py_1()
                        .text_sm()
                        .text_color(rgb(theme::TX_1))
                        .bg(rgb(bg))
                        .rounded(px(4.0))
                        .mx_1()
                        .child(m.command.label.clone()),
                );
            }
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
            .on_action(cx.listener(Self::on_open_project))
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
            .on_action(cx.listener(Self::on_clear_console))
            .on_action(cx.listener(Self::on_open_first_script))
            .on_action(cx.listener(Self::on_open_log_file))
            .on_action(cx.listener(Self::on_connect))
            .on_action(cx.listener(Self::on_disconnect))
            .on_action(cx.listener(Self::on_debug_resume))
            .on_action(cx.listener(Self::on_debug_step_over))
            .on_action(cx.listener(Self::on_debug_step_into))
            .on_action(cx.listener(Self::on_debug_step_out))
            .on_action(cx.listener(Self::on_debug_pause))
            .on_action(cx.listener(Self::on_watch_this))
            .on_action(cx.listener(Self::on_watch_clear))
            .on_action(cx.listener(Self::on_show_files))
            .on_action(cx.listener(Self::on_show_debugger))
            .on_action(cx.listener(Self::on_show_simulator))
            .on_action(cx.listener(Self::on_show_components))
            .on_action(cx.listener(Self::on_show_profiler))
            .on_action(cx.listener(Self::on_show_console))
            .on_action(cx.listener(Self::on_show_network))
            .on_action(cx.listener(Self::on_clear_network))
            .on_action(cx.listener(Self::on_components_load_demo))
            .on_action(cx.listener(Self::on_components_clear))
            .on_action(cx.listener(Self::on_profiler_refresh))
            .on_action(cx.listener(Self::on_simulator_capture))
            .on_key_down(cx.listener(Self::on_key_down))
            .flex()
            .flex_col()
            .size_full()
            .bg(rgb(theme::BG_1))
            .text_color(rgb(theme::TX_1))
            .child(
                div()
                    .h(px(36.0))
                    .flex()
                    .items_center()
                    .justify_center()
                    .px_4()
                    // Spec: linear-gradient(180deg, #1a1f27 0%, #14181f 100%).
                    // gpui angle: 0 = top -> increasing clockwise, so 180 =
                    // bottom. We want the lighter shade at the top, so the
                    // gradient line points down (angle 180) with TOP at 0%.
                    .bg(linear_gradient(
                        180.0,
                        linear_color_stop(rgb(theme::TITLEBAR_TOP), 0.0),
                        linear_color_stop(rgb(theme::TITLEBAR_BOT), 1.0),
                    ))
                    .border_b_1()
                    .border_color(rgb(theme::LINE_1))
                    .child(
                        div()
                            .flex()
                            .gap_2()
                            .items_center()
                            .text_xs()
                            .text_color(rgb(theme::TX_3))
                            .child(div().child(title))
                            .child(div().text_color(rgb(theme::TX_4)).child(subtitle)),
                    ),
            )
            .child(
                div()
                    .flex_1()
                    .flex()
                    .flex_row()
                    .child(self.render_activity_bar(cx))
                    .child(self.render_editor_pane(line_views, gutter_width, cx))
                    .child(self.render_dock(cx)),
            )
            .child(
                div()
                    .flex()
                    .justify_between()
                    .items_center()
                    .px_4()
                    .py_1()
                    .h(px(22.0))
                    .bg(rgb(theme::BG_2))
                    .border_t_1()
                    .border_color(rgb(theme::LINE_1))
                    .text_xs()
                    .text_color(rgb(theme::TX_3))
                    .child(
                        div()
                            .flex()
                            .gap_3()
                            .child(
                                div()
                                    .text_color(rgb(connection_color))
                                    .child(connection_label),
                            )
                            .child(div().child(status)),
                    )
                    .child(
                        div()
                            .flex()
                            .gap_3()
                            .items_center()
                            .child(div().text_color(rgb(theme::TX_4)).child(language_label))
                            .children(
                                dirty_marker.map(|m| div().text_color(rgb(theme::WARN)).child(m)),
                            )
                            .child(div().child(cursor)),
                    ),
            )
            .children(palette_overlay)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn short_ws_url_strips_scheme_and_keeps_short_path() {
        let s = short_ws_url("ws://localhost:8081/inspector/debug");
        assert_eq!(s, "localhost:8081/inspector/debug");
    }

    #[test]
    fn short_ws_url_tail_trims_long_path() {
        let s =
            short_ws_url("ws://localhost:8081/inspector/debug/very/very/very/long/extra/path/here");
        assert!(s.starts_with("localhost:8081…"));
        assert!(s.ends_with("path/here"));
    }

    #[test]
    fn short_ws_url_handles_no_path() {
        assert_eq!(short_ws_url("ws://localhost:8081"), "localhost:8081");
    }

    #[test]
    fn short_ws_url_handles_wss() {
        let s = short_ws_url("wss://example.com:443/x");
        assert_eq!(s, "example.com:443/x");
    }

    #[test]
    fn extract_idents_basic() {
        let v = extract_idents("const total = items.reduce((a, b) => a + b.price, 0);");
        assert!(v.contains(&"total".to_string()));
        assert!(v.contains(&"items".to_string()));
        assert!(!v.contains(&"const".to_string()));
        assert!(!v.contains(&"a".to_string()));
    }

    #[test]
    fn extract_idents_skips_strings() {
        let v = extract_idents(r#"console.log("hello world", x);"#);
        assert!(!v.iter().any(|s| s == "hello"));
        assert!(!v.iter().any(|s| s == "world"));
    }

    #[test]
    fn extract_idents_caps_to_five() {
        let v = extract_idents("aa bb cc dd ee ff gg hh");
        assert_eq!(v.len(), 5);
    }

    #[test]
    fn extract_idents_skips_comments() {
        let v = extract_idents("foo // shouldNotAppear");
        assert!(v.contains(&"foo".to_string()));
        assert!(!v.iter().any(|s| s == "shouldNotAppear"));
    }

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
