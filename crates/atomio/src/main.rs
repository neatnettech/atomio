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
use std::path::{Path, PathBuf};
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
use workspace::{AppState, Recents, Watcher as WorkspaceWatcher, Workspace};

actions!(
    atomio,
    [
        OpenFile,
        OpenProject,
        SaveFile,
        RevertToSaved,
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
        ShowTerminal,
        TerminalNew,
        TerminalKill,
        TerminalExpoStart,
        TerminalExpoPrebuild,
        TerminalNpmInstall,
        TerminalNextTab,
        TerminalPrevTab,
    ]
);

/// Breakpoint flavour for the conditional / logpoint palette flow.
#[derive(Debug, Clone, Copy)]
enum BpKind {
    Conditional,
    Logpoint,
}

/// One terminal tab. Pairs the PTY session with a user-visible title
/// used in the tab strip. Title is chosen at spawn time by the
/// handler (shell basename, "expo start", etc.) and doesn't update
/// from runtime output -- that's a future polish.
pub(crate) struct TerminalTab {
    pub(crate) title: String,
    pub(crate) pty: terminal::Terminal,
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
    Terminal,
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
            DockPane::Terminal => "Terminal",
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
            DockPane::Terminal => "icons/terminal.svg",
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
    /// Currently opened project workspace, if any. `None` when atomio
    /// launched without a project (single-file mode) or before the user
    /// picks one via `OpenProject`.
    workspace: Option<Workspace>,
    /// Recently opened projects. Persisted to disk in a follow-up PR;
    /// for now lives in memory for the duration of the session.
    recents: Recents,
    /// Project-tree directory paths the user has expanded. Tree paths
    /// are relative to `workspace.root()`. Empty on first open so the
    /// tree shows only top-level entries until the user clicks.
    expanded_dirs: HashSet<PathBuf>,
    /// Filesystem watcher tied to the current workspace root. `None`
    /// when no project is open or when watcher spawn failed (rare;
    /// usually means the root vanished between picker + watch). The
    /// render loop drains it each frame and triggers `Workspace::refresh`
    /// when ticks arrive.
    fs_watcher: Option<WorkspaceWatcher>,
    /// Embedded terminal sessions, rendered as a tab strip when the
    /// Terminal dock is active. Empty until `TerminalNew` /
    /// `TerminalExpoStart` spawns one. Dropping a tab closes its PTY
    /// and lets the reader thread exit.
    terminals: Vec<TerminalTab>,
    /// Index into `terminals` for the currently visible / focused tab.
    /// Meaningful only when `terminals` is non-empty; mutations to
    /// `terminals` keep this in bounds (`terminal_select` /
    /// `kill_active_terminal`).
    active_terminal: usize,
    /// `(url, line)` requested by a frame click while the buffer wasn't
    /// loaded yet. Applied once the matching [`DebuggerEvent::SourceFetched`]
    /// arrives, then cleared. Only the most recent click survives.
    pending_jump: Option<(String, u32)>,
    /// Reusable scratch buffer for `drain_bridge_events`. Carried across
    /// renders to amortise allocation when the bridge produces a steady
    /// stream of events (console logs / network ticks). Empty between
    /// drains; capacity grows to the high-water mark for the session.
    event_buf: Vec<DebuggerEvent>,
    /// Filesystem mtime of the currently-open buffer's backing file at
    /// the moment it was last loaded or saved. Used by the workspace
    /// tick handler to detect external edits without reading the file
    /// contents on every tick. `None` for unsaved scratch buffers and
    /// Metro-served sources.
    last_loaded_mtime: Option<std::time::SystemTime>,
    /// Set to true once any terminal tab's output contains a Metro
    /// start-up banner. Surfaces a "Metro detected -- Connect" pill
    /// in the status bar so the user doesn't have to remember the
    /// keybinding. Cleared on connect / disconnect so the pill is
    /// transient -- it shows up exactly when it adds value.
    metro_detected: bool,
}

/// Substrings the terminal scanner looks for in PTY output to decide
/// Metro is running. Expo CLI prints "Metro waiting on" (and a URL);
/// plain Metro prints "Welcome to Metro" at start-up. Both are
/// printable ASCII so they survive ANSI styling in the grid cells.
const METRO_BANNERS: &[&str] = &["Metro waiting", "Welcome to Metro"];

fn build_command_registry() -> CommandRegistry {
    let mut reg = CommandRegistry::new();
    reg.register("File: Open", "open_file");
    reg.register("File: Open Project…", "open_project");
    reg.register("File: Save", "save_file");
    reg.register("File: Revert to Saved", "revert_to_saved");
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
    reg.register("View: Terminal Pane", "show_terminal");
    reg.register("Terminal: New", "terminal_new");
    reg.register("Terminal: Kill Active Tab", "terminal_kill");
    reg.register("Terminal: expo start", "terminal_expo_start");
    reg.register("Terminal: expo prebuild", "terminal_expo_prebuild");
    reg.register("Terminal: npm install", "terminal_npm_install");
    reg.register("Terminal: Next Tab", "terminal_next_tab");
    reg.register("Terminal: Previous Tab", "terminal_prev_tab");
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
        let dirty = if self.state.buffer.is_dirty() {
            " •"
        } else {
            ""
        };
        // When a project is open, prefer "project · relative/path" so
        // the titlebar conveys both the workspace and the open file.
        // Falls back to the absolute path or the default tagline when
        // no project / no buffer path is set.
        if let Some(ws) = self.workspace.as_ref() {
            let name = ws.display_name();
            let rel = self
                .state
                .buffer
                .path()
                .and_then(|p| p.strip_prefix(ws.root()).ok())
                .map(|p| p.display().to_string());
            return match rel {
                Some(r) if !r.is_empty() => format!("{name} · {r}{dirty}").into(),
                _ => format!("{name}{dirty}").into(),
            };
        }
        match self.state.buffer.path() {
            Some(p) => format!("{}{dirty}", p.display()).into(),
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
                        this.record_loaded_mtime(&path);
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

    /// Best starting directory for the project picker. Prefers the
    /// parent of the currently-open project (so siblings are one click
    /// away), then the parent of the newest recent, then `$HOME`. Falls
    /// back to `/` when home is unknown -- the native picker handles
    /// that gracefully.
    fn picker_start_dir(&self) -> PathBuf {
        if let Some(ws) = self.workspace.as_ref() {
            if let Some(parent) = ws.root().parent() {
                return parent.to_path_buf();
            }
        }
        if let Some(recent) = self.recents.entries().first() {
            if let Some(parent) = recent.path.parent() {
                if parent.exists() {
                    return parent.to_path_buf();
                }
            }
        }
        dirs::home_dir().unwrap_or_else(|| PathBuf::from("/"))
    }

    /// Open a project directory via the native picker. Spawned on the
    /// gpui run loop per the rfd safety rule. On success: replaces
    /// `self.workspace`, bumps the recents list, updates the status
    /// bar, and switches the dock to the file tree pane so the user
    /// lands somewhere visibly different.
    fn on_open_project(&mut self, _: &OpenProject, _window: &mut Window, cx: &mut Context<Self>) {
        self.status = "opening project…".into();
        let start_dir = self.picker_start_dir();
        cx.spawn(async move |this, cx| {
            let picked = rfd::AsyncFileDialog::new()
                .set_title("Open project")
                .set_directory(&start_dir)
                .pick_folder()
                .await;
            let Some(folder) = picked else {
                let _ = this.update(cx, |this, cx| {
                    this.status = "open project cancelled".into();
                    cx.notify();
                });
                return;
            };
            let root = folder.path().to_path_buf();
            let result = Workspace::open(&root);
            let _ = this.update(cx, |this, cx| {
                match result {
                    Ok(ws) => {
                        let name = ws.display_name();
                        let kind = ws.kind();
                        let is_monorepo = ws.manifest().is_monorepo;
                        let file_count = ws.files().len();
                        let root = ws.root().to_path_buf();
                        this.recents.push(root.clone(), name.clone());
                        this.workspace = Some(ws);
                        this.expanded_dirs.clear();
                        this.dock = DockPane::Files;
                        this.reseed_recents_in_palette();
                        // Spawn a recursive notify-debouncer watcher on
                        // the new root. If it fails (e.g. root vanished
                        // between picker + watch) we keep the workspace
                        // open and surface the error in the status bar;
                        // the user can still navigate / open files.
                        this.fs_watcher = match WorkspaceWatcher::spawn(&root) {
                            Ok(w) => Some(w),
                            Err(e) => {
                                tracing::warn!(
                                    target: "atomio",
                                    error = %e,
                                    "failed to spawn fs watcher"
                                );
                                None
                            }
                        };
                        let monorepo = if is_monorepo { " monorepo" } else { "" };
                        this.status =
                            format!("opened {name} ({kind:?}{monorepo}, {file_count} files)")
                                .into();
                        this.persist_state();
                    }
                    Err(e) => {
                        this.status = format!("open project failed: {e}").into();
                    }
                }
                cx.notify();
            });
        })
        .detach();
    }

    /// Snapshot current persistable state to disk. Called after every
    /// project open / recents mutation. Best-effort: write failures
    /// log via tracing and surface in the status bar but never panic
    /// or abort the action that triggered the save.
    pub(crate) fn persist_state(&self) {
        let path = state_file_path();
        let last_project = self.workspace.as_ref().map(|w| w.root().to_path_buf());
        let state = AppState {
            version: AppState::VERSION,
            recents: self.recents.clone(),
            last_project,
        };
        if let Err(e) = state.save(&path) {
            tracing::warn!(target: "atomio", error = %e, path = %path.display(), "state save failed");
        }
    }

    /// Reopen recent entry at `idx` (newest is 0). Bumps it to the
    /// front of the recents + persists.
    pub(crate) fn open_recent(&mut self, idx: usize, cx: &mut Context<Self>) {
        let Some(entry) = self.recents.entries().get(idx).cloned() else {
            self.status = format!("recent #{idx} not found").into();
            cx.notify();
            return;
        };
        let root = entry.path.clone();
        match Workspace::open(&root) {
            Ok(ws) => {
                let name = ws.display_name();
                let kind = ws.kind();
                let is_monorepo = ws.manifest().is_monorepo;
                let file_count = ws.files().len();
                self.recents.push(root.clone(), name.clone());
                self.workspace = Some(ws);
                self.expanded_dirs.clear();
                self.dock = DockPane::Files;
                self.fs_watcher = match WorkspaceWatcher::spawn(&root) {
                    Ok(w) => Some(w),
                    Err(e) => {
                        tracing::warn!(target: "atomio", error = %e, "fs watcher spawn failed");
                        None
                    }
                };
                let monorepo = if is_monorepo { " monorepo" } else { "" };
                self.status =
                    format!("opened {name} ({kind:?}{monorepo}, {file_count} files)").into();
                self.reseed_recents_in_palette();
                self.persist_state();
            }
            Err(e) => {
                self.status = format!("open failed: {e} — pruning from recents").into();
                self.recents.forget(&root);
                self.reseed_recents_in_palette();
                self.persist_state();
            }
        }
        cx.notify();
    }

    /// Rebuild the `File: Recent #N -- NAME` palette entries from
    /// the current recents list. Drops every previous `open_recent:`
    /// entry first so the palette never accumulates stale rows.
    pub(crate) fn reseed_recents_in_palette(&mut self) {
        self.commands.unregister_prefix("open_recent:");
        for (i, entry) in self.recents.entries().iter().enumerate() {
            let label = format!("File: Recent #{i} -- {}", entry.name);
            let id = format!("open_recent:{i}");
            self.commands.register(label, id);
        }
    }

    /// Toggle expand/collapse state for a tree directory. Path is
    /// workspace-relative so toggling survives a `refresh()` that
    /// re-allocates entries.
    pub(crate) fn toggle_dir(&mut self, rel: PathBuf, cx: &mut Context<Self>) {
        if !self.expanded_dirs.remove(&rel) {
            self.expanded_dirs.insert(rel);
        }
        cx.notify();
    }

    /// Open a file from the tree pane. Resolves `rel` against the
    /// workspace root (rejecting `..` traversal), reads it via
    /// `Buffer::open`, and replaces the editor buffer. Clears
    /// `current_url` so paused-line / breakpoint matching doesn't
    /// fire against a CDP URL that no longer corresponds to what's
    /// on screen. The buffer opens writable.
    pub(crate) fn open_workspace_file(&mut self, rel: PathBuf, cx: &mut Context<Self>) {
        let Some(ws) = self.workspace.as_ref() else {
            self.status = "no project open".into();
            cx.notify();
            return;
        };
        let Some(abs) = ws.absolute(&rel) else {
            self.status = format!("rejecting path traversal: {}", rel.display()).into();
            cx.notify();
            return;
        };
        match Buffer::open(&abs) {
            Ok(buf) => {
                self.state.replace_buffer(buf);
                self.state.set_read_only(false);
                self.current_url = None;
                self.record_loaded_mtime(&abs);
                self.status = format!("opened {}", rel.display()).into();
            }
            Err(e) => {
                self.status = format!("open failed: {e}").into();
            }
        }
        cx.notify();
    }

    fn on_revert_to_saved(
        &mut self,
        _: &RevertToSaved,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        // No-op when the buffer is unsaved scratch or a Metro-served
        // source (no on-disk file to revert to). The status nudge tells
        // the user why nothing happened so the keystroke doesn't feel
        // dead.
        let Some(path) = self.state.buffer.path().map(|p| p.to_path_buf()) else {
            self.status = "no file to revert".into();
            cx.notify();
            return;
        };
        match Buffer::open(&path) {
            Ok(buf) => {
                self.state.reload_buffer(buf);
                self.record_loaded_mtime(&path);
                self.status = format!("reverted {}", path.display()).into();
            }
            Err(e) => {
                self.status = format!("revert failed: {e}").into();
            }
        }
        cx.notify();
    }

    /// Snapshot the filesystem mtime of `path` into
    /// `last_loaded_mtime`. Used as the baseline that
    /// [`check_for_external_buffer_edit`] compares against on each
    /// workspace tick. Failing to read mtime is silent -- the worst
    /// case is one missed auto-reload, which the user can always
    /// fix with the revert command.
    fn record_loaded_mtime(&mut self, path: &Path) {
        self.last_loaded_mtime = std::fs::metadata(path).and_then(|m| m.modified()).ok();
    }

    /// Detect that the currently-open buffer's backing file changed
    /// on disk since we last loaded or saved it. Clean buffers
    /// auto-reload in place with cursor preserved via
    /// [`EditorState::reload_buffer`]. Dirty buffers are left alone
    /// with a one-time status nudge so the user doesn't silently
    /// lose edits; the new mtime is stashed so we don't nag every
    /// tick for the same change.
    ///
    /// No-op for read-only buffers (Metro sources, log file) and
    /// for unsaved scratch buffers. Files outside the workspace
    /// root won't trigger this path at all -- the watcher only
    /// covers the root subtree -- but that's an acceptable v1
    /// limitation; revert-to-saved still works.
    fn check_for_external_buffer_edit(&mut self, cx: &mut Context<Self>) {
        if self.state.is_read_only() {
            return;
        }
        let Some(path) = self.state.buffer.path().map(|p| p.to_path_buf()) else {
            return;
        };
        let Ok(meta) = std::fs::metadata(&path) else {
            return;
        };
        let Ok(disk_mtime) = meta.modified() else {
            return;
        };
        if Some(disk_mtime) == self.last_loaded_mtime {
            return;
        }
        if self.state.buffer.is_dirty() {
            self.status = format!("external edit on {} (kept your edits)", path.display()).into();
            self.last_loaded_mtime = Some(disk_mtime);
            cx.notify();
            return;
        }
        match Buffer::open(&path) {
            Ok(buf) => {
                self.state.reload_buffer(buf);
                self.last_loaded_mtime = Some(disk_mtime);
                self.status = format!("reloaded {}", path.display()).into();
            }
            Err(e) => {
                self.status = format!("reload failed: {e}").into();
            }
        }
        cx.notify();
    }

    fn on_save(&mut self, _: &SaveFile, _window: &mut Window, cx: &mut Context<Self>) {
        if let Some(path) = self.state.buffer.path().map(|p| p.to_path_buf()) {
            self.status = match self.state.buffer.save() {
                Ok(()) => {
                    self.record_loaded_mtime(&path);
                    "saved".into()
                }
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
                    Ok(()) => {
                        this.record_loaded_mtime(&path);
                        format!("saved to {}", path.display()).into()
                    }
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
            "open_project" => self.on_open_project(&OpenProject, window, cx),
            "save_file" => self.on_save(&SaveFile, window, cx),
            "revert_to_saved" => self.on_revert_to_saved(&RevertToSaved, window, cx),
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
            "show_terminal" => self.on_show_terminal(&ShowTerminal, window, cx),
            "terminal_new" => self.on_terminal_new(&TerminalNew, window, cx),
            "terminal_kill" => self.on_terminal_kill(&TerminalKill, window, cx),
            "terminal_expo_start" => self.on_terminal_expo_start(&TerminalExpoStart, window, cx),
            "terminal_expo_prebuild" => {
                self.on_terminal_expo_prebuild(&TerminalExpoPrebuild, window, cx)
            }
            "terminal_npm_install" => self.on_terminal_npm_install(&TerminalNpmInstall, window, cx),
            "terminal_next_tab" => self.on_terminal_next_tab(&TerminalNextTab, window, cx),
            "terminal_prev_tab" => self.on_terminal_prev_tab(&TerminalPrevTab, window, cx),
            other if other.starts_with("open_recent:") => {
                let idx: Option<usize> = other
                    .strip_prefix("open_recent:")
                    .and_then(|s| s.parse().ok());
                if let Some(i) = idx {
                    self.open_recent(i, cx);
                }
            }
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
    /// Jump the editor to a specific call-frame location. Resolves the
    /// frame's `script_id` to a CDP URL via the script registry then
    /// delegates to [`Self::open_source_at`].
    pub(crate) fn jump_to_frame(&mut self, script_id: String, line: u32, cx: &mut Context<Self>) {
        let Some(script) = self.scripts.get(&script_id) else {
            self.status = format!("frame script {script_id} unknown to registry").into();
            cx.notify();
            return;
        };
        let url = script.url.clone();
        self.open_source_at(url, line, cx);
    }

    /// Open `url` in the editor with the caret on `line`. If the script
    /// is already the current buffer, moves the caret immediately;
    /// otherwise kicks off a `FetchSource` and parks a pending jump so
    /// the caret lands when the source arrives. Shared by frame clicks
    /// and breakpoint-sidebar clicks.
    pub(crate) fn open_source_at(&mut self, url: String, line: u32, cx: &mut Context<Self>) {
        if self.current_url.as_deref() == Some(url.as_str()) {
            self.state.move_cursor_to(line as usize, 0);
        } else if let Some(bridge) = &self.bridge {
            self.pending_jump = Some((url.clone(), line));
            let _ = bridge
                .commands
                .send(DebuggerCommand::FetchSource { url: url.clone() });
            self.status = format!("opening {url}:{}", line + 1).into();
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
        self.metro_detected = false;
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
    fn on_show_terminal(&mut self, _: &ShowTerminal, _: &mut Window, cx: &mut Context<Self>) {
        self.show_dock(DockPane::Terminal, cx);
    }

    /// Default cwd for new terminals: current workspace root, else
    /// $HOME, else `/`. Avoids dropping the user in `/` when launching
    /// atomio from Finder (which inherits `/` as cwd).
    fn terminal_cwd(&self) -> PathBuf {
        if let Some(ws) = self.workspace.as_ref() {
            return ws.root().to_path_buf();
        }
        dirs::home_dir().unwrap_or_else(|| PathBuf::from("/"))
    }

    /// Default shell argv: `$SHELL -l` falling back to `/bin/zsh -l`.
    /// `-l` so login dotfiles (PATH from `~/.zprofile` etc.) are loaded.
    fn default_shell_argv() -> Vec<String> {
        let shell = std::env::var("SHELL").unwrap_or_else(|_| "/bin/zsh".into());
        vec![shell, "-l".into()]
    }

    /// Spawn a new terminal tab, append it to `terminals`, and make
    /// it the active tab. Returns the spawn error so callers can
    /// surface it in the status bar.
    fn spawn_terminal_tab(
        &mut self,
        argv: &[&str],
        cwd: &Path,
        title: impl Into<String>,
    ) -> std::io::Result<()> {
        let pty = terminal::Terminal::spawn(argv, cwd, 100, 30)?;
        self.terminals.push(TerminalTab {
            title: title.into(),
            pty,
        });
        self.active_terminal = self.terminals.len() - 1;
        self.dock = DockPane::Terminal;
        Ok(())
    }

    /// Read-only access to the currently active terminal tab, if any.
    pub(crate) fn active_terminal(&self) -> Option<&TerminalTab> {
        self.terminals.get(self.active_terminal)
    }

    /// Mutable access to the currently active terminal tab, if any.
    /// Used by `forward_key_to_terminal` and the resize handler.
    pub(crate) fn active_terminal_mut(&mut self) -> Option<&mut TerminalTab> {
        self.terminals.get_mut(self.active_terminal)
    }

    /// Drop the active tab. Clamps `active_terminal` to a valid index
    /// (or 0 when the Vec becomes empty -- which is fine because the
    /// next access returns `None` via `Vec::get`).
    fn kill_active_terminal(&mut self) -> Option<TerminalTab> {
        if self.terminals.is_empty() {
            return None;
        }
        let idx = self.active_terminal.min(self.terminals.len() - 1);
        let tab = self.terminals.remove(idx);
        if self.terminals.is_empty() {
            self.active_terminal = 0;
        } else if self.active_terminal >= self.terminals.len() {
            self.active_terminal = self.terminals.len() - 1;
        }
        Some(tab)
    }

    fn on_terminal_new(&mut self, _: &TerminalNew, _: &mut Window, cx: &mut Context<Self>) {
        let cwd = self.terminal_cwd();
        let argv_owned = Self::default_shell_argv();
        let argv: Vec<&str> = argv_owned.iter().map(|s| s.as_str()).collect();
        // Title is the shell's basename ("zsh", "bash") so the tab
        // strip stays readable. Fall back to the full argv[0] when
        // the path has no terminal component.
        let title = std::path::Path::new(&argv_owned[0])
            .file_name()
            .and_then(|s| s.to_str())
            .unwrap_or(&argv_owned[0])
            .to_string();
        match self.spawn_terminal_tab(&argv, &cwd, title) {
            Ok(()) => {
                self.status = format!("terminal: {} in {}", argv_owned[0], cwd.display()).into();
            }
            Err(e) => {
                self.status = format!("terminal spawn failed: {e}").into();
            }
        }
        cx.notify();
    }

    fn on_terminal_kill(&mut self, _: &TerminalKill, _: &mut Window, cx: &mut Context<Self>) {
        if let Some(tab) = self.kill_active_terminal() {
            self.status = format!("closed: {}", tab.title).into();
        } else {
            self.status = "no terminal to close".into();
        }
        cx.notify();
    }

    /// Spawn a new terminal tab running `/bin/sh -lc <cmd>` in the
    /// project cwd. The command doubles as the tab title and the
    /// "running: ..." status. Used by every quick-spawn palette
    /// entry (`expo start`, `expo prebuild`, `npm install`, ...).
    fn spawn_shell_quick(&mut self, cmd: &str, cx: &mut Context<Self>) {
        let cwd = self.terminal_cwd();
        let argv = ["/bin/sh", "-lc", cmd];
        match self.spawn_terminal_tab(&argv, &cwd, cmd) {
            Ok(()) => {
                self.status = format!("running: {cmd}").into();
            }
            Err(e) => {
                self.status = format!("terminal spawn failed: {e}").into();
            }
        }
        cx.notify();
    }

    /// Spawn a terminal seeded with `expo start`. Picks the run
    /// command from the workspace kind so generic projects fall back
    /// to `npm run dev` instead of guessing wrong.
    fn on_terminal_expo_start(
        &mut self,
        _: &TerminalExpoStart,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let cmd = match self.workspace.as_ref().map(|w| w.kind()) {
            Some(workspace::ProjectKind::Expo) | None => "npx expo start",
            Some(workspace::ProjectKind::ReactNative) => "npx react-native start",
            Some(_) => "npm run dev",
        };
        self.spawn_shell_quick(cmd, cx);
    }

    /// Spawn a terminal running `npx expo prebuild`. No
    /// workspace-kind branching -- if the project isn't an Expo app
    /// the command surfaces its own error in the terminal, which is
    /// the right place for that signal to land.
    fn on_terminal_expo_prebuild(
        &mut self,
        _: &TerminalExpoPrebuild,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.spawn_shell_quick("npx expo prebuild", cx);
    }

    /// Spawn a terminal running `npm install` in the project cwd.
    /// Works for any Node project regardless of detected workspace
    /// kind; the lockfile (npm / yarn / pnpm) is the user's problem.
    fn on_terminal_npm_install(
        &mut self,
        _: &TerminalNpmInstall,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.spawn_shell_quick("npm install", cx);
    }

    /// Cycle to the next terminal tab, wrapping at the end. No-op
    /// when there are 0 or 1 tabs.
    pub(crate) fn on_terminal_next_tab(
        &mut self,
        _: &TerminalNextTab,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.terminals.len() <= 1 {
            return;
        }
        self.active_terminal = (self.active_terminal + 1) % self.terminals.len();
        cx.notify();
    }

    /// Cycle to the previous terminal tab, wrapping at the start.
    /// Mirrors `on_terminal_next_tab`.
    pub(crate) fn on_terminal_prev_tab(
        &mut self,
        _: &TerminalPrevTab,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.terminals.len() <= 1 {
            return;
        }
        self.active_terminal = if self.active_terminal == 0 {
            self.terminals.len() - 1
        } else {
            self.active_terminal - 1
        };
        cx.notify();
    }

    /// Switch to the terminal tab at `idx`. Out-of-range indexes
    /// are ignored so a stale click handler can't crash the window.
    pub(crate) fn select_terminal_tab(&mut self, idx: usize, cx: &mut Context<Self>) {
        if idx < self.terminals.len() && idx != self.active_terminal {
            self.active_terminal = idx;
            cx.notify();
        }
    }

    /// Drain pending PTY dirty pings on the active tab. Called once
    /// per render frame. Other tabs may have pending output too --
    /// they'll repaint when the user switches to them, which is fine
    /// because they're not on screen.
    ///
    /// Also runs the Metro detector once per frame on every tab
    /// (not just the active one) -- the user may have spawned
    /// `expo start` in tab A, switched to tab B to edit, and we
    /// still want the connect prompt to appear. Detection is a
    /// short string scan over the visible viewport; the cost is
    /// negligible compared to render.
    pub(crate) fn drain_terminal(&mut self, cx: &mut Context<Self>) {
        if let Some(tab) = self.active_terminal() {
            if tab.pty.drain_dirty() {
                cx.notify();
            }
        }
        if self.should_scan_for_metro() && self.scan_terminals_for_metro() {
            self.metro_detected = true;
            self.status =
                "Metro detected -- press Cmd+Shift+D or click \"Connect\" in the status bar".into();
            cx.notify();
        }
    }

    /// True when running the Metro detector would actually surface
    /// useful UX: we haven't already detected, we aren't already
    /// connecting / connected, and there's at least one terminal
    /// tab to scan. Keeps the scan a no-op in the happy path
    /// (connected user editing code) and in the empty path (no
    /// terminal open).
    fn should_scan_for_metro(&self) -> bool {
        if self.metro_detected || self.terminals.is_empty() {
            return false;
        }
        !matches!(
            self.connection,
            ConnectionState::Connecting { .. } | ConnectionState::Connected { .. }
        )
    }

    /// Iterate every terminal tab and return true if any visible
    /// viewport row contains a Metro start-up banner. Stops at the
    /// first match. Doesn't touch scrollback -- the banner is the
    /// first thing Metro prints, so it's still on screen during the
    /// window when this signal is useful.
    fn scan_terminals_for_metro(&self) -> bool {
        for tab in &self.terminals {
            let snap = tab.pty.snapshot();
            for line in snap.text_lines() {
                if METRO_BANNERS.iter().any(|b| line.contains(b)) {
                    return true;
                }
            }
        }
        false
    }

    /// Forward a key to the active terminal tab's PTY when the
    /// Terminal dock is active. Returns true when the key was
    /// consumed.
    pub(crate) fn forward_key_to_terminal(&mut self, key: &gpui::Keystroke) -> bool {
        let Some(tab) = self.active_terminal_mut() else {
            return false;
        };
        let term = &mut tab.pty;
        // Map gpui keys to PTY bytes. Coverage: printable keys via
        // key_char, Enter, Tab, Backspace, arrows, plus ctrl-<letter>.
        let bytes: Vec<u8> = if key.modifiers.control {
            // ctrl-a..z -> 0x01..0x1a
            if key.key.len() == 1 {
                let c = key.key.chars().next().unwrap().to_ascii_lowercase();
                if c.is_ascii_lowercase() {
                    vec![c as u8 - b'a' + 1]
                } else {
                    return false;
                }
            } else {
                return false;
            }
        } else {
            match key.key.as_str() {
                "enter" => vec![b'\r'],
                "tab" => vec![b'\t'],
                "backspace" => vec![0x7f],
                "escape" => vec![0x1b],
                "left" => b"\x1b[D".to_vec(),
                "right" => b"\x1b[C".to_vec(),
                "up" => b"\x1b[A".to_vec(),
                "down" => b"\x1b[B".to_vec(),
                "home" => b"\x1b[H".to_vec(),
                "end" => b"\x1b[F".to_vec(),
                _ => {
                    if let Some(s) = key.key_char.as_deref() {
                        s.as_bytes().to_vec()
                    } else {
                        return false;
                    }
                }
            }
        };
        if let Err(e) = term.send_input(&bytes) {
            tracing::warn!(target: "atomio", error = %e, "pty write failed");
        }
        true
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
    /// Drain pending filesystem ticks from the workspace watcher and
    /// re-scan the tree when anything came through. Cheap when there
    /// are no ticks (one channel try_recv); the rescan itself is the
    /// existing `ignore`-walker cap'd at `DEFAULT_MAX_ENTRIES`.
    ///
    /// After the rescan, prunes `expanded_dirs` to only the
    /// directories still present in the tree so a deleted folder
    /// doesn't leave stale expansion state hanging around.
    pub(crate) fn drain_workspace_events(&mut self, cx: &mut Context<Self>) {
        let Some(watcher) = self.fs_watcher.as_ref() else {
            return;
        };
        if !watcher.drain() {
            return;
        }
        if let Some(ws) = self.workspace.as_mut() {
            let n = ws.refresh();
            // Borrow live directory paths out of the refreshed files
            // slice rather than cloning each PathBuf -- the set lives
            // only for this retain call. expanded_dirs is a disjoint
            // field, so the immutable borrow of ws.files() and the
            // mutable borrow of self.expanded_dirs coexist fine.
            let live_dirs: HashSet<&Path> = ws
                .files()
                .iter()
                .filter(|e| e.kind == workspace::FileKind::Directory)
                .map(|e| e.path.as_path())
                .collect();
            self.expanded_dirs
                .retain(|p| live_dirs.contains(p.as_path()));
            tracing::debug!(target: "atomio", file_count = n, "workspace refreshed");
            cx.notify();
        }
        self.check_for_external_buffer_edit(cx);
    }

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
                    // Any transition out of Disconnected / Failed
                    // means the user acted on the connect prompt
                    // (or the bridge took itself there); the pill
                    // has served its purpose. Re-arms naturally on
                    // the next disconnect because the field flips
                    // back to false in `on_disconnect`.
                    if matches!(
                        state,
                        ConnectionState::Scanning
                            | ConnectionState::Connecting { .. }
                            | ConnectionState::Connected { .. }
                    ) {
                        self.metro_detected = false;
                    }
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
                            self.current_url = Some(url.clone());
                            // Honour any pending frame-click jump that
                            // requested this exact URL. Old pending
                            // entries for other URLs are dropped.
                            if let Some((pending_url, line)) = self.pending_jump.take() {
                                if pending_url == url {
                                    self.state.move_cursor_to(line as usize, 0);
                                }
                            }
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

        // Terminal dock takes input first when active and palette isn't
        // open. Cmd-modified strokes still flow to actions (cmd-j, etc.)
        // so the user can toggle panes without leaving the terminal.
        if matches!(self.dock, DockPane::Terminal)
            && self.palette_query.is_none()
            && !m.platform
            && !self.terminals.is_empty()
            && self.forward_key_to_terminal(keystroke)
        {
            cx.notify();
            return;
        }

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

/// Path to the persisted app state JSON. Mirrors macOS convention
/// `~/Library/Application Support/atomio/state.json`. Parent dirs are
/// created lazily on first save by [`workspace::AppState::save`].
fn state_file_path() -> PathBuf {
    dirs::data_dir()
        .map(|d| d.join("atomio").join("state.json"))
        .unwrap_or_else(|| PathBuf::from("/tmp/atomio/state.json"))
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
    // Hydrate persisted state once before the gpui runtime takes over.
    // Cheap: small JSON file, no I/O on the hot path. Missing /
    // malformed files yield default state silently.
    let mut initial_state = AppState::load(&state_file_path());
    // Drop recents entries whose dirs vanished between sessions so the
    // launch screen never offers a project the user can't open. Persist
    // immediately so a crash before the user does anything still leaves
    // the on-disk state clean.
    let pruned = initial_state.recents.prune_missing();
    if pruned > 0 {
        tracing::info!(target: "atomio", count = pruned, "pruned stale recents");
        if let Err(e) = initial_state.save(&state_file_path()) {
            tracing::warn!(target: "atomio", error = %e, "failed to persist pruned recents");
        }
    }
    // Auto-reopen the most recent project, but only if the user didn't
    // pass an explicit file path on the CLI — that's a stronger signal
    // they want single-file mode for this launch.
    let cli_path_provided = std::env::args().nth(1).is_some();
    let initial_ws_root: Option<PathBuf> = if cli_path_provided {
        None
    } else {
        initial_state.last_project.clone()
    };

    Application::new()
        .with_assets(assets::EmbeddedAssets)
        .run(move |cx| {
            cx.bind_keys([
                KeyBinding::new("cmd-o", OpenFile, Some("atomio")),
                KeyBinding::new("cmd-s", SaveFile, Some("atomio")),
                KeyBinding::new("cmd-shift-r", RevertToSaved, Some("atomio")),
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
                KeyBinding::new("cmd-shift-o", OpenProject, Some("atomio")),
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
                KeyBinding::new("cmd-j", ShowTerminal, Some("atomio")),
                KeyBinding::new("cmd-shift-j", TerminalNew, Some("atomio")),
                KeyBinding::new("cmd-shift-]", TerminalNextTab, Some("atomio")),
                KeyBinding::new("cmd-shift-[", TerminalPrevTab, Some("atomio")),
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
                        let mut commands = build_command_registry();
                        let bridge = cdp_bridge::spawn();
                        let recents = initial_state.recents.clone();
                        // Seed "File: Recent #N" entries from persisted
                        // recents before the window opens so the palette
                        // shows them on first paint.
                        for (i, entry) in recents.entries().iter().enumerate() {
                            commands.register(
                                format!("File: Recent #{i} -- {}", entry.name),
                                format!("open_recent:{i}"),
                            );
                        }
                        // Try to re-open the last project. Logs the
                        // failure path + cause so the user can tell why
                        // they landed on the launch screen instead of
                        // their project; the launch-screen recents list
                        // still offers a click-to-reopen if applicable.
                        let workspace: Option<Workspace> =
                            initial_ws_root
                                .as_ref()
                                .and_then(|p| match Workspace::open(p) {
                                    Ok(w) => Some(w),
                                    Err(e) => {
                                        tracing::info!(
                                            target: "atomio",
                                            path = %p.display(),
                                            error = %e,
                                            "auto-reopen failed, showing launch screen"
                                        );
                                        None
                                    }
                                });
                        let fs_watcher = workspace
                            .as_ref()
                            .and_then(|w| WorkspaceWatcher::spawn(w.root()).ok());
                        let initial_dock = if workspace.is_some() {
                            DockPane::Files
                        } else {
                            DockPane::Console
                        };
                        let initial_status: SharedString = match workspace.as_ref() {
                            Some(w) => {
                                format!("reopened {} ({} files)", w.display_name(), w.files().len())
                                    .into()
                            }
                            None => {
                                "cmd+shift+p palette · cmd+shift+d connect · cmd+1..6 panes".into()
                            }
                        };
                        cx.new(|cx| AtomioWindow {
                            state,
                            commands,
                            status: initial_status,
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
                            dock: initial_dock,
                            current_url: None,
                            workspace,
                            recents,
                            expanded_dirs: HashSet::new(),
                            fs_watcher,
                            terminals: Vec::new(),
                            active_terminal: 0,
                            pending_jump: None,
                            event_buf: Vec::new(),
                            last_loaded_mtime: None,
                            metro_detected: false,
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
