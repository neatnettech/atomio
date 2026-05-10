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

mod cdp_bridge;
mod theme;

use std::collections::{HashMap, HashSet};
use std::path::PathBuf;
use std::time::Duration;

use cdp_bridge::{ConnectionState, DebuggerBridge, DebuggerCommand, DebuggerEvent};
use console::Console;
use debugger::breakpoints::BreakpointRegistry;
use debugger::scripts::ScriptRegistry;
use editor_core::{Buffer, CommandRegistry, EditorState};
use gpui::{
    actions, div, point, prelude::*, px, rgb, size, Application, Bounds, ClipboardItem, Context,
    FocusHandle, Focusable, KeyBinding, KeyDownEvent, Render, SharedString, TitlebarOptions,
    Window, WindowBackgroundAppearance, WindowBounds, WindowOptions,
};
use inspector::{CallStack, Property, WatchList};
use language::{highlight, HighlightKind, Language, Span};
use network::{NetworkRegistry, RequestState, ResourceKind, WsDirection};
use react_tree::{ComponentTree, NodeId};
use tracing::debug;

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
        Connect,
        Disconnect,
        ClearConsole,
        OpenFirstScript,
        OpenLogFile,
        DebugResume,
        DebugStepOver,
        DebugStepInto,
        DebugStepOut,
        DebugPause,
        WatchThis,
        WatchClear,
        ShowFiles,
        ShowDebugger,
        ShowSimulator,
        ShowComponents,
        ShowProfiler,
        ShowConsole,
        ShowNetwork,
        ClearNetwork,
        ComponentsLoadDemo,
        ComponentsClear,
        ProfilerRefresh,
        SimulatorCapture,
    ]
);

/// Breakpoint flavour for the conditional / logpoint palette flow.
#[derive(Debug, Clone, Copy)]
enum BpKind {
    Conditional,
    Logpoint,
}

/// Right-dock pane selector. Mirrors the activity-bar items in the design
/// spec; only [`DockPane::Console`] and [`DockPane::Files`] are functional
/// today, the others render placeholders pointing at the milestone where
/// they'll land.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum DockPane {
    Files,
    Debugger,
    Simulator,
    Components,
    Profiler,
    Console,
    Network,
}

impl DockPane {
    fn label(self) -> &'static str {
        match self {
            DockPane::Files => "Files",
            DockPane::Debugger => "Debugger",
            DockPane::Simulator => "Simulator",
            DockPane::Components => "Components",
            DockPane::Profiler => "Profiler",
            DockPane::Console => "Console",
            DockPane::Network => "Network",
        }
    }

    /// Single-character glyph for the activity bar. Real lucide-style icons
    /// land in a follow-up PR; for now a calm monospace symbol.
    fn glyph(self) -> &'static str {
        match self {
            DockPane::Files => "F",
            DockPane::Debugger => "D",
            DockPane::Simulator => "S",
            DockPane::Components => "C",
            DockPane::Profiler => "P",
            DockPane::Console => "L",
            DockPane::Network => "N",
        }
    }
}

struct LineView {
    number: usize,
    text: String,
    caret: Option<usize>,
    /// `(start_col, end_col)` character range selected on this line, if any.
    selection: Option<(usize, usize)>,
    /// Syntax highlight runs on this line, in char-column space. Sorted and
    /// non-overlapping. Empty when the file has no recognised language.
    highlights: Vec<(usize, usize, HighlightKind)>,
    /// True when a breakpoint is set on this line in the current source.
    has_breakpoint: bool,
    /// True when the runtime is paused on this exact line.
    is_paused_here: bool,
}

/// Map a [`HighlightKind`] to an RGB foreground colour. This is where the
/// "theme" lives for now — a single built-in palette loosely modelled on
/// Catppuccin Mocha, matching the rest of the window chrome.
fn highlight_color(kind: HighlightKind) -> u32 {
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

const DEFAULT_FG: u32 = theme::SX_PL;

/// JS keywords/operators we don't want as inline-value targets.
const JS_KEYWORDS: &[&str] = &[
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
fn extract_idents(line: &str) -> Vec<String> {
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
fn value_color_for(type_str: &str) -> u32 {
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

fn method_color_for(m: network::Method) -> u32 {
    use network::Method::*;
    match m {
        Get => theme::INFO,
        Post => theme::ACCENT,
        Put | Patch => theme::WARN,
        Delete => theme::ERROR,
        _ => theme::TX_3,
    }
}

fn status_color(status: u16) -> u32 {
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
fn json_preview(v: &serde_json::Value) -> String {
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

fn url_basename(url: &str) -> String {
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
fn log_level_color(level: console::LogLevel) -> u32 {
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
    /// Console log buffer fed by the CDP bridge.
    console: Console,
    /// Current debugger connection state, mirrored from the worker thread.
    connection: ConnectionState,
    /// Bridge to the CDP worker thread. `None` until the runtime initialises it.
    bridge: Option<DebuggerBridge>,
    /// Scripts the runtime has reported via `Debugger.scriptParsed`.
    scripts: ScriptRegistry,
    /// Breakpoints set in the current debug session.
    breakpoints: BreakpointRegistry,
    /// True when runtime is paused at a breakpoint or step.
    paused: bool,
    /// Last paused-frames payload (raw CDP `callFrames`). Decoded on demand
    /// when rendering the call stack.
    /// Decoded call stack from the most recent `Debugger.paused` event.
    call_stack: Option<CallStack>,
    /// CDP `objectId` -> last-fetched properties. Populated by Properties
    /// events; cleared on resume.
    properties: HashMap<String, Vec<Property>>,
    /// `objectId`s currently rendered as expanded in the variables tree.
    expanded: HashSet<String>,
    /// Counter for tagging in-flight `GetProperties` requests.
    next_props_tag: u64,
    /// Tag -> `objectId` for matching response back to the right key.
    pending_props_tags: HashMap<u64, String>,
    /// User-defined watch expressions, evaluated on each pause.
    watches: WatchList,
    /// Tag -> watch index for matching `WatchEvaluated` responses.
    pending_watch_tags: HashMap<u64, usize>,
    /// Counter for tagging in-flight evaluate requests.
    next_watch_tag: u64,
    /// Inline values for identifiers on the paused line. Keyed by ident
    /// name; cleared on resume.
    inline_values: HashMap<String, Result<inspector::RemoteValue, String>>,
    /// Captured network requests + WS frames.
    network: NetworkRegistry,
    /// Selected request id in the network pane.
    selected_request: Option<String>,
    /// React component tree.
    components: ComponentTree,
    /// Selected node in the components pane.
    selected_node: Option<NodeId>,
    /// Most recent `Performance.getMetrics` snapshot.
    metrics: Vec<(String, f64)>,
    /// Filesystem path of the last decoded screenshot. The bytes live on
    /// disk so gpui's image loader can pick them up via `img(path)`.
    screenshot_path: Option<PathBuf>,
    /// Capture count for the simulator pane label.
    screenshot_count: u64,
    /// Currently active right-dock pane.
    dock: DockPane,
    /// CDP source URL of the currently displayed buffer (when fetched from
    /// Metro). Required to scope breakpoints + match paused locations.
    current_url: Option<String>,
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
    reg.register("Debug: Connect", "connect");
    reg.register("Debug: Disconnect", "disconnect");
    reg.register("Debug: Clear Console", "clear_console");
    reg.register("Debug: Open First Script", "open_first_script");
    reg.register("Debug: Open Log File", "open_log_file");
    reg.register("Debug: Resume (F5)", "debug_resume");
    reg.register("Debug: Step Over (F10)", "debug_step_over");
    reg.register("Debug: Step Into (F11)", "debug_step_into");
    reg.register("Debug: Step Out (Shift+F11)", "debug_step_out");
    reg.register("Debug: Pause", "debug_pause");
    reg.register("Debug: Watch 'this'", "watch_this");
    reg.register("Debug: Clear Watches", "watch_clear");
    reg.register("View: Files Pane", "show_files");
    reg.register("View: Debugger Pane", "show_debugger");
    reg.register("View: Simulator Pane", "show_simulator");
    reg.register("View: Components Pane", "show_components");
    reg.register("View: Profiler Pane", "show_profiler");
    reg.register("View: Console Pane", "show_console");
    reg.register("View: Network Pane", "show_network");
    reg.register("Network: Clear", "clear_network");
    reg.register("Components: Load Demo Tree", "components_load_demo");
    reg.register("Components: Clear", "components_clear");
    reg.register("Profiler: Refresh Metrics", "profiler_refresh");
    reg.register("Simulator: Capture Screenshot", "simulator_capture");
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

        // Run the syntax highlighter once per render when we recognise
        // the file's language by extension. Full-text reparse per render
        // is fine for small buffers; incremental reparse is a future
        // optimisation tracked for v0.2.
        let lang = buffer.path().and_then(Language::from_path);
        let line_runs: Vec<Vec<(usize, usize, HighlightKind)>> = if let Some(lang) = lang {
            let src = buffer.slice_to_string(0..buffer.len_chars());
            let spans = highlight(&src, lang);
            spans_to_line_runs(&src, &spans)
        } else {
            Vec::new()
        };

        // Pre-compute per-line breakpoint set + paused-line for the
        // current source URL. Lookups are linear over a tiny registry,
        // fine for hundreds of breakpoints.
        let bp_lines: std::collections::HashSet<u32> = match &self.current_url {
            Some(url) => self
                .breakpoints
                .iter_for_url(url)
                .map(|bp| bp.resolved_line)
                .collect(),
            None => Default::default(),
        };
        let paused_line = self.paused_line_in_current();

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
                let line_idx = i as u32;
                LineView {
                    number: i + 1,
                    text,
                    caret,
                    selection,
                    highlights,
                    has_breakpoint: bp_lines.contains(&line_idx),
                    is_paused_here: paused_line == Some(line_idx),
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
        // Special prefix sigils let users feed free-form input to the
        // debugger without a separate modal:
        //   `=expr` -- add watch
        //   `?expr` -- set conditional breakpoint at cursor line
        //   `!expr` -- set logpoint at cursor line (logs `expr`, never halts)
        if let Some(rest) = query.strip_prefix('=') {
            let expr = rest.trim();
            if !expr.is_empty() {
                self.add_watch(expr.to_string(), cx);
            }
            self.palette_selected = 0;
            cx.notify();
            return;
        }
        if let Some(rest) = query.strip_prefix('?') {
            let expr = rest.trim();
            if !expr.is_empty() {
                self.set_breakpoint_with_kind(BpKind::Conditional, expr, cx);
            }
            self.palette_selected = 0;
            cx.notify();
            return;
        }
        if let Some(rest) = query.strip_prefix('!') {
            let expr = rest.trim();
            if !expr.is_empty() {
                self.set_breakpoint_with_kind(BpKind::Logpoint, expr, cx);
            }
            self.palette_selected = 0;
            cx.notify();
            return;
        }
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
            "connect" => self.on_connect(&Connect, window, cx),
            "disconnect" => self.on_disconnect(&Disconnect, window, cx),
            "clear_console" => self.on_clear_console(&ClearConsole, window, cx),
            "open_first_script" => self.on_open_first_script(&OpenFirstScript, window, cx),
            "open_log_file" => self.on_open_log_file(&OpenLogFile, window, cx),
            "debug_resume" => self.on_debug_resume(&DebugResume, window, cx),
            "debug_step_over" => self.on_debug_step_over(&DebugStepOver, window, cx),
            "debug_step_into" => self.on_debug_step_into(&DebugStepInto, window, cx),
            "debug_step_out" => self.on_debug_step_out(&DebugStepOut, window, cx),
            "debug_pause" => self.on_debug_pause(&DebugPause, window, cx),
            "watch_this" => self.on_watch_this(&WatchThis, window, cx),
            "watch_clear" => self.on_watch_clear(&WatchClear, window, cx),
            "show_files" => self.on_show_files(&ShowFiles, window, cx),
            "show_debugger" => self.on_show_debugger(&ShowDebugger, window, cx),
            "show_simulator" => self.on_show_simulator(&ShowSimulator, window, cx),
            "show_components" => self.on_show_components(&ShowComponents, window, cx),
            "show_profiler" => self.on_show_profiler(&ShowProfiler, window, cx),
            "show_console" => self.on_show_console(&ShowConsole, window, cx),
            "show_network" => self.on_show_network(&ShowNetwork, window, cx),
            "clear_network" => self.on_clear_network(&ClearNetwork, window, cx),
            "components_load_demo" => self.on_components_load_demo(&ComponentsLoadDemo, window, cx),
            "components_clear" => self.on_components_clear(&ComponentsClear, window, cx),
            "profiler_refresh" => self.on_profiler_refresh(&ProfilerRefresh, window, cx),
            "simulator_capture" => self.on_simulator_capture(&SimulatorCapture, window, cx),
            _ => {}
        }
    }

    fn on_clear_console(&mut self, _: &ClearConsole, _: &mut Window, cx: &mut Context<Self>) {
        self.console.clear();
        cx.notify();
    }
    fn on_open_log_file(&mut self, _: &OpenLogFile, _: &mut Window, cx: &mut Context<Self>) {
        let path = log_file_path();
        match Buffer::open(&path) {
            Ok(buf) => {
                self.state.replace_buffer(buf);
                self.state.set_read_only(true);
                self.status = format!("opened log: {}", path.display()).into();
            }
            Err(e) => {
                self.status = format!("open log failed: {e}").into();
            }
        }
        cx.notify();
    }
    fn on_open_first_script(
        &mut self,
        _: &OpenFirstScript,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(bridge) = &self.bridge else {
            return;
        };
        let Some(script) = self.scripts.iter().next() else {
            self.status = "no scripts known yet — connect and let runtime load".into();
            cx.notify();
            return;
        };
        let url = script.url.clone();
        let _ = bridge
            .commands
            .send(DebuggerCommand::FetchSource { url: url.clone() });
        self.status = format!("fetching {url}...").into();
        cx.notify();
    }
    fn on_connect(&mut self, _: &Connect, _: &mut Window, cx: &mut Context<Self>) {
        if let Some(bridge) = &self.bridge {
            let _ = bridge.commands.send(DebuggerCommand::Connect);
            self.status = "scanning Metro...".into();
        } else {
            self.status = "debugger bridge not initialised".into();
        }
        cx.notify();
    }
    fn on_disconnect(&mut self, _: &Disconnect, _: &mut Window, cx: &mut Context<Self>) {
        if let Some(bridge) = &self.bridge {
            let _ = bridge.commands.send(DebuggerCommand::Disconnect);
        }
        self.breakpoints.clear();
        self.paused = false;
        self.call_stack = None;
        self.properties.clear();
        self.expanded.clear();
        self.pending_props_tags.clear();
        self.network.clear();
        self.selected_request = None;
        self.components.clear();
        self.selected_node = None;
        self.metrics.clear();
        self.screenshot_path = None;
        self.screenshot_count = 0;
        cx.notify();
    }

    fn send_debug(&self, cmd: DebuggerCommand) {
        if let Some(bridge) = &self.bridge {
            let _ = bridge.commands.send(cmd);
        }
    }

    fn on_debug_resume(&mut self, _: &DebugResume, _: &mut Window, cx: &mut Context<Self>) {
        self.send_debug(DebuggerCommand::Resume);
        cx.notify();
    }
    fn on_debug_step_over(&mut self, _: &DebugStepOver, _: &mut Window, cx: &mut Context<Self>) {
        self.send_debug(DebuggerCommand::StepOver);
        cx.notify();
    }
    fn on_debug_step_into(&mut self, _: &DebugStepInto, _: &mut Window, cx: &mut Context<Self>) {
        self.send_debug(DebuggerCommand::StepInto);
        cx.notify();
    }
    fn on_debug_step_out(&mut self, _: &DebugStepOut, _: &mut Window, cx: &mut Context<Self>) {
        self.send_debug(DebuggerCommand::StepOut);
        cx.notify();
    }
    fn on_debug_pause(&mut self, _: &DebugPause, _: &mut Window, cx: &mut Context<Self>) {
        self.send_debug(DebuggerCommand::Pause);
        cx.notify();
    }
    fn on_watch_this(&mut self, _: &WatchThis, _: &mut Window, cx: &mut Context<Self>) {
        self.add_watch("this", cx);
    }
    fn on_watch_clear(&mut self, _: &WatchClear, _: &mut Window, cx: &mut Context<Self>) {
        let n = self.watches.len();
        for i in (0..n).rev() {
            self.watches.remove(i);
        }
        self.pending_watch_tags.clear();
        cx.notify();
    }
    fn show_dock(&mut self, pane: DockPane, cx: &mut Context<Self>) {
        self.dock = pane;
        cx.notify();
    }
    fn on_show_files(&mut self, _: &ShowFiles, _: &mut Window, cx: &mut Context<Self>) {
        self.show_dock(DockPane::Files, cx);
    }
    fn on_show_debugger(&mut self, _: &ShowDebugger, _: &mut Window, cx: &mut Context<Self>) {
        self.show_dock(DockPane::Debugger, cx);
    }
    fn on_show_simulator(&mut self, _: &ShowSimulator, _: &mut Window, cx: &mut Context<Self>) {
        self.show_dock(DockPane::Simulator, cx);
    }
    fn on_show_components(&mut self, _: &ShowComponents, _: &mut Window, cx: &mut Context<Self>) {
        self.show_dock(DockPane::Components, cx);
    }
    fn on_show_profiler(&mut self, _: &ShowProfiler, _: &mut Window, cx: &mut Context<Self>) {
        self.show_dock(DockPane::Profiler, cx);
    }
    fn on_show_console(&mut self, _: &ShowConsole, _: &mut Window, cx: &mut Context<Self>) {
        self.show_dock(DockPane::Console, cx);
    }
    fn on_show_network(&mut self, _: &ShowNetwork, _: &mut Window, cx: &mut Context<Self>) {
        self.show_dock(DockPane::Network, cx);
    }
    fn on_clear_network(&mut self, _: &ClearNetwork, _: &mut Window, cx: &mut Context<Self>) {
        self.network.clear();
        self.selected_request = None;
        cx.notify();
    }
    fn on_components_load_demo(
        &mut self,
        _: &ComponentsLoadDemo,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        // Until React DevTools backend protocol decoding lands, this seeds
        // a representative tree so users can see the Components pane loop.
        let snap = serde_json::json!([
            {
                "id": "1",
                "name": "App",
                "props": {"children": "<...>" },
                "state": {},
                "children": [
                    {
                        "id": "2",
                        "name": "NavigationContainer",
                        "props": {"theme": "dark"},
                        "children": [
                            {
                                "id": "3",
                                "name": "TabNavigator",
                                "children": [
                                    {"id": "4", "name": "HomeScreen", "props": {"route": "/home"}},
                                    {"id": "5", "name": "CartScreen", "props": {"route": "/cart"}, "state": {"count": 3}}
                                ]
                            }
                        ]
                    }
                ]
            }
        ]);
        self.components.seed_from_devtools_snapshot(&snap);
        self.dock = DockPane::Components;
        self.status = "loaded demo component tree".into();
        cx.notify();
    }
    fn on_components_clear(&mut self, _: &ComponentsClear, _: &mut Window, cx: &mut Context<Self>) {
        self.components.clear();
        self.selected_node = None;
        cx.notify();
    }
    fn on_profiler_refresh(&mut self, _: &ProfilerRefresh, _: &mut Window, cx: &mut Context<Self>) {
        self.send_debug(DebuggerCommand::GetPerformanceMetrics);
        self.dock = DockPane::Profiler;
        self.status = "fetching metrics...".into();
        cx.notify();
    }
    fn on_simulator_capture(
        &mut self,
        _: &SimulatorCapture,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.send_debug(DebuggerCommand::CaptureScreenshot);
        self.dock = DockPane::Simulator;
        self.status = "capturing screenshot...".into();
        cx.notify();
    }

    /// Toggle breakpoint at zero-based `line` for the currently displayed
    /// source. Sends the corresponding CDP command via the bridge.
    /// Set a conditional breakpoint or logpoint at the cursor's current line.
    /// `kind` selects which encoding to use; `expr` is the user's input.
    fn set_breakpoint_with_kind(&mut self, kind: BpKind, expr: &str, cx: &mut Context<Self>) {
        let Some(url) = self.current_url.clone() else {
            self.status = "no source loaded — cmd+shift+o first".into();
            cx.notify();
            return;
        };
        let (line, _col) = self.state.cursor_line_col();
        let line = line as u32;
        let condition = match kind {
            BpKind::Conditional => format!("({expr})"),
            // Logpoint: side-effect log + always-falsy so runtime never halts.
            BpKind::Logpoint => format!("console.log({expr}), false"),
        };
        // If a bp already exists on this line, drop it first so we replace
        // it with the conditioned variant.
        let existing = self
            .breakpoints
            .iter_for_url(&url)
            .find(|bp| bp.requested_line == line)
            .map(|bp| (bp.id, bp.server_id.clone()));
        if let Some((id, server_id)) = existing {
            self.breakpoints.remove(id);
            if let Some(sid) = server_id {
                self.send_debug(DebuggerCommand::RemoveBreakpoint { server_id: sid });
            }
        }
        let local = self
            .breakpoints
            .set(url.clone(), line, None, Some(condition.clone()));
        self.send_debug(DebuggerCommand::SetBreakpoint {
            local,
            url,
            line,
            column: None,
            condition: Some(condition),
        });
        self.status = match kind {
            BpKind::Conditional => format!("conditional breakpoint @ ln {}: {expr}", line + 1),
            BpKind::Logpoint => format!("logpoint @ ln {}: {expr}", line + 1),
        }
        .into();
        cx.notify();
    }

    fn toggle_breakpoint_at(&mut self, line: u32, cx: &mut Context<Self>) {
        use debugger::breakpoints::ToggleOutcome;
        let Some(url) = self.current_url.clone() else {
            self.status = "no source loaded — cmd+shift+o to fetch first script".into();
            cx.notify();
            return;
        };
        let outcome = self.breakpoints.toggle(&url, line);
        match outcome {
            ToggleOutcome::Added(local) => {
                self.send_debug(DebuggerCommand::SetBreakpoint {
                    local,
                    url: url.clone(),
                    line,
                    column: None,
                    condition: None,
                });
                self.status = format!("breakpoint set @ {url}:{line}").into();
            }
            ToggleOutcome::Removed { server_id, .. } => {
                if let Some(sid) = server_id {
                    self.send_debug(DebuggerCommand::RemoveBreakpoint { server_id: sid });
                }
                self.status = format!("breakpoint cleared @ {url}:{line}").into();
            }
        }
        cx.notify();
    }

    /// Add a watch expression. Triggers immediate eval if currently paused.
    fn add_watch(&mut self, expression: impl Into<String>, cx: &mut Context<Self>) {
        let idx = self.watches.add(expression);
        if self.paused {
            self.evaluate_watch_at(idx);
        }
        cx.notify();
    }

    /// Remove a watch expression by index. Currently only callable from
    /// future per-watch UI; kept here so the row-level remove handler can
    /// land in a follow-up without restructuring.
    #[allow(dead_code)]
    fn remove_watch(&mut self, idx: usize, cx: &mut Context<Self>) {
        self.watches.remove(idx);
        cx.notify();
    }

    /// Kick off inline-value evaluation for identifiers on the paused
    /// line in the currently displayed source. Results stream back via
    /// `InlineEvaluated` events and populate `inline_values`.
    fn evaluate_inline_for_paused_line(&mut self) {
        self.inline_values.clear();
        let Some(line) = self.paused_line_in_current() else {
            return;
        };
        let Some(stack) = self.call_stack.as_ref() else {
            return;
        };
        let Some(frame) = stack.frames.first() else {
            return;
        };
        let line_text = self.state.buffer.line_text(line as usize);
        let idents = extract_idents(&line_text);
        for ident in idents {
            self.send_debug(DebuggerCommand::EvaluateInline {
                ident,
                call_frame_id: frame.id.clone(),
            });
        }
    }

    /// Re-evaluate all watch expressions against the current top frame.
    fn evaluate_all_watches(&mut self) {
        let n = self.watches.len();
        for i in 0..n {
            self.evaluate_watch_at(i);
        }
    }

    /// Dispatch evaluation of one watch against the current top call frame.
    fn evaluate_watch_at(&mut self, idx: usize) {
        let Some(stack) = self.call_stack.as_ref() else {
            return;
        };
        let Some(frame) = stack.frames.first() else {
            return;
        };
        let Some(watch) = self.watches.get(idx) else {
            return;
        };
        let tag = self.next_watch_tag;
        self.next_watch_tag += 1;
        self.pending_watch_tags.insert(tag, idx);
        let cmd = DebuggerCommand::EvaluateWatch {
            tag,
            call_frame_id: frame.id.clone(),
            expression: watch.expression.clone(),
        };
        self.send_debug(cmd);
    }

    /// Toggle expanded state for a CDP `objectId`. Fetches properties on
    /// first expand; subsequent toggles flip visibility without re-fetch.
    fn toggle_object(&mut self, object_id: String, cx: &mut Context<Self>) {
        if self.expanded.contains(&object_id) {
            self.expanded.remove(&object_id);
        } else {
            self.expanded.insert(object_id.clone());
            if !self.properties.contains_key(&object_id) {
                let tag = self.next_props_tag;
                self.next_props_tag += 1;
                self.pending_props_tags.insert(tag, object_id.clone());
                self.send_debug(DebuggerCommand::GetProperties {
                    tag,
                    object_id,
                    own_properties: true,
                });
            }
        }
        cx.notify();
    }

    /// Zero-based line in the currently displayed source where the runtime
    /// is paused, if any. Resolves the top frame's `script_id` to its URL
    /// via the script registry and matches against `current_url`.
    fn paused_line_in_current(&self) -> Option<u32> {
        let frame = self.call_stack.as_ref()?.frames.first()?;
        let url = self.scripts.get(&frame.script_id).map(|s| s.url.clone())?;
        if Some(&url) == self.current_url.as_ref() {
            Some(frame.line)
        } else {
            None
        }
    }

    /// Drain pending events from the bridge and apply them to the window.
    fn drain_bridge_events(&mut self, cx: &mut Context<Self>) {
        // Collect events first so the immutable borrow on `self.bridge` is
        // dropped before we start mutating other fields (some handlers
        // dispatch back into the bridge, which needs `&self`).
        let events: Vec<DebuggerEvent> = match &self.bridge {
            Some(b) => b.events.try_iter().collect(),
            None => return,
        };
        let mut changed = false;
        for event in events {
            match event {
                DebuggerEvent::State(state) => {
                    self.status = match &state {
                        ConnectionState::Disconnected => "disconnected".into(),
                        ConnectionState::Scanning => "scanning Metro...".into(),
                        ConnectionState::Connecting { ws_url } => {
                            format!("connecting to {ws_url}").into()
                        }
                        ConnectionState::Connected { ws_url } => {
                            format!("connected: {ws_url}").into()
                        }
                        ConnectionState::Failed { reason } => {
                            format!("connect failed: {reason}").into()
                        }
                    };
                    self.connection = state;
                    changed = true;
                }
                DebuggerEvent::Log {
                    level,
                    message,
                    location,
                } => {
                    self.console.push(level, message, location);
                    changed = true;
                }
                DebuggerEvent::ScriptParsed(script) => {
                    self.scripts.upsert(script);
                    changed = true;
                }
                DebuggerEvent::SourceFetched { url, body } => {
                    match body {
                        Ok(text) => {
                            // Replace the editor buffer with the fetched
                            // source and lock it as read-only — Metro
                            // serves the canonical text the runtime
                            // executed, mutating it locally would diverge.
                            let mut buf = Buffer::default();
                            buf.insert(0, &text);
                            self.state.replace_buffer(buf);
                            self.state.set_read_only(true);
                            self.status =
                                format!("loaded {url} ({} chars)", text.chars().count()).into();
                            self.current_url = Some(url);
                        }
                        Err(e) => {
                            self.status = format!("fetch failed: {url}: {e}").into();
                        }
                    }
                    changed = true;
                }
                DebuggerEvent::BreakpointSet {
                    local,
                    server_id,
                    line,
                } => {
                    self.breakpoints.attach_server_id(local, &server_id);
                    if let Some(line) = line {
                        self.breakpoints.set_resolved_line(local, line);
                    }
                    self.status =
                        format!("breakpoint set: {} (server {})", server_id, server_id).into();
                    changed = true;
                }
                DebuggerEvent::BreakpointResolved { server_id, line } => {
                    if let Some(local_id) = self
                        .breakpoints
                        .find_by_server_id(&server_id)
                        .map(|bp| bp.id)
                    {
                        self.breakpoints.set_resolved_line(local_id, line);
                    }
                    changed = true;
                }
                DebuggerEvent::Paused { reason, call_stack } => {
                    self.paused = true;
                    self.call_stack = Some(call_stack);
                    self.status = format!("paused ({reason})").into();
                    // Auto-switch dock to Debugger pane on pause so users
                    // see the call stack + scopes immediately.
                    self.dock = DockPane::Debugger;
                    self.evaluate_all_watches();
                    self.evaluate_inline_for_paused_line();
                    changed = true;
                }
                DebuggerEvent::Resumed => {
                    self.paused = false;
                    self.call_stack = None;
                    self.properties.clear();
                    self.expanded.clear();
                    self.pending_props_tags.clear();
                    self.pending_watch_tags.clear();
                    self.inline_values.clear();
                    // Clear stale watch results; expressions persist.
                    for w in self.watches.iter_mut() {
                        w.last = None;
                    }
                    self.status = "resumed".into();
                    changed = true;
                }
                DebuggerEvent::Properties { tag, properties } => {
                    debug!(target: "atomio", tag, count = properties.len(), "properties");
                    if let Some(object_id) = self.pending_props_tags.remove(&tag) {
                        self.properties.insert(object_id, properties);
                    }
                    changed = true;
                }
                DebuggerEvent::WatchEvaluated { tag, result } => {
                    if let Some(idx) = self.pending_watch_tags.remove(&tag) {
                        if let Some(w) = self.watches.iter_mut().nth(idx) {
                            w.last = Some(result);
                        }
                    }
                    changed = true;
                }
                DebuggerEvent::InlineEvaluated { ident, result } => {
                    self.inline_values.insert(ident, result);
                    changed = true;
                }
                DebuggerEvent::PerformanceMetrics(metrics) => {
                    self.metrics = metrics;
                    changed = true;
                }
                DebuggerEvent::ScreenshotCaptured { data_base64 } => {
                    use base64::Engine;
                    match base64::engine::general_purpose::STANDARD.decode(data_base64) {
                        Ok(bytes) => {
                            self.screenshot_count += 1;
                            let path = screenshot_path_for(self.screenshot_count);
                            if let Some(parent) = path.parent() {
                                let _ = std::fs::create_dir_all(parent);
                            }
                            match std::fs::write(&path, &bytes) {
                                Ok(_) => {
                                    self.status = format!(
                                        "screenshot {} ({} bytes)",
                                        self.screenshot_count,
                                        bytes.len()
                                    )
                                    .into();
                                    self.screenshot_path = Some(path);
                                }
                                Err(e) => {
                                    self.status = format!("write screenshot: {e}").into();
                                }
                            }
                        }
                        Err(e) => {
                            self.status = format!("decode screenshot: {e}").into();
                        }
                    }
                    changed = true;
                }
                DebuggerEvent::NetworkEvent { method, params } => {
                    match method.as_str() {
                        "Network.requestWillBeSent" => {
                            self.network.on_request_will_be_sent(&params);
                        }
                        "Network.responseReceived" => {
                            self.network.on_response_received(&params);
                        }
                        "Network.loadingFinished" => {
                            self.network.on_loading_finished(&params);
                        }
                        "Network.loadingFailed" => {
                            self.network.on_loading_failed(&params);
                        }
                        "Network.webSocketFrameSent" => {
                            self.network.on_ws_frame(WsDirection::Sent, &params);
                        }
                        "Network.webSocketFrameReceived" => {
                            self.network.on_ws_frame(WsDirection::Received, &params);
                        }
                        _ => {}
                    }
                    changed = true;
                }
            }
        }
        if changed {
            cx.notify();
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

impl AtomioWindow {
    /// Activity bar (left rail). Per `docs/design.md`: 48px wide, 36x36
    /// items, accent left-edge indicator on active.
    fn render_activity_bar(&self) -> gpui::Div {
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
            bar = bar.child(
                div()
                    .w(px(36.0))
                    .h(px(36.0))
                    .flex()
                    .items_center()
                    .justify_center()
                    .rounded(px(6.0))
                    .text_xs()
                    .text_color(rgb(fg))
                    .bg(rgb(bg))
                    .child(pane.glyph()),
            );
        }
        bar
    }

    /// Editor pane (centre). Builds the per-line layout: clickable gutter
    /// (line number + breakpoint dot + paused arrow), then the syntax-coloured
    /// text row, with a paused-line full-row tint when applicable.
    fn render_editor_pane(
        &self,
        line_views: Vec<LineView>,
        gutter_width: gpui::Pixels,
        cx: &mut Context<Self>,
    ) -> gpui::Div {
        let mut pane = div()
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
            // Zero-based CDP line index for click + paused dispatch.
            let line_idx = (number - 1) as u32;

            // Gutter cell: line number + bp dot + paused arrow. Clickable
            // anywhere on the gutter: toggles the breakpoint via the
            // bridge.
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

            // Inline value pills on the paused line.
            if is_paused_here && !self.inline_values.is_empty() {
                for ident in extract_idents(&text) {
                    if let Some(result) = self.inline_values.get(&ident) {
                        let (display, color) = match result {
                            Ok(rv) => (rv.display.clone(), value_color_for(rv.type_str.as_str())),
                            Err(_) => continue, // skip out-of-scope idents
                        };
                        row = row.child(
                            div()
                                .ml_2()
                                .px_2()
                                .py(px(0.0))
                                .rounded(px(4.0))
                                .bg(rgb(theme::BG_4))
                                .text_xs()
                                .text_color(rgb(color))
                                .child(format!("{ident} = {display}")),
                        );
                    }
                }
            }

            let row_bg = if is_paused_here {
                theme::ACCENT_SOFT
            } else {
                theme::BG_1
            };
            pane = pane.child(
                div()
                    .flex()
                    .flex_row()
                    .bg(rgb(row_bg))
                    .child(gutter)
                    .child(row),
            );
        }
        pane
    }

    /// Right dock. Renders the active pane; non-functional ones display
    /// a placeholder pointing at the milestone where they'll land.
    fn render_dock(&self, cx: &mut Context<Self>) -> gpui::Div {
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
    /// the selected frame. Renders a placeholder when not paused. Variable
    /// expansion (lazy property fetch) ships in a follow-up PR.
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
            // Show only file basename for compactness.
            let basename = url.rsplit_once('/').map(|(_, b)| b).unwrap_or(url);
            let row_color = if i == 0 { theme::TX_1 } else { theme::TX_2 };
            frames_section = frames_section.child(
                div()
                    .flex()
                    .flex_row()
                    .justify_between()
                    .text_xs()
                    .text_color(rgb(row_color))
                    .py_1()
                    .child(div().child(name.to_string()))
                    .child(div().text_color(rgb(theme::TX_4)).child(format!(
                        "{}:{}",
                        basename,
                        f.line + 1
                    ))),
            );
        }

        // Scopes for the top frame.
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

        div()
            .flex_1()
            .flex()
            .flex_col()
            .child(frames_section)
            .child(scopes_section)
            .child(watch_section)
    }

    /// Watch expressions section. Header + each watch's last evaluated
    /// value (or "evaluating..." when pending, or red error when CDP raised).
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
    /// `indent` is in tree levels; depth-limit kicks in at 6 to avoid
    /// runaway recursion on cyclic objects (CDP doesn't loop, but caps
    /// keep the UI tame).
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

    /// Build one step-control button. Disabled-looking when `enabled` is
    /// false; click only fires the command when enabled.
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
        // Wrap Stateful<Div> in plain Div so callers can compose normally.
        div().child(inner)
    }

    /// Network pane body. Header + scrolling row list of requests; each
    /// row clickable to select. Selected request shows a small detail
    /// strip (status / mime / duration / encoded bytes).
    fn render_network_body(&self, cx: &mut Context<Self>) -> gpui::Div {
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

    /// Detail strip for the selected request -- full URL, headers,
    /// timing, WS frames if any.
    fn render_network_detail(&self, req: &network::NetworkRequest) -> gpui::Div {
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

    /// Console body (entries list). Used by [`render_dock`] when console
    /// pane is active.
    /// Simulator pane body. Capture button + phone-frame border that
    /// renders the most recent screenshot via gpui's image loader. Live
    /// streaming + device picker land in a follow-up PR.
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

        // Phone-frame container, sized roughly 300x600 device aspect.
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

    /// Profiler pane body. Refresh button + metric rows. Bar widths are
    /// proportional to the largest value in the snapshot for quick visual
    /// comparison; deeper sampling/flame graph lands in a follow-up PR.
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

    /// Components pane body: indented DFS render of the component tree
    /// + props/state strip for the selected node.
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

fn placeholder(label: impl Into<SharedString>) -> gpui::Div {
    div()
        .flex_1()
        .flex()
        .items_center()
        .justify_center()
        .text_xs()
        .text_color(rgb(theme::TX_4))
        .child(label.into())
}

impl Focusable for AtomioWindow {
    fn focus_handle(&self, _cx: &gpui::App) -> FocusHandle {
        self.focus_handle.clone()
    }
}

impl Render for AtomioWindow {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        // Drain any debugger events that arrived since the last render.
        // Combined with the polling Task spawned in `init`, this means new
        // entries appear within ~50ms of arriving from CDP.
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
        let connection_label: SharedString = match &self.connection {
            ConnectionState::Disconnected => "● disconnected".into(),
            ConnectionState::Scanning => "● scanning".into(),
            ConnectionState::Connecting { .. } => "● connecting".into(),
            ConnectionState::Connected { .. } => "● connected".into(),
            ConnectionState::Failed { .. } => "● failed".into(),
        };
        let connection_color: u32 = match &self.connection {
            ConnectionState::Connected { .. } => theme::ACCENT,
            ConnectionState::Connecting { .. } | ConnectionState::Scanning => theme::WARN,
            ConnectionState::Failed { .. } => theme::ERROR,
            ConnectionState::Disconnected => theme::TX_4,
        };

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
            // Suppress the candidate list when using a sigil prefix —
            // there are no matching commands and the rows would distract
            // from the literal expression being typed.
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
            // Custom titlebar (system bar is transparent; traffic lights
            // float at the configured position).
            .child(
                div()
                    .h(px(36.0))
                    .flex()
                    .items_center()
                    .justify_center()
                    .px_4()
                    .bg(rgb(theme::BG_2))
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
            // Main row: activity bar | editor | dock
            .child(
                div()
                    .flex_1()
                    .flex()
                    .flex_row()
                    .child(self.render_activity_bar())
                    .child(self.render_editor_pane(line_views, gutter_width, cx))
                    .child(self.render_dock(cx)),
            )
            // Footer / status line
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

/// Resolve the path where atomio's diagnostic log lives, creating parent
/// directories if needed. Mirrors the macOS convention
/// `~/Library/Logs/atomio/atomio.log`.
/// Path where the simulator pane writes captured PNG frames. Filename
/// includes the count so gpui's image cache reloads (it keys by path).
fn screenshot_path_for(seq: u64) -> PathBuf {
    let base = dirs::cache_dir()
        .map(|c| c.join("atomio").join("screenshots"))
        .unwrap_or_else(|| PathBuf::from("/tmp/atomio/screenshots"));
    base.join(format!("frame-{seq}.png"))
}

fn log_file_path() -> PathBuf {
    let base = dirs::home_dir()
        .map(|h| h.join("Library").join("Logs").join("atomio"))
        .unwrap_or_else(|| PathBuf::from("/tmp/atomio"));
    let _ = std::fs::create_dir_all(&base);
    base.join("atomio.log")
}

/// Initialise tracing subscribers. Logs go to stderr (visible when atomio
/// is launched from a terminal) and to a rotating file at
/// `~/Library/Logs/atomio/atomio.log`. Filter via `RUST_LOG`; defaults to
/// `info` for `atomio`/`debugger`, `warn` for everything else.
fn init_logging() -> tracing_appender::non_blocking::WorkerGuard {
    use tracing_subscriber::{fmt, prelude::*, EnvFilter};

    let log_path = log_file_path();
    let file = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(&log_path)
        .ok();

    let (file_writer, guard) = match file {
        Some(f) => tracing_appender::non_blocking(f),
        // If we can't open the file, fall back to a sink. The stderr
        // layer still works.
        None => tracing_appender::non_blocking(std::io::sink()),
    };

    let env_filter = EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| EnvFilter::new("warn,atomio=info,debugger=info,console=info"));

    let stderr_layer = fmt::layer().with_target(true).with_writer(std::io::stderr);
    let file_layer = fmt::layer()
        .with_target(true)
        .with_ansi(false)
        .with_writer(file_writer);

    tracing_subscriber::registry()
        .with(env_filter)
        .with(stderr_layer)
        .with(file_layer)
        .init();

    tracing::info!(
        target: "atomio",
        log_path = %log_path.display(),
        "atomio v{} starting",
        env!("CARGO_PKG_VERSION")
    );

    guard
}

fn main() {
    // Hold the guard for the lifetime of main so non-blocking writer flushes.
    let _log_guard = init_logging();

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
            KeyBinding::new("cmd-shift-d", Connect, Some("atomio")),
            KeyBinding::new("cmd-k", ClearConsole, Some("atomio")),
            KeyBinding::new("cmd-shift-o", OpenFirstScript, Some("atomio")),
            KeyBinding::new("f5", DebugResume, Some("atomio")),
            KeyBinding::new("f10", DebugStepOver, Some("atomio")),
            KeyBinding::new("f11", DebugStepInto, Some("atomio")),
            KeyBinding::new("shift-f11", DebugStepOut, Some("atomio")),
            KeyBinding::new("f6", DebugPause, Some("atomio")),
            KeyBinding::new("cmd-1", ShowFiles, Some("atomio")),
            KeyBinding::new("cmd-2", ShowDebugger, Some("atomio")),
            KeyBinding::new("cmd-3", ShowSimulator, Some("atomio")),
            KeyBinding::new("cmd-4", ShowComponents, Some("atomio")),
            KeyBinding::new("cmd-5", ShowProfiler, Some("atomio")),
            KeyBinding::new("cmd-6", ShowConsole, Some("atomio")),
            KeyBinding::new("cmd-7", ShowNetwork, Some("atomio")),
        ]);

        let bounds = Bounds::centered(None, size(px(1280.0), px(800.0)), cx);
        let window = cx
            .open_window(
                WindowOptions {
                    window_bounds: Some(WindowBounds::Windowed(bounds)),
                    titlebar: Some(TitlebarOptions {
                        title: Some("atomio".into()),
                        // Hide system titlebar so we draw our own 36px bar
                        // with traffic lights still positioned over it.
                        appears_transparent: true,
                        traffic_light_position: Some(point(px(14.0), px(12.0))),
                    }),
                    window_background: WindowBackgroundAppearance::Opaque,
                    ..Default::default()
                },
                |_window, cx| {
                    let state = EditorState::new(buffer.clone());
                    let commands = build_command_registry();
                    let bridge = cdp_bridge::spawn();
                    cx.new(|cx| AtomioWindow {
                        state,
                        commands,
                        status: "cmd+shift+p palette · cmd+shift+d connect · cmd+1..6 panes".into(),
                        focus_handle: cx.focus_handle(),
                        palette_query: None,
                        palette_selected: 0,
                        console: Console::new(),
                        connection: ConnectionState::Disconnected,
                        bridge: Some(bridge),
                        scripts: ScriptRegistry::new(),
                        breakpoints: BreakpointRegistry::new(),
                        paused: false,
                        call_stack: None,
                        properties: HashMap::new(),
                        expanded: HashSet::new(),
                        next_props_tag: 0,
                        pending_props_tags: HashMap::new(),
                        watches: WatchList::new(),
                        pending_watch_tags: HashMap::new(),
                        next_watch_tag: 0,
                        inline_values: HashMap::new(),
                        network: NetworkRegistry::new(),
                        selected_request: None,
                        components: ComponentTree::new(),
                        selected_node: None,
                        metrics: Vec::new(),
                        screenshot_path: None,
                        screenshot_count: 0,
                        dock: DockPane::Console,
                        current_url: None,
                    })
                },
            )
            .expect("failed to open atomio window");

        window
            .update(cx, |view, window, cx| {
                window.focus(&view.focus_handle);
                // Spawn a low-frequency tick task that nudges the window
                // to drain the bridge channels and re-render. The render
                // method itself does the drain; this just keeps the wheel
                // turning when no other event is firing.
                cx.spawn(async move |this, cx| loop {
                    cx.background_executor()
                        .timer(Duration::from_millis(100))
                        .await;
                    if this.update(cx, |_, cx| cx.notify()).is_err() {
                        break;
                    }
                })
                .detach();
            })
            .ok();

        cx.activate(true);
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn extract_idents_basic() {
        let v = extract_idents("const total = items.reduce((a, b) => a + b.price, 0);");
        assert!(v.contains(&"total".to_string()));
        assert!(v.contains(&"items".to_string()));
        assert!(!v.contains(&"const".to_string()));
        assert!(!v.contains(&"a".to_string())); // single-char
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
