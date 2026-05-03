# CLAUDE.md

Context for Claude Code working in this repository. Read this first.

## What this project is

**atomio** is a native, GPU-accelerated debugger for React Native and Expo apps, built from scratch in Rust on gpui + Metal. It connects to running Expo apps via the Chrome DevTools Protocol (CDP) and Hermes, providing breakpoints, variable inspection, call stack navigation, network monitoring, console output, React component tree, and a built-in code editor -- all in a single macOS window.

macOS / Apple Silicon only through v1.0. MIT, no CLA. See `README.md` for the pitch and `docs/ROADMAP.md` for the milestone breakdown.

**Current state:** v0.0 is done. The code editor works (buffer, selection, undo/redo, clipboard, syntax highlighting, command palette). v0.1 ("It connects") is the active focus: CDP client, Metro discovery, Hermes attach, console log stream.

## Repo layout

```
atomio/
+-- Cargo.toml                  # virtual workspace
+-- crates/
|   +-- atomio/                 # macOS app entry -- gpui window + pane layout
|   |   +-- Cargo.toml
|   |   +-- src/main.rs
|   +-- editor_core/            # buffer + selection + state model (no GUI)
|   |   +-- Cargo.toml
|   |   +-- src/
|   |       +-- lib.rs          # module declarations + re-exports only
|   |       +-- buffer.rs       # ropey wrapper, on-disk identity, line/col math
|   |       +-- command.rs      # command palette registry + fuzzy matcher
|   |       +-- selection.rs    # cursor/selection primitive
|   |       +-- state.rs        # EditorState: buffer + selection + undo/redo
|   +-- language/               # tree-sitter parsing + token classification
|       +-- Cargo.toml
|       +-- src/lib.rs          # highlight_rust(&str) -> Vec<Span>
+-- docs/
|   +-- ROADMAP.md              # milestones v0.0 -> v1.0 + won't-do list
|   +-- dev-practices.md        # branching, versioning, CI policy
|   +-- github-setup.md         # manual GitHub UI setup steps
+-- .github/
|   +-- workflows/              # ci, audit, nightly, release, pr-title, release-please, cut-prerelease
|   +-- dependabot.yml
+-- release-please-config.json
+-- .release-please-manifest.json
+-- README.md
+-- CHANGELOG.md
```

Crates that **don't exist yet** but appear in the roadmap (do not invent files for these without asking): `debugger`, `inspector`, `network`, `console`, `react_tree`, `workspace`.

## Architecture rules

1. **Editing logic lives in `editor_core`, never in `crates/atomio/src/main.rs`.** `main.rs` is purely a translation layer: gpui events -> `EditorState` method calls -> `cx.notify()`. If you find yourself writing rope or selection logic in the UI crate, stop and put it in `editor_core::state` with a unit test.
2. **`editor_core` has zero gpui / AppKit / async dependencies.** It must stay testable with plain `cargo test -p editor_core` and no display.
3. **Undo model.** Every mutation in `EditorState` pushes an inverse `Edit` onto the undo stack and clears redo. Don't bypass this by calling `Buffer::insert` / `Buffer::remove` directly from outside `state.rs`.
4. **gpui version is pinned to `0.2`** from crates.io. Don't switch to a git pin or vendor a fork without explicit approval.
5. **Native dialogs (`rfd`) must be triggered from inside `cx.spawn` / the gpui run loop, never before `Application::new().run()`.** Calling rfd at startup races NSApplication initialization and crashes AppKit (`Ivar platform not found on NSKVONotifying_NSApplication`). This already burned us once.
6. **Debugger protocol logic lives in `debugger`, not in UI crates.** The `debugger` crate owns the CDP WebSocket connection, breakpoint state, step control. UI crates read its model and send commands. Same pattern as editor_core: model is pure, UI is thin.
7. **Each inspector domain gets its own crate** (`inspector`, `network`, `console`, `react_tree`). Keep them independently testable with mock CDP messages.

## Development practices

Read `docs/dev-practices.md` for the full policy. Highlights:

- **Trunk-based.** One `main` branch. Feature work happens on short-lived `feat/*`, `fix/*`, `chore/*`, `docs/*` branches that get squash-merged into `main`. No long-lived `dev` / `preview` / `release` branches.
- **Conventional Commits, enforced on PR titles.** The PR title becomes the squash-merge commit message and is read by release-please. Allowed types: `feat`, `fix`, `perf`, `refactor`, `docs`, `test`, `build`, `ci`, `chore`. Allowed scopes: `atomio`, `editor_core`, `language`, `debugger`, `inspector`, `network`, `console`, `react_tree`, `workspace`, `docs`, `ci`, `release`, `dev-practices`, `deps`, `deps-dev`. Subject must start lowercase, no trailing period.
- **Release channels are tags, not branches.** Nightly (`nightly` moving tag), alpha/beta (`vX.Y.Z-alpha.N`), prod (`vX.Y.Z`). Three GitHub Environments (`nightly`, `staging`, `production`) gate codesign + notarize secrets. release-please automates version bumps + CHANGELOG.
- **`bump-minor-pre-major: true`** until v1.0.
- **No emojis in documentation.** Anywhere. README, ROADMAP, dev-practices, CLAUDE.md, comments, commit messages -- none.
- **Branch protection on `main` is on (or about to be).** You cannot push directly to main. Always create a branch.

## Mandatory checks before committing

```sh
cargo fmt --all
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace --locked
```

CI runs the same three on every PR. If clippy fires, **fix the underlying issue** -- do not paper over it with `#[allow(...)]` unless you can justify it in the commit message.

When adding a new feature to any model crate (`editor_core`, `debugger`, `inspector`, etc.), **add a unit test in the same module** in the same commit. The current test count is the floor, never the ceiling.

## Build / run

```sh
cargo run -p atomio                     # opens with the greeting buffer
cargo run -p atomio -- path/to/file.ts  # opens that file
```

Requires full Xcode (not just CLT) -- gpui compiles Metal shaders at build time and needs the `metal` compiler bundled with Xcode.

## Keybindings (current)

| Key | Action |
|---|---|
| Cmd+O | Open file (native dialog) |
| Cmd+S | Save (native save-as if no path) |
| Cmd+Z | Undo |
| Cmd+Shift+Z | Redo |
| Cmd+C / Cmd+X / Cmd+V | Copy / Cut / Paste |
| Cmd+A | Select all |
| Cmd+Shift+P | Command palette |
| Arrows | Cursor movement |
| Shift+Arrows | Extend selection |
| Home / End / Cmd+Left / Cmd+Right | Line start / end |
| Backspace / Delete | Delete previous / next char |
| Escape | Dismiss palette |
| Printable keys | Insert at caret |

All bound via `actions!` + `KeyBinding::new` in the `atomio` key context.

## Workflow when adding a feature

1. Create a branch (`feat/<thing>`, `fix/<thing>`, etc.).
2. If it's model logic: write the test first or alongside, then the code.
3. If it's UI: keep the UI crate thin -- call into model crates.
4. Run the three checks above.
5. Smoke-run `cargo run -p atomio` -- gpui issues only show up at runtime.
6. Commit with a Conventional Commit message. PR title must follow the same.
7. Push the branch and open a PR with an **explicit** title:
   ```sh
   gh pr create --title "$(git log -1 --pretty=%s)" --body "..."
   ```
8. Squash-merge.

## What NOT to do

- Don't add cross-platform code, Linux/Windows shims, or `cfg(not(target_os = "macos"))` branches. macOS-only is a hard constraint through v1.0.
- Don't introduce a custom rendering layer -- use `gpui` primitives.
- Don't add general-purpose editor features (LSP, git integration, terminal). The editor exists to support the debugging workflow.
- Don't add new dependencies casually. Each one is a long-term commitment.
- Don't bypass the dirty flag -- always go through `EditorState`.
- Don't write documentation files (`*.md`) or READMEs unless explicitly asked.
- Don't put emojis in any file.
- Don't implement custom debugger protocols -- build on CDP.

## Useful pointers

- **gpui source** is vendored at `~/.cargo/registry/src/index.crates.io-*/gpui-0.2.2/`. When the API surface is unclear, grep there before guessing.
- **release-please** reads `release-please-config.json` and `.release-please-manifest.json`. Each crate inside `crates/` declares its own explicit `version = "..."`.
- **PR title scopes**: if you add a new top-level crate, also add it to the `scopes:` list in `.github/workflows/pr-title.yml`, otherwise PRs touching it will be rejected by the validator.
- **CDP protocol reference**: https://chromedevtools.github.io/devtools-protocol/
- **Hermes CDP support**: https://reactnative.dev/docs/hermes#debugging-js-on-hermes-using-google-chromes-devtools
- **Metro bundler**: `/json/list` endpoint returns debuggable targets. `/symbolicate` for source maps.
