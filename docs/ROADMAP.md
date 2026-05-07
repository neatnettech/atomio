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
6. **Design system is law.** [`docs/design.md`](design.md) and the interactive mock at [`docs/design/handoff/project/atomio.html`](design/handoff/project/atomio.html) are the visual target. New panes ship styled or don't ship.

---

## Architecture

### Stack

- **Language:** Rust.
- **UI / rendering:** `gpui`, Metal backend.
- **Text buffer:** `ropey` rope for the code editor pane.
- **Syntax:** `tree-sitter` (Rust, TypeScript, TSX, JavaScript, JSON).
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
|   +-- debugger/        # CDP client, breakpoint manager, scripts, transport
|   +-- console/         # log stream model, source map resolution
|   +-- inspector/       # variable inspector, scope tree, watch (planned)
|   +-- network/         # network request capture + display model (planned)
|   +-- react_tree/      # React DevTools protocol, component tree (planned)
|   +-- workspace/       # panes, file tree, command palette, layout (planned)
+-- docs/
```

---

## Target Workflow

1. Developer runs `npx expo start` in their terminal.
2. Developer opens atomio. It discovers the Metro bundler on the local network automatically.
3. atomio connects to the Hermes debug endpoint via CDP WebSocket.
4. Developer sees their project source (fetched from Metro), sets breakpoints, and triggers the flow they want to debug.
5. When a breakpoint hits: execution pauses, the editor navigates to the line, variables panel shows local/closure/global scopes, call stack is navigable.
6. Developer steps through code, inspects values, optionally edits source and triggers hot reload.
7. Network panel shows HTTP/WS traffic. Console panel shows `console.log` output with source-mapped locations.
8. React component tree shows the live component hierarchy, props, and state.

---

## Reprioritized Milestones

Dates omitted. Ship when ready. **Reprioritized 2026-05-06** to reflect debugger-first reality and design-system rollout.

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
- Bridge plumbing: breakpoint set/remove/resolved, paused/resumed events, step controls (Resume / StepOver / StepInto / StepOut / Pause).
- tracing-based diagnostics (stderr + `~/Library/Logs/atomio/atomio.log`).
- Smart target selection (skip Expo Go shell + UI targets).

### v0.2 -- "It looks right" (NEXT)

Design-system rollout + breakpoint UI. Block all new panes behind this; every pane shipped from here on matches `docs/design.md`.

- **Window chrome**: macOS-style centered window, traffic lights, 36px titlebar with document title + branch suffix.
- **Activity bar** (48px left rail) with file / debugger / simulator / components / profiler / console icons. Active item glows accent.
- **Editor pane** retokened: gutter dim TX_5, accent caret, ACCENT_SOFT selection.
- **Right dock** scaffold (currently console-only) — switchable region for future panes.
- **Status bar** redesigned per spec: connection dot + ws_url + cursor + branch.
- **Breakpoint gutter UI**: click line number to toggle, accent dot when set.
- **Step toolbar**: Continue / Step Over / Step Into / Step Out / Pause buttons with hover states (already keyboard-bindable).
- **Paused-line highlight**: full-row ACCENT_SOFT background + arrow on gutter.

### v0.3 -- "It debugs" (variables + call stack)

Core debugging loop completed.

- **`inspector` crate**: scope tree (Local / Closure / Global), watch expressions.
- **Variables pane** in right dock: lazy-expand object properties via `Runtime.getProperties`.
- **Call stack pane**: navigate frames, click jumps to source.
- **Inline value pills** next to expressions on the paused line.
- **Conditional breakpoints + logpoints** (extends `BreakpointRegistry`, sends `condition` to CDP).
- Breakpoint sidebar list with hit counts.

### v0.4 -- "It inspects" (network + react)

Beyond the breakpoint loop.

- **`network` crate** + pane: CDP Network domain capture, request list, headers/body/timing.
- WebSocket frame inspector.
- **`react_tree` crate**: connect to React DevTools backend, render component hierarchy.
- Component props + state inspection.

### v0.5 -- "Simulator + profiler"

The killer-feature lineup from the design chat.

- **Simulator pane**: device picker, embedded iOS Simulator view (initially: live screenshot stream from CDP `Page.captureScreenshot` or simctl bridge), perf chips (FPS / memory / network).
- **Profiler pane**: frame chart on top, flame graph below, suggestion banner.
- Performance timeline from CDP Performance domain.

### v0.6 -- "It flows"

Daily-driver polish.

- **`workspace` crate**: pane layout, tabs, splits.
- File tree pane (Metro-served sources + local project).
- Fuzzy file finder (cmd+p) -- reuse fuzzy matcher from `editor_core::command`.
- Trigger hot reload from editor.
- Persist breakpoints + open files across sessions.
- Auto-reconnect when Metro restarts.
- Project picker / launch screen (recents list, simulator-ready hint).

### v1.0 -- "Ship it"

Production-ready.

- **Source maps**: fetch + parse `.map` files, resolve generated → original (`debugger::sourcemap` module). Click bundle line → jump to your `.tsx` file.
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

## What changed in this reprioritization

Compared to the pre-2026-05-06 plan:

- **v0.2 narrows** to design-system rollout + breakpoint UI. Previously bundled variables / call stack / step controls into a single milestone — that was too wide.
- **v0.3 picks up the variables + call stack loop** as a focused milestone.
- **v0.4 swaps to extras** (network + React tree). Previously v0.3.
- **v0.5 is new** -- simulator + profiler. Previously implicit. Design chat called these out as killer-feature material.
- **v0.6 takes over the editor-shaped work** (file tree, fuzzy finder, splits). Previously v0.4. De-prioritized because the debugger workflow doesn't need it before v1.0.
- **Source maps move to v1.0**. Previously v0.1. Debugger works without them — bundle URLs are fine for development; mapping to original `.tsx` is polish.
- **Design-system rollout is explicit**, no longer "happens whenever". Every pane shipped after v0.2 must match `docs/design.md`.

The "Won't-Do List" is unchanged.

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
- iOS/Android simulator management beyond a screenshot stream (use Expo CLI for that).

Each "no" is what makes the "yes" list achievable.

---

## Verification Targets (by v1.0)

- Cold start <200ms on M-series Mac (`hyperfine`).
- 120fps sustained scroll on source files >10k lines.
- Connect to a running Expo app and hit first breakpoint in <2s.
- Step through 100 consecutive lines without UI stall.
- Display 1000 network requests without scroll jank.
- Handle source maps >50MB without blocking the UI thread.
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
| Design alignment drift across PRs | Theme module enforces token use. PR template will require comparison against mock for UI changes. |
| "Yet another debugger" fatigue | Native speed + Expo focus is the differentiator. Ship v0.4 before announcing publicly. |
