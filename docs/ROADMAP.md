# Atomio — Roadmap

Atomio revives the Atom philosophy ("hackable to the core") on a modern, native, GPU-accelerated foundation. macOS-first, fully open source.

The pitch is the union of three audiences:

- Atom alumni who want hackability and soul back.
- VS Code users who want to keep their extensions but leave the lag.
- Zed users who want native speed and first-class AI.

> *"Atom's soul, VS Code's ecosystem, Zed's speed — and the AI agent lives inside the editor, not as a sidecar."*

Atomio is a greenfield project. It draws inspiration, brand, and select assets from `atom/atom`, but does not fork its Electron/CoffeeScript codebase.

---

## Guiding Constraints

1. **Stand on giants.** Adopt `gpui` (Apache-2.0) directly rather than rolling a renderer.
2. **macOS / Apple Silicon only** through v1.0. Metal backend, AppKit integration, native dialogs, codesign + notarize in CI from day one.
3. **Compatibility lane over native rewrite.** Ship a VS Code extension host as a Node sidecar before a native WASM plugin tier.
4. **No collab, no debugger, no Linux/Windows in year one.**

---

## Architecture

### Stack

- **Language:** Rust.
- **UI / rendering:** `gpui`, Metal backend.
- **Text buffer:** rope + CRDT-ready primitives. `ropey` first; CRDT layer slots in later.
- **Syntax:** `tree-sitter`.
- **Language intelligence:** LSP client, one process per language server.
- **AI:** first-class inline panel, pluggable providers (Anthropic default, OpenAI, Ollama).
- **Plugin host (v0.1):** Node sidecar running unmodified VS Code extensions.
- **Plugin host (v0.3+):** `wasmtime` WASM runtime + typed TypeScript SDK mirroring Atom's `atom.workspace` / `atom.commands` ergonomics.

### Process Model

```
┌────────────────────────────────────────┐
│ atomio.app (Rust, single binary)       │
│ ├─ gpui UI thread (Metal)              │
│ ├─ editor_core (rope, selections)      │
│ ├─ tree-sitter incremental parse       │
│ ├─ AI agent runtime                    │
│ └─ IPC broker                          │
└────────────┬───────────────────────────┘
             │ stdio / unix sockets
   ┌─────────┼──────────┬──────────┐
   ▼         ▼          ▼          ▼
 LSP      Node ext    Git       (later: WASM)
 procs    host        worker
```

### Crate Layout

```
atomio/
├── crates/
│   ├── atomio/          # macOS app entry
│   ├── editor_core/     # buffer + selections
│   ├── editor_view/     # gpui rendering
│   ├── language/        # tree-sitter + LSP
│   ├── ai/              # agent runtime + providers
│   ├── ext_host_node/   # VS Code compat lane
│   ├── workspace/       # panes, file tree, command palette
│   └── theme/           # theme loader (TextMate + Atom .less compat)
├── sdk-ts/              # TypeScript SDK (v0.3+)
└── docs/
```

---

## What We Take From `atom/atom`

Inspiration only, not a fork base:

- Brand, name, logo, philosophy.
- Tree-sitter.
- Default keybindings and command palette UX.
- Package API surface as reference for the TS SDK.
- Atom syntax/UI theme format — load old Atom themes in v0.2.

Explicitly dropped: CoffeeScript, Electron, the `text-buffer` C++ addon, `apm`, `space-pen`.

---

## Milestones

Dates are deliberately omitted. Ship when it's ready.

### v0.0 — "Hello, atom"

Prove the stack works end-to-end.

- Repo skeleton, MIT license, CI with codesign + notarize stubs.
- gpui hello-world window, single buffer, monospace text rendering.
- File open / save via native dialog.
- Cursor, selection, basic editing, undo/redo.
- Brand: logo, README, minimal landing page on GitHub Pages.

### v0.1 — "It edits"

A usable text editor for small files. Dogfoodable for markdown and config.

- `ropey` buffer, large-file load (>100MB without stall).
- tree-sitter incremental highlighting (5–10 grammars: rust, ts, py, md, json, toml, yaml, html, css, sh).
- Command palette + keybinding system (Atom-compatible JSON format).
- File tree pane, fuzzy file finder.
- One built-in theme (light + dark, "Atomio One").

### v0.2 — "It speaks LSP"

Real coding workflow.

- LSP client: completions, hover, go-to-definition, diagnostics, format-on-save.
- DAP-stub (IPC plumbing, no debugger UI yet).
- Multi-pane splits, tabs.
- Git gutter (added/modified/removed).
- Atom theme compat shim.

### v0.3 — "It has an ecosystem"

VS Code users can switch.

- Node sidecar VS Code extension host. Curated set of 20–30 popular extensions (eslint, prettier, gitlens, language packs).
- Settings UI (JSON-first, GUI second).
- Auto-update (Sparkle or equivalent).

### v0.4 — "It's an AI editor"

The differentiator.

- Inline AI panel tied to the open file.
- Inline edit with diff preview.
- Pluggable providers: Anthropic (default), OpenAI, Ollama.
- Agent tool access: read files, run shell, run tests, apply edits.

### v1.0 — "It's hackable"

Deliver on the Atom promise.

- WASM plugin runtime + TypeScript SDK.
- `atomio.workspace.observeTextEditors(...)`-style API.
- Plugin marketplace (static GitHub-hosted index, no server).
- Documentation site.

### Post-v1.0 (out of scope for year one)

- Linux + Windows ports.
- Collaborative editing (CRDT layer activation).
- Native debugger UI.
- Self-hosted plugin marketplace.

---

## Won't-Do List

Tempting, but explicitly out of scope in year one:

- Cross-platform (macOS only).
- Custom rendering engine (use `gpui`).
- Custom extension format competing with VS Code's (compat first).
- Vim mode (community plugin territory).
- Notebook / Jupyter UI.
- Built-in terminal.
- Self-hosted sync, accounts, or cloud services.
- Mobile companion app.

Each "no" is what makes the "yes" list achievable.

---

## Verification Targets (by v0.4)

- Cold start <150ms on M-series Mac (`hyperfine`).
- 120fps sustained scroll on a 100k-line file (gpui frame timing).
- Open and highlight a 500MB log file without UI stall.
- Load and run `vscode-eslint` unmodified via the Node sidecar.
- AI inline edit round-trip (prompt → diff → apply) <2s on a 200-line file.
- Single signed, notarized `.dmg` produced by CI on every tag.

---

## Risks & Mitigations

| Risk | Mitigation |
|---|---|
| `gpui` too coupled to Zed to extract cleanly | Validate gpui-as-a-dependency before v0.0 ships. Fallback: `floem`. |
| Apple notarization headaches | Codesign + notarize CI in place from v0.0. |
| VS Code extension host scope creep | Curate 20 extensions, not 2000. Document the unsupported surface. |
| Yet-another-editor fatigue | The AI + ecosystem combo is the differentiator; withhold major launches until v0.4 is real. |
| "Atom" trademark | Use **atomio**. Acknowledge inspiration, claim no affiliation. |

---

## Brand

- Project name: **atomio**. Distinct, searchable, evokes Atom without claiming it.
- Logo: stylized atom orbital, echoing Atom's mark but unmistakably new.
- Tagline candidates:
  - *"The editor you can rewrite on a Sunday afternoon."*
  - *"Atom's soul. VS Code's ecosystem. Zed's speed."*
  - *"Hackable to the core. Again."*
