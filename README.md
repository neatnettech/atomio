# atomio

[![status: pre-alpha](https://img.shields.io/badge/status-pre--alpha-red)](docs/ROADMAP.md)
[![ci](https://github.com/neatnettech/atomio/actions/workflows/ci.yml/badge.svg)](https://github.com/neatnettech/atomio/actions/workflows/ci.yml)
[![license: MIT](https://img.shields.io/badge/license-MIT-blue.svg)](LICENSE)

> **A native, GPU-accelerated debugger + lightweight IDE shell for React Native and Expo.**
> Open project. Hit run. Debug. All in one window.

> **Status: pre-alpha.** Trunk builds. Editor, design system, breakpoint UI, call stack, watches, network/react/profiler scaffolding, and minimap all in place. Project model + file tree + embedded terminal land next. See [the roadmap](docs/ROADMAP.md).

**atomio** is a debugger and lightweight IDE shell for [Expo](https://expo.dev) / React Native developers -- built from scratch in Rust on a native, GPU-accelerated stack. It connects to your running Expo app via the Chrome DevTools Protocol (CDP) and Hermes, and ships the surrounding workflow -- file tree, embedded terminal, live simulator stream, debugger panes, console, network, React tree, profiler -- in a single native macOS window. Extensible via a `plugin_api` trait from v0.9.

macOS (Apple Silicon) first. MIT licensed. No CLA.

## The vision

State-of-the-art, modern, lightweight, **extensible** debugger and IDE shell. **Dark-only**, **Zed-minimal with Atom warmth**. The full developer loop happens inside the window: open project -> navigate files -> run dev server in embedded terminal -> see the app live on the simulator pane -> debug + inspect against the live Hermes runtime. No browser tabs, no separate terminal app, no separate simulator window unless you want one. Plugins (Redux, Zustand, MMKV, custom CDP domains) load through a stable trait API.

Live interactive mocks (open in any browser, no build step):
- App: [`docs/design/handoff/project/atomio.html`](docs/design/handoff/project/atomio.html)
- Brand (icons + GitHub hero + social card): [`docs/design/brand-handoff/project/brand.html`](docs/design/brand-handoff/project/brand.html)

```
+----------------------------------------------------------------------+
| ●●●           foaf-mobile  · feat/checkout                           |
+----+--------------------------------+--------------------------------+
| 📁 |  app/                          |  Debugger                      |
| 🐛 |   (tabs)/                      |   ▶  Continue   ⏭  Step over   |
| 📱 |     index.tsx     [active]     |   ↪  Step into  ⤴  Step out    |
| 🧩 |     cart.tsx                   |   ⏸  Pause                     |
| 📊 |   _layout.tsx                  |  Call stack                    |
| 📜 |                                |   ▸ checkout()  cart.tsx:42    |
|    |  ────────────────────────────  |   ▸ onPress     cart.tsx:18    |
|    |  ▶  42  const total = items    |  Variables                     |
|    |  ●  43    .reduce((a,b) =>     |   ▸ Local                      |
|    |     44      a + b.price, 0);   |     items:  Array(3)           |
|    |     45                         |     total:  126.50             |
|    |     46  return (               |   ▸ Closure                    |
|    |     47    <View>               |   ▸ Global                     |
|    |     48      <Text>{total}</T>  |  Watch                         |
|    |     49    </View>              |   total * 1.08  →  136.62      |
|    |     50  );                     |  Breakpoints (2)               |
+----+--------------------------------+--------------------------------+
| ● connected  ws://localhost:8081/inspector/...   ln 43 · col 1   tsx |
+----------------------------------------------------------------------+
```

**Accent green** `#3ecf8e` for run states. **Calm syntax palette** (Zed One Dark adjacent). **SF Pro + SF Mono.** **Spotlight-style command palette** (Cmd+Shift+P) drives everything.

Full design spec: [`docs/design.md`](docs/design.md). Handoff bundles: [`docs/design/handoff/`](docs/design/handoff/) (app), [`docs/design/brand-handoff/`](docs/design/brand-handoff/) (brand).

## Why

React Native debugging is fragmented. You bounce between Chrome DevTools, Flipper (archived), React Native Debugger (unmaintained), VS Code's debugger extension, and Expo's dev tools browser UI. None of them are purpose-built for Expo. None of them are fast. atomio consolidates the debugger workflow into a single native app that launches instantly, connects to your Metro bundler, and stays out of your way.

## What

- **Rust + [`gpui`](https://github.com/zed-industries/zed) + Metal** -- native, GPU-rendered, 120fps on ProMotion.
- **CDP / Hermes debugger protocol** -- connect to any running Expo / React Native app.
- **Project shell** -- open a folder, file tree, fuzzy finder, recents launch screen.
- **Embedded terminal** -- PTY-backed grid with ANSI parsing; run `npx expo start` without leaving the window.
- **Live simulator** -- CDP `Page.startScreencast` frames in a phone-frame pane; click to forward input.
- **Breakpoints** -- set, hit, continue, step over, step into, step out, conditional + logpoints.
- **Variable inspector** -- scopes, watch expressions, inline values.
- **Call stack** -- navigate frames, click jumps to source.
- **Network inspector** -- HTTP requests, WebSocket frames, timing.
- **Console** -- log output with source mapping.
- **React component tree** -- via React DevTools protocol integration.
- **Profiler** -- frame chart + flame graph + Performance metrics.
- **Code editor** -- syntax-highlighted, with undo/redo, selection, clipboard, minimap. Edit and hot-reload without leaving the debugger.
- **Tree-sitter** highlighting for TypeScript, TSX, JavaScript, JSON.
- **Command palette** -- cmd+shift+p for every action.
- **Plugin trait** -- third-party panes via `AtomioPlugin` (v0.9).
- **Single signed `.dmg`**, auto-updating, no Electron, no browser tab.

## Status

**v0.2 done.** Editor + design system + breakpoint loop + inspector scaffolding all in trunk. Active focus: **v0.3 "It opens"** -- project model + file tree. Follow along -- this is being built in public.

## Roadmap (TL;DR)

| Version | Theme | What ships |
|---|---|---|
| v0.0 | "It edits" (DONE) | gpui window, code editor, syntax highlighting, command palette |
| v0.1 | "It connects" (DONE) | CDP client, Metro discovery, Hermes attach, console log stream, bridge wiring |
| v0.2 | "It looks right" (DONE) | design system, breakpoint UI, step toolbar, call stack, watches, inline pills, minimap |
| v0.3 | "It opens" (NEXT) | project model, file tree, native dir picker, watcher, recents |
| v0.4 | "It runs" | embedded terminal (PTY + ANSI grid + scrollback + tabs) |
| v0.5 | "It debugs" | variables polish, conditional bp + logpoints, hit counts, pretty-printers |
| v0.6 | "It mirrors" | live simulator screencast, touch dispatch, device picker, perf chips |
| v0.7 | "It inspects" | network + react tree + profiler polish |
| v0.8 | "It flows" | fuzzy finder, multi-buffer tabs, splits, persistent state |
| v0.9 | "It extends" | `plugin_api` trait, in-tree reference plugins |
| v1.0 | "Ship it" | source maps, auto-update, signed DMG, stable API |

Full roadmap: [`docs/ROADMAP.md`](docs/ROADMAP.md).

## Build

```sh
cargo run -p atomio
```

Requires:

- Rust stable
- macOS 13+ on Apple Silicon
- Full Xcode (not just Command Line Tools) -- gpui compiles Metal shaders at build time and needs the `metal` compiler bundled with Xcode.

## License

MIT.
