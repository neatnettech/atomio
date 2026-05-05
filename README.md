# atomio

[![status: pre-alpha](https://img.shields.io/badge/status-pre--alpha-red)](docs/ROADMAP.md)
[![ci](https://github.com/neatnettech/atomio/actions/workflows/ci.yml/badge.svg)](https://github.com/neatnettech/atomio/actions/workflows/ci.yml)
[![license: MIT](https://img.shields.io/badge/license-MIT-blue.svg)](LICENSE)

> **A native, GPU-accelerated debugger for React Native and Expo.**
> Inspect variables. Track execution. Edit code. All in one window.

> **Status: pre-alpha.** The trunk builds and opens a window with a working code editor. Debugger connection, variable inspector, and breakpoint UI are under active development. See [the roadmap](docs/ROADMAP.md).

**atomio** is a standalone debugger and code editor for [Expo](https://expo.dev) / React Native developers -- built from scratch in Rust on a native, GPU-accelerated stack. It connects to your running Expo app via the Chrome DevTools Protocol (CDP) and Hermes, giving you breakpoints, variable inspection, call stack navigation, network monitoring, and a full code editor in a single native macOS window.

macOS (Apple Silicon) first. MIT licensed. No CLA.

## Why

React Native debugging is fragmented. You bounce between Chrome DevTools, Flipper (archived), React Native Debugger (unmaintained), VS Code's debugger extension, and Expo's dev tools browser UI. None of them are purpose-built for Expo. None of them are fast. atomio consolidates the debugger workflow into a single native app that launches instantly, connects to your Metro bundler, and stays out of your way.

## What

- **Rust + [`gpui`](https://github.com/zed-industries/zed) + Metal** -- native, GPU-rendered, 120fps on ProMotion.
- **CDP / Hermes debugger protocol** -- connect to any running Expo / React Native app.
- **Breakpoints** -- set, hit, continue, step over, step into, step out.
- **Variable inspector** -- scopes, watch expressions, inline values.
- **Call stack** -- navigate frames, inspect closures.
- **Network inspector** -- HTTP requests, WebSocket frames, timing.
- **Console** -- log output with source mapping.
- **React component tree** -- via React DevTools protocol integration.
- **Code editor** -- syntax-highlighted, with undo/redo, selection, clipboard. Edit and hot-reload without leaving the debugger.
- **Tree-sitter** highlighting for TypeScript, TSX, JavaScript, JSON.
- **Command palette** -- cmd+shift+p for every action.
- **Single signed `.dmg`**, auto-updating, no Electron, no browser tab.

## Status

**Pre-v0.1.** The code editor works (buffer, selection, undo/redo, syntax highlighting, command palette). The debugger connection layer is the active focus. Follow along -- this is being built in public.

## Roadmap (TL;DR)

| Version | Theme | What ships |
|---|---|---|
| v0.0 | "It edits" | gpui window, code editor, syntax highlighting, command palette |
| v0.1 | "It connects" | CDP client, Metro discovery, Hermes attach, console log stream |
| v0.2 | "It debugs" | breakpoints, step controls, variable inspector, call stack |
| v0.3 | "It inspects" | network inspector, React component tree, performance timeline |
| v0.4 | "It flows" | file tree, fuzzy finder, multi-pane splits, integrated hot reload |
| v1.0 | "Ship it" | polish, auto-update, documentation, stable CDP protocol surface |

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
