# atomio

> **Atom's soul. VS Code's ecosystem. Zed's speed.**
> Hackable to the core. Again.

**atomio** is a revival of the [Atom editor](https://github.com/atom/atom) — rebuilt from scratch in Rust on a native, GPU-accelerated stack, designed to feel as hackable as Atom, run the VS Code extensions you already use, and ship with an AI agent that lives *inside* the editor.

Built solo, after hours, in the open. macOS (Apple Silicon) first. MIT licensed. No CLA.

## Why

Atom was archived in December 2022. The Electron + CoffeeScript stack couldn't keep up — but the *philosophy* (hackable, community-first, beautiful by default) is more relevant than ever in an editor landscape split between VS Code's ecosystem and Zed's speed. atomio reclaims that philosophy on a stack that can actually compete.

## What

- **Rust + [`gpui`](https://github.com/zed-industries/zed) + Metal** — native, GPU-rendered, ProMotion-smooth.
- **Tree-sitter** for syntax (Atom invented it — we're bringing it home).
- **VS Code extension compatibility** via a Node sidecar host.
- **AI agent built in** — pluggable providers (Anthropic, OpenAI, Ollama). Inline edits, not a chat sidebar.
- **WASM + TypeScript SDK** for native plugins, with an API that echoes Atom's beloved `atom.workspace`.
- **Single signed `.dmg`**, auto-updating, no Electron.

## Status

**Pre-v0.0.** This repo is a few hours old. The roadmap lives in [`docs/ROADMAP.md`](docs/ROADMAP.md). Follow along — this is being built in public.

## Roadmap (TL;DR)

| Version | Theme | What ships |
|---|---|---|
| v0.0 | "Hello, atom" | gpui window, basic editing, brand |
| v0.1 | "It edits" | tree-sitter, command palette, file tree |
| v0.2 | "It speaks LSP" | LSP, splits, git gutter |
| v0.3 | "It has an ecosystem" | VS Code extension host |
| v0.4 | "It's an AI editor" | inline AI agent |
| v1.0 | "It's hackable" | WASM plugin runtime + TS SDK |

Full roadmap: [`docs/ROADMAP.md`](docs/ROADMAP.md).

## Build

```sh
cargo run -p atomio
```

Requires:

- Rust stable
- macOS 13+ on Apple Silicon
- Xcode Command Line Tools
- Apple's Metal Toolchain — install once with:

  ```sh
  xcodebuild -downloadComponent MetalToolchain
  ```

  (gpui compiles its Metal shaders at build time and needs this even for a
  trivial window. Roughly 700 MB.)

## License

MIT. Atom is a trademark of GitHub. atomio is an independent project, not affiliated with or endorsed by GitHub or the original Atom team — built in tribute.
