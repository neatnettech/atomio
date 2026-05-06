# atomio -- Visual Design Spec

This document is the canonical reference for atomio's visual design. The interactive prototype lives in [`docs/design/handoff/`](design/handoff/) -- HTML/CSS/JSX bundle exported from Claude Design. Open `docs/design/handoff/project/atomio.html` in a browser to preview the live mock; all the tokens and structure below are extracted from the same source.

When implementing UI in `crates/atomio`, this spec wins. Rendering primitives (gpui) differ, but **the visual output must match the prototype pixel-for-pixel**: same colors, same spacing, same layout, same typography.

---

## Design language

**Inspiration:** Zed's minimalism + Atom's warmth, dark-only.

**Killer pitch:** All-in-one code editor + debugger for Expo / React Native. Everything at hand. No browser tabs, no Electron.

**Aesthetic rules:**

- Backgrounds are a **near-black slate** (`#0c0e11` -> `#14171c`), never pure black, slight cool tint.
- Surfaces stack via subtle warm tints, never just lighter grays.
- Accent green `#3ecf8e` for active / run states.
- Orange `#ffb454` warnings, red `#ff6b6b` errors, blue `#6cb3ff` info, magenta `#d18cff` keywords.
- Syntax palette is calm and low-saturation, not rainbow. Comments very dim, italic.
- Typography is Apple-native: SF Pro for UI, SF Mono for code.

---

## Color tokens

Source of truth: [`docs/design/handoff/project/theme.css`](design/handoff/project/theme.css).

### Surfaces (cool dark slate)

| Token       | Hex       | Use                                     |
|-------------|-----------|-----------------------------------------|
| `--bg-0`    | `#0c0e11` | Deepest -- outside chrome (desktop bg)  |
| `--bg-1`    | `#11141a` | Main app surface                        |
| `--bg-2`    | `#161a21` | Sidebar / panels                        |
| `--bg-3`    | `#1c2129` | Selected rows, hovered cards            |
| `--bg-4`    | `#232934` | Lifted elements, overlays               |
| `--bg-tooltip` | `#2a3140` | Tooltips                             |

### Lines & dividers

| Token      | Value                       |
|------------|-----------------------------|
| `--line-1` | `rgba(255,255,255,0.05)`    |
| `--line-2` | `rgba(255,255,255,0.08)`    |
| `--line-3` | `rgba(255,255,255,0.14)`    |

### Text

| Token    | Hex       | Use                            |
|----------|-----------|--------------------------------|
| `--tx-1` | `#e8ebf0` | Primary text                   |
| `--tx-2` | `#b6bdc9` | Secondary                      |
| `--tx-3` | `#7a8290` | Tertiary / labels              |
| `--tx-4` | `#525866` | Dim / placeholders             |
| `--tx-5` | `#363b46` | Very dim -- gutter inactive    |

### Brand & semantic

| Token            | Hex                     | Use                                 |
|------------------|-------------------------|-------------------------------------|
| `--accent`       | `#3ecf8e`               | Run state, active tab, focus ring   |
| `--accent-soft`  | `rgba(62,207,142,0.14)` | Active row background               |
| `--accent-line`  | `rgba(62,207,142,0.30)` | Active border                       |
| `--warn`         | `#ffb454`               | Warnings                            |
| `--warn-soft`    | `rgba(255,180,84,0.14)` |                                     |
| `--error`        | `#ff6b6b`               | Errors                              |
| `--error-soft`   | `rgba(255,107,107,0.13)`|                                     |
| `--info`         | `#6cb3ff`               | Info                                |
| `--info-soft`    | `rgba(108,179,255,0.13)`|                                     |
| `--magenta`      | `#d18cff`               | Keywords                            |

### Syntax (calm, Zed One Dark adjacent)

| Token     | Hex       | Use                              |
|-----------|-----------|----------------------------------|
| `--sx-pl` | `#c8cfd9` | Plain identifiers                |
| `--sx-kw` | `#d18cff` | Keywords (import, return, const) |
| `--sx-fn` | `#6cb3ff` | Functions                        |
| `--sx-str`| `#9bd87f` | Strings                          |
| `--sx-num`| `#ffb454` | Numbers                          |
| `--sx-comp`| `#ff8a8a`| JSX components                   |
| `--sx-prop`| `#ffd58a`| JSX props / attributes           |
| `--sx-type`| `#6cb3ff`| Types                            |
| `--sx-com`| `#4f5663` | Comments -- very dim, italic     |
| `--sx-op` | `#97a0ad` | Operators                        |
| `--sx-brk`| `#7a8290` | Brackets                         |

---

## Typography

| Token           | Stack                                                       |
|-----------------|-------------------------------------------------------------|
| `--font-ui`     | -apple-system, BlinkMacSystemFont, 'SF Pro Text', sans-serif|
| `--font-mono`   | ui-monospace, 'SF Mono', 'JetBrains Mono', 'Menlo', mono    |
| `--font-display`| -apple-system, BlinkMacSystemFont, 'SF Pro Display', sans   |

### Font sizes

| Token    | Size     |
|----------|----------|
| `--fs-xs`| 10.5px   |
| `--fs-sm`| 11.5px   |
| `--fs-md`| 12.5px   |
| `--fs-lg`| 13.5px   |
| `--fs-xl`| 15px     |
| `--code-fs` | 12.5px |
| `--code-lh` | 20px   |

### Radii

| Token        | Value |
|--------------|-------|
| `--r-sm`     | 4px   |
| `--r-md`     | 6px   |
| `--r-lg`     | 10px  |
| `--r-xl`     | 14px  |
| `--r-window` | 12px  |

---

## Window chrome

- macOS-style centered window in a desktop background. Subtle radial gradients (green top-left, purple bottom-right) plus a 28px grid overlay at 1.8% opacity.
- Window radius `12px`, drop shadow stacked: thin white inner highlight + heavy blur cast.
- 36px titlebar with traffic lights (red `#ff5f57`, yellow `#febc2e`, green `#28c941`), centered document title with branch suffix in monospace dim.
- Titlebar gradient: `linear-gradient(180deg, #1a1f27 0%, #14181f 100%)`.

---

## Layout

The window splits into three vertical regions:

```
+---------------------------------------------------------------+
| Titlebar (36px)                                               |
+----+--------------------------------+-------------------------+
|    |                                |                         |
| A  |   Editor (file tree + tabs +   |   Right dock            |
| c  |   code + minimap)              |   (Debugger / Console / |
| t  |                                |   Simulator / Components|
| i  |                                |   / Profiler)           |
| v  |                                |                         |
| i  |                                |                         |
| t  |                                |                         |
| y  |                                |                         |
+----+--------------------------------+-------------------------+
| Status bar (compact)                                          |
+---------------------------------------------------------------+
```

### Activity bar (left rail, Zed-style)

- 48px wide, `var(--bg-2)`, single thin right border `var(--line-1)`.
- 36x36 icon items, 6px radius. Hover lifts to `var(--bg-3)`. Active gets `--accent` color + `--accent-soft` background + 2px accent indicator on left edge.
- Items: file tree, debugger, simulator, components, profiler, console, search.

### Editor

- File tree pane (resizable) with outline at bottom.
- Tab bar above editor: SF Pro semibold 11.5px, active tab uses `--bg-1` with thin top border highlight.
- Code area uses SF Mono 12.5px / 20px line-height.
- Gutter shows line numbers in `--tx-5` (dim) / `--tx-2` (active line).
- Breakpoint dots: clickable in gutter -- `--accent` filled circle when set.
- Paused-line indicator: full-row `--accent-soft` background + arrow on the gutter.
- Inline value pills appear after expressions on the paused line: pill background `--bg-4`, text `--tx-2`, mono.
- Minimap on the right edge of editor (60px wide, 8% opacity overlay).

### Right dock

Switches between five views (radio in tweaks panel decides which is mounted):

- **Debugger** -- control bar (continue / step over / step into / step out / pause), call stack list, variables tree (Local / Closure / Global tabs), watch expressions, breakpoints with hit counts.
- **Console** -- streaming logs with level filters (LOG / INFO / WARN / ERROR), evaluator input pinned at bottom.
- **Simulator** -- device picker, live phone mock frame, perf chips (FPS / memory / network).
- **Components** -- React DevTools-style component tree with props/state inspector pane.
- **Profiler** -- frame chart on top, flame graph below, suggestion banner at the bottom.

### Status bar

- Compact, 22px tall, `var(--bg-2)`.
- Left: connection dot (green / orange / red / grey) + status text.
- Right: cursor position (ln/col), file size, encoding, language, branch.

---

## Overlays

### Command palette (Spotlight-style)

- Triggered by Cmd+K or Cmd+Shift+P.
- Floating panel, ~440px wide, centered horizontally, anchored ~80px from top.
- Background `var(--bg-4)`, 8px radius, drop shadow + `var(--line-3)` border.
- Input field with `>` prompt, results list grouped by category (File, Edit, Debug, View).
- Selected row: `var(--bg-3)` background, accent text on category label.
- Keyboard-driven: arrows navigate, enter dispatches, escape dismisses.

### Project picker / launch screen

- Shown when no project is loaded.
- Recent projects list (path, last opened), action buttons (Open, Clone, New), "simulator-ready" hint.
- Same dark chrome, content centered max-width ~720px.

### Tweaks panel (dev-only)

- Right-side overlay for testing variations during prototype phase. Not shipped to users.
- Holds: accent color, right-panel choice, sim toggle, jump to launch, toggle paused state.

---

## Interactions / shortcuts

- F5 -- toggle running ↔ paused
- Cmd+K / Cmd+Shift+P -- command palette
- Cmd+P -- file finder (subset of palette)
- Cmd+Shift+D -- connect to Metro
- Cmd+Shift+C -- toggle console
- Cmd+Shift+O -- open first script
- Click breakpoint dot in gutter -- toggle
- Click line number -- toggle breakpoint

(See [`docs/design/handoff/project/app.jsx`](design/handoff/project/app.jsx) for the prototype wiring.)

---

## Implementation notes

When porting to gpui:

- Map CSS tokens to constants in a single `theme.rs` module. Don't sprinkle hex literals across the codebase.
- Use gpui's flexbox layout to mirror the prototype's grid. Activity bar = fixed-width column; editor + dock = `flex: 1`; status bar = fixed-height row.
- Drop shadows use `gpui::ShadowStyle`; keep the layered approach (thin white inner + heavy outer blur).
- Syntax colors plug into `language::HighlightKind` mapping in the renderer (already partially done in `crates/atomio/src/main.rs`).

The handoff bundle's JSX is reference, not source -- don't import or run it. Read it like you would a Figma file.

---

## Provenance

- Designed in Claude Design (`claude.ai/design`) on 2026-05-06 by Piotr Pestka.
- Bundle imported via API: `https://api.anthropic.com/v1/design/h/psfRkSUeZwSq3tschyeeag`.
- Original chat transcript: [`docs/design/handoff/chats/chat1.md`](design/handoff/chats/chat1.md).
- This spec is the **target**, not the current state. As implementation lands, link PRs that bring atomio closer to it.
