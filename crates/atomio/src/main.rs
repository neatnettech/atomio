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

mod assets;
mod cdp_bridge;
mod render;
mod theme;

use render::{extract_idents, spans_to_line_runs, LineView};

use std::collections::{HashMap, HashSet};
use std::path::PathBuf;
use std::time::Duration;

use cdp_bridge::{ConnectionState, DebuggerBridge, DebuggerCommand, DebuggerEvent};
use console::Console;
use debugger::breakpoints::BreakpointRegistry;
use debugger::scripts::ScriptRegistry;
use editor_core::{Buffer, CommandRegistry, EditorState};
use gpui::{
    actions, point, prelude::*, px, size, Application, Bounds, ClipboardItem, Context, FocusHandle,
    KeyBinding, KeyDownEvent, SharedString, TitlebarOptions, Window, WindowBackgroundAppearance,
    WindowBounds, WindowOptions,
};
use inspector::{CallStack, Property, WatchList};
use language::{highlight, HighlightKind, Language};
use network::{NetworkRegistry, WsDirection};
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

    /// Asset path to the lucide-style SVG icon for this pane. Resolved
    /// by [`crate::assets::EmbeddedAssets`] at render time.
    fn icon_path(self) -> &'static str {
        match self {
            DockPane::Files => "icons/files.svg",
            DockPane::Debugger => "icons/debugger.svg",
            DockPane::Simulator => "icons/simulator.svg",
            DockPane::Components => "icons/components.svg",
            DockPane::Profiler => "icons/profiler.svg",
            DockPane::Console => "icons/console.svg",
            DockPane::Network => "icons/network.svg",
        }
    }
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
    /// Reusable scratch buffer for `drain_bridge_events`. Carried across
    /// renders to amortise allocation when the bridge produces a steady
    /// stream of events (console logs / network ticks). Empty between
    /// drains; capacity grows to the high-water mark for the session.
    event_buf: Vec<DebuggerEvent>,
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
        trim_screenshots(true);
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
        // Reuse the per-window scratch buffer instead of allocating a new
        // Vec per render. `mem::take` swaps in an empty Vec (no alloc)
        // while we hold the borrow on `self.bridge`; we put the buffer
        // back at the end with its capacity preserved.
        let mut buf = std::mem::take(&mut self.event_buf);
        buf.clear();
        match &self.bridge {
            Some(b) => buf.extend(b.events.try_iter()),
            None => {
                self.event_buf = buf;
                return;
            }
        }
        if buf.is_empty() {
            self.event_buf = buf;
            return;
        }
        let mut changed = false;
        for event in buf.drain(..) {
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
                                    trim_screenshots(false);
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
        // Restore the (now-empty, capacity-retained) buffer for next drain.
        self.event_buf = buf;
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
/// Directory where simulator screenshots are cached.
fn screenshot_dir() -> PathBuf {
    dirs::cache_dir()
        .map(|c| c.join("atomio").join("screenshots"))
        .unwrap_or_else(|| PathBuf::from("/tmp/atomio/screenshots"))
}

/// Path where the simulator pane writes captured PNG frames. Filename
/// includes the count so gpui's image cache reloads (it keys by path).
fn screenshot_path_for(seq: u64) -> PathBuf {
    screenshot_dir().join(format!("frame-{seq}.png"))
}

/// Maximum captured frames kept on disk before older ones are evicted.
const SCREENSHOT_CAP: usize = 50;

/// Delete the oldest captured frames so at most [`SCREENSHOT_CAP`] remain.
/// Called after each capture and on disconnect (with `wipe = true`).
fn trim_screenshots(wipe: bool) {
    let dir = screenshot_dir();
    let Ok(entries) = std::fs::read_dir(&dir) else {
        return;
    };
    let mut files: Vec<(std::path::PathBuf, std::time::SystemTime)> = entries
        .filter_map(|e| e.ok())
        .filter_map(|e| {
            let m = e.metadata().ok()?;
            let t = m.modified().ok()?;
            Some((e.path(), t))
        })
        .collect();
    if wipe {
        for (p, _) in &files {
            let _ = std::fs::remove_file(p);
        }
        return;
    }
    if files.len() <= SCREENSHOT_CAP {
        return;
    }
    files.sort_by_key(|(_, t)| *t);
    let drop_count = files.len() - SCREENSHOT_CAP;
    for (p, _) in files.iter().take(drop_count) {
        let _ = std::fs::remove_file(p);
    }
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

    Application::new()
        .with_assets(assets::EmbeddedAssets)
        .run(move |cx| {
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
                            status: "cmd+shift+p palette · cmd+shift+d connect · cmd+1..6 panes"
                                .into(),
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
                            event_buf: Vec::new(),
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
