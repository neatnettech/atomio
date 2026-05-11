# Atomio -- Roadmap

A native, GPU-accelerated **debugger + lightweight IDE shell** for React Native and Expo. macOS-first, fully open source.

The audience: Expo / React Native developers who want one fast native window instead of a juggle of Chrome DevTools, archived Flipper, browser tabs, and a separate editor + terminal.

> *"Open project. Hit run. Debug. All in one window."*

Atomio is a greenfield project built in Rust with gpui.

---

## Vision

State-of-the-art, modern, lightweight, **extensible** debugger and IDE shell for React Native / Expo. The full developer loop happens inside the window:

1. **Open project** -- pick an Expo / RN repo, atomio reads the manifest.
2. **Navigate files** -- file tree on the left, fuzzy finder via the palette.
3. **Run the dev server** -- embedded terminal spawns `npx expo start` with the project as cwd.
4. **See the app live** -- simulator pane streams frames from CDP `Page.startScreencast`; click/tap forwards back to the runtime.
5. **Debug** -- breakpoints, call stack, variables, watches, inline values, conditional bp + logpoints.
6. **Inspect** -- network, React component tree, profiler.
7. **Extend** -- plugins (Redux, Zustand, MMKV, custom CDP domains) load through a stable trait API.

Nothing leaves the window. No browser tabs, no separate terminal app, no separate simulator window unless you want one.

---

## Guiding Constraints

1. **Stand on giants.** Adopt `gpui` (Apache-2.0) for rendering rather than rolling a custom UI layer.
2. **macOS / Apple Silicon only** through v1.0. Metal backend, AppKit integration, native dialogs, codesign + notarize in CI from day one.
3. **CDP-first.** Chrome DevTools Protocol is the lingua franca of JavaScript debugging. Hermes exposes CDP. Build on that, not custom protocols.
4. **Debugger-first, IDE-shell-second.** The editor, file tree, and terminal exist to support the debugging workflow end-to-end. They are not a replacement for VS Code or Zed -- no LSP, no git UI, no language server marketplace.
5. **Extensible by design.** Built-in panes (debugger, network, react, profiler) implement the same `AtomioPlugin` trait that future third-party panes will use. Eat our own dogfood from v0.7.
6. **Design system is law.** [`docs/design.md`](design.md) and the interactive mock at [`docs/design/handoff/project/atomio.html`](design/handoff/project/atomio.html) are the visual target. New panes ship styled or don't ship.

---

## Architecture

### Stack

- **Language:** Rust.
- **UI / rendering:** `gpui`, Metal backend.
- **Text buffer:** `ropey` rope for the code editor pane.
- **Syntax:** `tree-sitter` (Rust, TypeScript, TSX, JavaScript, JSON).
- **Debugger protocol:** CDP (Chrome DevTools Protocol) over WebSocket.
- **Runtime target:** Hermes (Expo/RN default engine). JSC support post-v1.0.
- **Terminal:** `portable-pty` for PTY spawn, `vte` for ANSI parse.
- **File watching:** `notify` for project tree refresh.
- **React integration:** React DevTools protocol for component tree.
- **Network inspection:** CDP Network domain events.
- **Plugin discovery:** `inventory` for compile-time registration; `libloading` for dynamic `cdylib` plugins post-v1.0.

### Process Model

```
+----------------------------------------+
| atomio.app (Rust, single binary)       |
| +- gpui UI thread (Metal)              |
| +- editor_core (rope, selections)      |
| +- tree-sitter incremental parse       |
| +- CDP client (WebSocket)              |
| +- React DevTools relay                |
| +- Metro discovery (mDNS / HTTP)       |
| +- PTY worker pool (expo start, etc.)  |
| +- File watcher (project tree)         |
| +- Plugin registry                     |
+----+-----------------------------------+
     | WebSocket / HTTP / PTY
+----+------+----------+----------+---------+
|           |          |          |         |
v           v          v          v         v
Hermes    Metro     React DT   Project    Sim
debug     bundler   backend     shell    screencast
endpoint                                   stream
```

### Crate Layout

```
atomio/
+-- crates/
|   +-- atomio/          # macOS app entry, gpui window, pane layout
|   +-- editor_core/     # buffer + selections + undo/redo (no GUI)
|   +-- language/        # tree-sitter parsing + token classification
|   +-- debugger/        # CDP client, breakpoint manager, scripts, transport
|   +-- console/         # log stream model
|   +-- inspector/       # variable inspector, scope tree, watch
|   +-- network/         # network request capture + display model
|   +-- react_tree/      # React DevTools protocol, component tree
|   +-- workspace/       # project root, file tree, watcher (planned v0.3)
|   +-- terminal/        # PTY spawn, ANSI grid, scrollback (planned v0.4)
|   +-- simulator/       # screencast stream, device picker (planned v0.6)
|   +-- plugin_api/      # AtomioPlugin trait + registry (planned v0.9)
+-- docs/
```

---

## Target Workflow

1. Developer launches atomio. Recents list shows their last 5 projects + a "Open Project" button.
2. Pick a project (or atomio auto-opens the last one). File tree populates from disk; manifest parsed; editor opens `app/_layout.tsx`.
3. `Cmd+J` opens the terminal pane. Auto-suggested command: `npx expo start`. Hit enter, server boots in-pane.
4. atomio detects Metro on `localhost:8081`, connects via CDP. Status bar dot turns green.
5. Simulator pane shows the live JS runtime view (CDP screencast). Click in the pane to interact.
6. Developer sets breakpoints by clicking the gutter. Triggers a flow. Execution pauses; variables / call stack / inline pills populate.
7. Developer steps, watches expressions, jumps frames by clicking the stack, navigates to other files via the tree or `Cmd+P` fuzzy finder.
8. Network pane shows API traffic. Console streams `console.log` with source-mapped locations. React tree shows live components.
9. Edits in the editor trigger hot reload. Loop continues without leaving the window.

---

## Milestones

Dates omitted. Ship when ready. **Reprioritized 2026-05-11** -- vision broadened from "debugger with attached editor" to "debugger + lightweight IDE shell + extensible plugin host". Embedded terminal is now in scope.

### v0.0 -- "It edits" (DONE)

Editor scaffolding. Proves the stack.

- Repo skeleton, MIT license, CI with codesign + notarize stubs.
- gpui window, single buffer, monospace text rendering.
- File open / save via native dialog.
- Cursor, selection, basic editing, undo/redo, clipboard.
- Syntax highlighting (tree-sitter, Rust grammar).
- Command palette with fuzzy search (cmd+shift+p).

### v0.1 -- "It connects" (DONE)

Debugger connection pipeline.

- CDP WebSocket client crate (`debugger`).
- Metro bundler discovery (scan localhost ports, parse `/json/list`).
- Hermes attach: `Runtime.enable`, `Debugger.enable`, receive `Debugger.scriptParsed`.
- Console log stream from `Runtime.consoleAPICalled`.
- tree-sitter grammars: TypeScript, TSX, JavaScript, JSON.
- Display Metro-served sources in editor (read-only).
- Bridge plumbing: breakpoint set/remove/resolved, paused/resumed events, step controls.
- tracing-based diagnostics.

### v0.2 -- "It looks right" (DONE)

Design-system rollout + breakpoint + foundational UI.

- Window chrome with titlebar gradient, activity bar with SVG icons + accent indicator + hover, right dock scaffold.
- Editor pane retokened; breakpoint gutter UI; step toolbar with hover; paused-line highlight.
- Status bar: connection dot + ws_url + language + cursor + dirty marker.
- Inline value pills, minimap, call stack pane, breakpoint sidebar, frame click to source.

### v0.3 -- "It opens" (NEXT)

Project model + file tree. Pivot the app from "single-file editor" to "project shell".

- **`workspace` crate**: `Workspace { root, manifest }`, `detect_expo(path)`, `.gitignore` parser, recents list.
- **Open Project** command + native directory picker; persisted state at `~/Library/Application Support/atomio/state.json`.
- **File tree pane** in left rail (replaces `DockPane::Files` placeholder): recursive scan, expand/collapse, file/folder icons, click to load buffer.
- **`notify` watcher**: tree refreshes on disk changes; open buffers prompt reload on external edit.
- **Title + status bar** become project-relative: titlebar shows `project · relative/path`.
- **Recents launch screen** when no project open.

### v0.4 -- "It runs" (NEXT+1)

Embedded terminal. Required for the "everything in one window" promise.

- **`terminal` crate**: PTY spawn via `portable-pty`, ANSI parsing via `vte`, grid + scrollback model.
- **Terminal pane** in the dock (`DockPane::Terminal`): mono grid render, keyboard input forwarded to PTY, copy/paste, resize.
- **Tabs within pane**: multiple PTYs (e.g. `expo start` + `git status` in another tab).
- **Quick-spawn commands**: palette entries for `expo start`, `expo prebuild`, `npm install`, scoped to the project root.
- **Connect-on-detect**: when Metro appears on a port the terminal exposed, status bar prompts "Metro detected -- connect?".

### v0.5 -- "It debugs" (variables + advanced bp)

Close the core debugging loop.

- **Variables pane** lazy-expand object properties via `Runtime.getProperties` (scaffolding done; rendering polished).
- **Watch expressions UX**: in-pane add/remove instead of palette-only.
- **Conditional breakpoints + logpoints** wired end-to-end (palette flow + `BpKind` already exist; need CDP `condition` round-trip + UI).
- **Hit counts** in breakpoint sidebar (track via `Debugger.paused.hitBreakpoints`).
- **Scope filter chips** (Local / Closure / Global tabs).
- **Pretty-printers** for common React Native types (`StyleSheet`, `Animated.Value`, `NativeModule`).

### v0.6 -- "It mirrors" (live simulator)

Make the simulator pane first-class.

- Swap one-shot `Page.captureScreenshot` for `Page.startScreencast` (live 60Hz JPEG stream).
- **`simulator` crate**: screencast frame buffer + decoder; pause/resume when pane hidden to save CPU.
- **Device picker chip**: list Hermes runtime view + booted iOS sims (`xcrun simctl list booted`).
- **Touch dispatch**: click/drag in pane sends `Input.dispatchMouseEvent` to Hermes; mouse/keyboard forwarded to `simctl` for native chrome later.
- **Perf chips**: FPS / memory / network strip atop the phone frame, fed from CDP Performance metrics.

### v0.7 -- "It inspects" (network + react + profiler)

Beyond the breakpoint loop. (Current `network`, `react_tree`, `metrics` panes already exist; this milestone polishes them and unifies their plumbing under the plugin trait.)

- Network: response body preview (json/text), filter chips by `ResourceKind`, search, save HAR.
- React tree: prop/state diff highlighting, "find in tree", select-in-app -> highlight overlay via `Input` events.
- Profiler: frame chart on top, flame graph below, suggestion banner; recording bound to a CDP `Tracing` session.

### v0.8 -- "It flows" (editor polish)

Daily-driver editor ergonomics.

- **Fuzzy file finder** (`Cmd+P`) reuses `editor_core::command::FuzzyMatcher` over the workspace file list.
- **Multi-buffer tabs** above the editor pane.
- **Splits**: horizontal split inside the editor pane (read-only debugger view + writable patch view).
- **Persistent state**: open files + breakpoints + active pane survive restart.
- **Auto-reconnect** when Metro restarts.

### v0.9 -- "It extends" (plugin API)

Open the door for third-party panes and inspectors.

- **`plugin_api` crate**: `AtomioPlugin` trait + `PaneDescriptor`, `CommandDescriptor`, CDP event hook.
- **Compile-time registry** via `inventory`: built-in `network`, `react_tree`, `inspector` crates re-shaped as plugins (no runtime cost vs current model, but proves the API).
- **Sample plugin in-tree**: `redux_inspector` reading `__REDUX_DEVTOOLS_EXTENSION__` messages.
- **Plugin metadata** in palette + status bar (which plugins loaded, version).
- Dynamic `cdylib` discovery via `libloading` -- spec'd but not wired until post-v1.0.

### v1.0 -- "Ship it"

Production-ready.

- **Source maps**: fetch + parse `.map` files, resolve generated -> original (`debugger::sourcemap` module). Click bundle line -> jump to your `.tsx` file.
- Polished UI, keyboard-driven workflow.
- Auto-update (Sparkle or equivalent).
- Single signed, notarized `.dmg` produced by CI on every tag.
- User documentation + getting-started guide + plugin author guide.
- Stable CDP + plugin API surface (semver).
- Performance: cold start <200ms, 120fps scroll, handle large source maps without stall, terminal grid renders 10k-line scrollback without jank.

### Post-v1.0 (out of scope for year one)

- JSC (JavaScriptCore) debugger support.
- Linux + Windows ports.
- Dynamic `cdylib` plugin loading (API exists in v0.9; runtime discovery later).
- Collaborative debugging sessions.
- DAP (Debug Adapter Protocol) support for non-JS targets.
- Plugin marketplace / signing.

---

## What changed in this reprioritization

Compared to the pre-2026-05-11 plan:

- **Vision broadened**: from "debugger with attached editor" to "debugger + lightweight IDE shell + extensible plugin host". Reason: the end-to-end demo workflow (open -> nav -> run -> see app) requires terminal + file tree + simulator stream, not just panes around a single buffer.
- **Embedded terminal is in scope** (new v0.4). Previously banned in the won't-do list -- lifted.
- **File tree pulled forward** from v0.6 to v0.3. The project-shell pivot needs it early.
- **Live simulator screencast** carved out as its own milestone (v0.6). Previously a one-line mention under "v0.5 simulator+profiler".
- **Plugin API is now an explicit milestone** (v0.9). Previously listed as "post-v1.0".
- **v0.5 narrows** to advanced debugging (variables polish, conditional bp, hit counts). Previously bundled with profiler.
- **v0.7 absorbs network + react + profiler polish** as a single "inspect" milestone, since those panes already exist as basic implementations.
- **Source maps stay at v1.0**.

---

## Won't-Do List

Tempting, but explicitly out of scope through v1.0:

- Cross-platform (macOS only).
- Custom rendering engine (use `gpui`).
- LSP / language server integration.
- Git integration (status, blame, diff, log).
- Vim mode.
- Notebook / Jupyter UI.
- Cloud sync, accounts, or telemetry.
- General-purpose marketplace (plugins ship in-tree through v1.0).
- Native iOS / Android simulator process management beyond screencast + `simctl` shell-out.

Each "no" is what makes the "yes" list achievable.

---

## Verification Targets (by v1.0)

- Cold start <200ms on M-series Mac (`hyperfine`).
- 120fps sustained scroll on source files >10k lines.
- Open project -> first paint of file tree <100ms on a 5k-file repo.
- `npx expo start` from terminal -> Metro detected + CDP connected in <3s after the bundler reports ready.
- Connect to a running Expo app and hit first breakpoint in <2s.
- Step through 100 consecutive lines without UI stall.
- Display 1000 network requests without scroll jank.
- Handle source maps >50MB without blocking the UI thread.
- Terminal grid: 10k-line scrollback at 60fps.
- Simulator screencast: 30fps sustained, <100ms input-to-frame latency.
- Single signed, notarized `.dmg` produced by CI on every tag.
- 100% of UI surfaces match `docs/design.md` tokens (no inline hex literals).

---

## Risks and Mitigations

| Risk | Mitigation |
|---|---|
| `gpui` too coupled to Zed to extract cleanly | Validated in v0.0 (gpui 0.2 from crates.io works). Fallback: `floem`. |
| Hermes CDP surface incomplete for full debugging | Hermes team actively expanding CDP. Worst case: supplement with custom Hermes inspector messages. |
| Metro source map format changes | Pin Metro version support matrix. Test against Expo SDK versions. |
| React DevTools protocol instability | Vendor the protocol types. Target one React DevTools version per atomio release. |
| Apple notarization headaches | Codesign + notarize CI in place from v0.0. |
| Design alignment drift across PRs | Theme module enforces token use. PR template requires comparison against mock for UI changes. |
| Embedded terminal scope creep into full shell experience | Hard cap: PTY + ANSI grid + scrollback + tabs. No shell integration features (zsh prompts, ligatures, transparency). |
| Plugin API churn breaks downstream plugins | Trait + descriptor types vendored in `plugin_api` crate; `#[non_exhaustive]` on every public enum/struct so additions don't break. |
| Screencast bandwidth saturates UI thread | Decode JPEG frames on a background task; gpui only sees the decoded `Pixmap` via a channel. |
| "Yet another debugger" fatigue | Native speed + Expo focus + in-window terminal/simulator is the differentiator. Ship v0.6 demo video before announcing publicly. |
