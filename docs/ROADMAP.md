# Atomio -- Roadmap

A native, GPU-accelerated debugger for React Native and Expo. macOS-first, fully open source.

The pitch is a single audience:

- Expo / React Native developers who are tired of switching between Chrome DevTools, Flipper (archived), and scattered browser-based tools. One native app, fast, purpose-built.

> *"Stop debugging your debugger. Start debugging your app."*

Atomio is a greenfield project built in Rust with gpui.

---

## Guiding Constraints

1. **Stand on giants.** Adopt `gpui` (Apache-2.0) for rendering rather than rolling a custom UI layer.
2. **macOS / Apple Silicon only** through v1.0. Metal backend, AppKit integration, native dialogs, codesign + notarize in CI from day one.
3. **CDP-first.** Chrome DevTools Protocol is the lingua franca of JavaScript debugging. Hermes exposes CDP. Build on that, not custom protocols.
4. **Debugger-first, editor-second.** The code editor exists to support the debugging workflow (set breakpoints, read context, quick fixes). It is not a general-purpose editor competing with VS Code or Zed.
5. **No plugins in year one.** Ship a focused, integrated experience. Extensibility comes after the core is solid.

---

## Architecture

### Stack

- **Language:** Rust.
- **UI / rendering:** `gpui`, Metal backend.
- **Text buffer:** `ropey` rope for the code editor pane.
- **Syntax:** `tree-sitter` (TypeScript, TSX, JavaScript, JSON, Markdown).
- **Debugger protocol:** CDP (Chrome DevTools Protocol) over WebSocket.
- **Runtime target:** Hermes (Expo/RN default engine). JSC support later.
- **React integration:** React DevTools protocol for component tree.
- **Network inspection:** CDP Network domain events.

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
+----+-----------------------------------+
     | WebSocket / HTTP
+----+------+----------+----------+
|           |          |          |
v           v          v          v
Hermes    Metro     React DT   (source
debug     bundler   backend     maps)
endpoint
```

### Crate Layout

```
atomio/
+-- crates/
|   +-- atomio/          # macOS app entry, gpui window, pane layout
|   +-- editor_core/     # buffer + selections + undo/redo (no GUI)
|   +-- language/        # tree-sitter parsing + token classification
|   +-- debugger/        # CDP client, breakpoint manager, step controller
|   +-- inspector/       # variable inspector, scope tree, watch expressions
|   +-- network/         # network request capture + display model
|   +-- console/         # log stream model, source map resolution
|   +-- react_tree/      # React DevTools protocol, component tree model
|   +-- workspace/       # panes, file tree, command palette, layout
+-- docs/
```

---

## Target Workflow

1. Developer runs `npx expo start` in their terminal.
2. Developer opens atomio. It discovers the Metro bundler on the local network automatically (or accepts a manual URL).
3. atomio connects to the Hermes debug endpoint via CDP WebSocket.
4. Developer sees their project source (fetched from Metro source maps), sets breakpoints, and triggers the flow they want to debug.
5. When a breakpoint hits: execution pauses, the editor navigates to the line, variables panel shows local/closure/global scopes, call stack is navigable.
6. Developer steps through code, inspects values, optionally edits source and triggers hot reload.
7. Network panel shows HTTP/WS traffic. Console panel shows `console.log` output with source-mapped locations.
8. React component tree shows the live component hierarchy, props, and state.

---

## Milestones

Dates are deliberately omitted. Ship when it's ready.

### v0.0 -- "It edits" (DONE)

Prove the stack works end-to-end.

- Repo skeleton, MIT license, CI with codesign + notarize stubs.
- gpui window, single buffer, monospace text rendering.
- File open / save via native dialog.
- Cursor, selection, basic editing, undo/redo, clipboard.
- Syntax highlighting (tree-sitter, Rust grammar as proof-of-concept).
- Command palette with fuzzy search.

### v0.1 -- "It connects"

Establish the debugger connection pipeline.

- CDP WebSocket client crate (`debugger`).
- Metro bundler discovery: scan localhost ports, parse `/json/list` endpoint.
- Hermes attach: `Runtime.enable`, `Debugger.enable`, receive `Debugger.scriptParsed`.
- Console log stream: capture `Runtime.consoleAPICalled` events, display in a console pane.
- Source map fetching and resolution (inline and external maps from Metro).
- tree-sitter grammars for TypeScript, TSX, JavaScript, JSON.
- Display source files fetched from Metro in the editor pane.

### v0.2 -- "It debugs"

The core debugging loop.

- Breakpoint manager: set (`Debugger.setBreakpointByUrl`), remove, toggle.
- Breakpoint gutter UI: click line number to toggle, visual indicators.
- Pause / resume / step over / step into / step out controls.
- Variable inspector pane: local scope, closure scope, global scope.
- Watch expressions: user-defined expressions evaluated in paused context.
- Call stack pane: navigate frames, click to jump to source.
- Inline value display: show variable values next to their declaration on pause.
- Conditional breakpoints and logpoints.

### v0.3 -- "It inspects"

Deeper visibility into the running app.

- Network inspector: capture HTTP requests/responses via CDP Network domain.
- Request detail view: headers, body, timing, response.
- WebSocket frame inspector.
- React component tree: connect to React DevTools backend, render component hierarchy.
- Component props + state inspection.
- Performance timeline: basic frame timing from CDP Performance domain.
- Bundle size view: parsed from Metro bundle output.

### v0.4 -- "It flows"

Usable as a daily driver.

- File tree pane: browse project source from Metro.
- Fuzzy file finder (cmd+p).
- Multi-pane splits and tabs (editor, console, variables, network, components).
- Trigger hot reload from editor.
- Persist breakpoints across sessions.
- Auto-reconnect when Metro restarts.
- One built-in theme (light + dark).

### v1.0 -- "Ship it"

Production-ready.

- Polished UI, keyboard-driven workflow.
- Auto-update (Sparkle or equivalent).
- Single signed, notarized `.dmg` produced by CI on every tag.
- User documentation and getting-started guide.
- Stable CDP protocol surface (document what's supported).
- Performance: cold start <200ms, 120fps scroll, handle large source maps without stall.

### Post-v1.0 (out of scope for year one)

- JSC (JavaScriptCore) debugger support.
- Linux + Windows ports.
- Plugin / extension system.
- Collaborative debugging sessions.
- DAP (Debug Adapter Protocol) support for non-JS targets.

---

## Won't-Do List

Tempting, but explicitly out of scope in year one:

- Cross-platform (macOS only).
- General-purpose editor features (LSP, git integration, terminal).
- Custom rendering engine (use `gpui`).
- Plugin system or extension API.
- Vim mode.
- Notebook / Jupyter UI.
- Cloud sync, accounts, or telemetry.
- iOS/Android simulator management (use Expo CLI for that).

Each "no" is what makes the "yes" list achievable.

---

## Verification Targets (by v1.0)

- Cold start <200ms on M-series Mac (`hyperfine`).
- 120fps sustained scroll on source files >10k lines (gpui frame timing).
- Connect to a running Expo app and hit first breakpoint in <2s.
- Step through 100 consecutive lines without UI stall.
- Display 1000 network requests without scroll jank.
- Handle source maps >50MB without blocking the UI thread.
- Single signed, notarized `.dmg` produced by CI on every tag.

---

## Risks and Mitigations

| Risk | Mitigation |
|---|---|
| `gpui` too coupled to Zed to extract cleanly | Validated in v0.0 (gpui 0.2 from crates.io works). Fallback: `floem`. |
| Hermes CDP surface incomplete for full debugging | Hermes team actively expanding CDP. Worst case: supplement with custom Hermes inspector messages. |
| Metro source map format changes | Pin Metro version support matrix. Test against Expo SDK versions. |
| React DevTools protocol instability | Vendor the protocol types. Target one React DevTools version per atomio release. |
| Apple notarization headaches | Codesign + notarize CI in place from v0.0. |
| "Yet another debugger" fatigue | Native speed + Expo focus is the differentiator. Ship v0.2 before announcing publicly. |
