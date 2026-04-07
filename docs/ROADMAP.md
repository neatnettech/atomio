# Atomio — Reviving Atom as a Solo, After-Hours, macOS-First Editor

## Context

Atom was archived by GitHub in Dec 2022. Its core failure was performance: an Electron + Node + CoffeeScript-era stack that couldn't keep up with VS Code (better Electron engineering + LSP ecosystem) or Zed (Rust + GPU rendering + native concurrency).

The goal of **atomio** is to revive the Atom *brand and philosophy* ("hackable to the core") on a modern, native, GPU-accelerated foundation — built **solo, after hours, macOS-only, fully open source**. The pitch is the union of three audiences:

- People who loved **Atom** (hackability, soul, community).
- People who love **VS Code** (extension ecosystem, "it just works").
- People who love **Zed** (native speed, AI as a first-class citizen).

> *"Atom's soul, VS Code's ecosystem, Zed's speed — and the AI agent lives inside the editor, not as a sidecar."*

**Current state:** `/Users/piotrpestka/github/atomio` is empty. There is no fork yet. This plan treats it as greenfield. Continuing the original Atom Electron/CoffeeScript codebase would inherit exactly the problems we want to escape — we draw inspiration, brand, and select assets only.

## Solo / After-Hours Reality Check

A solo dev cannot out-build the Zed team. The plan must be ruthless about:

1. **Stand on giants.** Adopt `gpui` (Apache-2.0, Zed's UI framework) directly. Do not roll our own renderer. This is the single most important decision — it shaves ~12 months off v0.1.
2. **macOS only, Apple Silicon only.** No cross-platform cost. Use `gpui`'s Metal backend, AppKit integration, native menus, native file dialogs. Codesigning + notarization in CI from day one.
3. **Compatibility lane > native rewrite.** Ship a VS Code extension host as a Node sidecar in v0.1 so the ecosystem exists on launch day. Native WASM plugin tier comes later.
4. **No collab, no debugger, no Linux/Windows in year one.**
5. **Public in the open from day one.** Solo OSS projects live or die by narrative and momentum, not code quality alone.

## Target Architecture

### Stack
- **Language:** Rust.
- **UI / rendering:** `gpui` (Zed's framework, Apache-2.0). Metal backend.
- **Text buffer:** rope + CRDT-ready primitives. Start with `ropey`; design the API so a CRDT layer can slot in later for collab.
- **Syntax:** `tree-sitter` (Atom's own invention — reclaim it as brand).
- **Language intelligence:** LSP via `tower-lsp`-style client. One process per language server.
- **AI:** First-class inline panel, pluggable provider (Anthropic, OpenAI, Ollama). Default to Anthropic. No vendor lock-in.
- **Plugin host (v0.1):** Node sidecar running unmodified VS Code extensions via the documented Extension Host protocol where possible.
- **Plugin host (v0.3+):** WASM runtime (`wasmtime`) + a typed TypeScript SDK that mirrors Atom's beloved `atom.workspace` / `atom.commands` ergonomics.

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

### Crate Layout (when implementation starts)
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

## What to Take From `atom/atom`

Read-only inspiration, not a fork base:
- **Brand, name, logo, philosophy** ("hackable to the core").
- **Tree-sitter** (already ours).
- **Default keybindings + command palette UX.**
- **Package API surface** as *reference* for the TS SDK ergonomics.
- **Atom's syntax/UI theme format** — support loading old Atom themes in v0.2 as a community goodwill move.

Do **not** carry forward: CoffeeScript, Electron, the old `text-buffer` C++ addon, `apm`, `space-pen`.

## Roadmap & Milestones (Solo, After-Hours)

Each milestone is sized for **~2–3 months of evening/weekend work** by one person. Dates are deliberately omitted — slip happens, ship when it's ready.

### v0.0 — "Hello, atom" (Month 0–2)
**Goal:** prove the stack works end-to-end. Public repo, public README, no users yet.
- Repo skeleton, MIT license, CI (codesign + notarize on tag).
- gpui hello-world window, single buffer, monospace text rendering.
- File open / save via native dialog.
- Cursor, selection, basic editing, undo/redo.
- Brand: logo, README, landing page (one HTML file on GitHub Pages).
- **Public artifact:** dev log post #1, "I'm reviving Atom."

### v0.1 — "It edits" (Month 2–5)
**Goal:** a usable text editor for small files. Dogfoodable for writing markdown and config.
- `ropey` buffer, large-file load (>100MB without stall).
- tree-sitter incremental highlighting (5–10 grammars: rust, ts, py, md, json, toml, yaml, html, css, sh).
- Command palette + keybinding system (Atom-compatible JSON format).
- File tree pane, fuzzy file finder (à la `Cmd-P`).
- One built-in theme (light + dark, "Atomio One").
- **Public artifact:** v0.1 release on GitHub, HN "Show HN: I'm reviving Atom, here's the first build."

### v0.2 — "It speaks LSP" (Month 5–8)
**Goal:** real coding workflow.
- LSP client: completions, hover, go-to-definition, diagnostics, format-on-save.
- DAP-stub (no debugger UI yet, but the IPC plumbing).
- Multi-pane splits, tabs.
- Git gutter (added/modified/removed).
- Atom theme compat shim.
- **Public artifact:** dev log "Atomio now runs rust-analyzer faster than VS Code."

### v0.3 — "It has an ecosystem" (Month 8–12)
**Goal:** the moment users from VS Code can switch.
- **Node sidecar VS Code extension host.** Run a curated set of 20–30 popular VS Code extensions unmodified (eslint, prettier, gitlens, language packs).
- Settings UI (JSON-first, GUI second — Atom heritage).
- Auto-update (Sparkle framework or equivalent).
- **Public artifact:** "Atomio runs your VS Code extensions" launch post.

### v0.4 — "It's an AI editor" (Month 12–15)
**Goal:** the Zed-killer feature.
- Inline AI panel: chat with the agent in a sidebar tied to the open file.
- Inline edit ("/edit this function to..."), with diff preview.
- Pluggable providers: Anthropic (default), OpenAI, Ollama.
- Agent has tool access to: read files, run shell, run tests, apply edits.
- **Public artifact:** demo video. This is the launch that gets traction.

### v1.0 — "It's hackable" (Month 15–18)
**Goal:** deliver on the Atom promise.
- WASM plugin runtime + TypeScript SDK.
- `atomio.workspace.observeTextEditors(...)`-style API.
- Plugin marketplace (static GitHub-hosted index, no server).
- Documentation site.
- **Public artifact:** v1.0 launch.

### Post-v1.0 (deferred, explicitly out of scope for year one)
- Linux + Windows ports.
- Collaborative editing (CRDT layer activation).
- Native debugger UI.
- Self-hosted plugin marketplace.

## Backlog (Won't-Do List — important for solo focus)

These are tempting and **must be refused** in year one:
-Cross-platform (macOS only).
-Custom rendering engine (use `gpui`).
-Custom extension format that competes with VS Code's (compat first).
-Vim mode (LSP-quality vim is a project of its own — community plugin later).
-Notebook / Jupyter UI.
-Built-in terminal (use Warp/iTerm — add later).
-Self-hosted sync / accounts / cloud anything.
-Mobile companion app.

Each "no" is what makes the "yes" list achievable solo.

## Marketing Strategy

The marketing is the project. A solo OSS editor with no narrative dies; a mediocre one with a great story gets contributors.

### Audience Segments & Hooks
| Audience | What they want | Atomio hook |
|---|---|---|
| Ex-Atom users (nostalgic) | Hackability, soul, community | "Atom is back. Hackable to the core, again." |
| VS Code power users | Their extensions, their muscle memory | "Bring your VS Code extensions. Leave the lag." |
| Zed users / perf nerds | Native speed, low latency | Public benchmarks vs VS Code & Zed every release |
| AI-first devs | Agent integration, not a chat sidebar | "The agent edits your code, not your patience." |
| OSS / indie hackers | Permissive license, single maintainer story | Build-in-public dev log, MIT license, no CLA |

### Channels (cheap, solo-friendly)
1. **Build-in-public dev log.** Weekly post on a personal blog + cross-post to Twitter/X, Mastodon, Bluesky. Show GIFs of frame timings, tree-sitter parses, AI edits.
2. **GitHub README as landing page** until v0.3. Then a one-page site on GitHub Pages.
3. **Hacker News** — one Show HN at v0.1, one at v0.4 (AI launch), one at v1.0. Don't burn HN on incomplete builds.
4. **Lobste.rs, /r/rust, /r/programming** — for technical milestones (gpui adoption, tree-sitter perf).
5. **Demo videos.** 60–90 seconds, no voiceover, just showing the editor being faster than VS Code at a real task. One per minor release.
6. **Benchmarks repo.** Public, reproducible, vs VS Code and Zed. Numbers travel.
7. **Discord or Zulip** — only after v0.3. Earlier is wasted maintainer energy.
8. **Conference talk** at v1.0 — RustConf, Strange Loop successor, or local meetups.

### Naming & Brand
- Keep **atomio** as the project name. It's distinct, searchable, evokes Atom without claiming it.
- Logo: a stylized atom orbital, echoing Atom's original mark but unmistakably new.
- Tagline candidates (pick one and stick to it):
  - *"The editor you can rewrite on a Sunday afternoon."*
  - *"Atom's soul. VS Code's ecosystem. Zed's speed."*
  - *"Hackable to the core. Again."*

### Community Posture
- **MIT license.** No CLA. PRs welcome from day one even if rarely merged in v0.0–v0.2.
- **Public roadmap** (this file, in `/docs`).
- **Public dev log.** Honesty about being solo and after-hours is a feature, not a flaw — it's the Atom-era ethos.
- **Issue triage cadence:** weekly, max 30 min. Solo means saying no a lot.

## Critical Risks & Mitigations

| Risk | Mitigation |
|---|---|
| Solo burnout | Hard rule: no work on atomio Sunday evenings. Ship slow, ship steady. |
| `gpui` is too coupled to Zed to extract cleanly | Spend the **first weekend** validating gpui-as-a-dependency. If it fails, fall back to `floem`. This is the project's first go/no-go. |
| Apple notarization headaches | Set up codesign + notarize CI in v0.0, before there's anything to ship. |
| VS Code extension host is harder than it looks | Curate 20 extensions, not 2000. Document the unsupported surface. |
| Yet-another-editor fatigue | The AI + ecosystem combo is the differentiator. Don't launch publicly until v0.4 demo is real. |
| "Atom" trademark / GitHub pushback | Use **atomio**, not Atom. Acknowledge inspiration in README, claim no affiliation. |
| Competing with funded teams | Don't. Compete on soul, hackability, and the build-in-public story. |

## Verification (architecture-level, not code-level)

The architecture is "right" if, at v0.4:
- Cold start <150ms on M-series Mac (`hyperfine`).
- 120fps sustained scroll on a 100k-line file (gpui frame timing).
- Open + highlight a 500MB log file without UI stall.
- Load and run `vscode-eslint` unmodified via the Node sidecar.
- AI inline edit round-trip (prompt → diff preview → apply) <2s on a 200-line file with Claude.
- Single signed, notarized `.dmg` produced by CI on every tag.

## Critical Files / First Steps (when implementation begins)

This plan does not implement code. When implementation begins, the first commits should be, in order:
1. `LICENSE` (MIT), `README.md` (manifesto), `.github/workflows/ci.yml` (codesign + notarize stub).
2. `Cargo.toml` workspace + `crates/atomio/src/main.rs` — gpui hello-window.
3. `crates/editor_core` — rope buffer + selection model (lean on `ropey`).
4. **Go/no-go checkpoint:** Is `gpui` workable as an external dep?

## Open Questions for You

(none blocking — answers can shape v0.0 but not the overall plan)

1. Anthropic-hosted `/docs` site, or stay README-only until v0.3?
2. Personal-brand build-in-public, or anonymous/project-only account?
3. Donations / GitHub Sponsors enabled at v0.0 or only after v1.0?
