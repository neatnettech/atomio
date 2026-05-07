# Expo iOS Debugger

_Started 2026-05-06 09:18 UTC_

---

## User

i am currently working on the expo / react native debugger application for ios only. i would like to design the app to be intuitive and user friendly. a simple code editor, debugger, future embedded ios emulator in the application, all of this in a neat and simplistic design,. my inspiration was atom and zed. lets have a look what we can cook

## Assistant

I'll help you design an iOS-focused Expo/React Native debugger app inspired by Atom and Zed. Before I start designing, let me explore the design system and ask some clarifying questions.

_[tool: list_files]_

The design system appears to be empty. Let me ask the right questions to nail this down before designing.

_[tool: questions_v2]_

## User

Questions answered:
- platform_form_factor: macOS desktop app (debugs iOS apps — like Atom/Zed)
- primary_screens: Project picker / launch screen, Main editor + file tree, Debugger panel (breakpoints, call stack, vars), Embedded iOS simulator/emulator view, Component tree / React DevTools, Performance / profiler, Command palette (⌘K style), Console / logs
- accent_color: 2
- aesthetic: A blend — Zed's minimalism with a little Atom warmth
- theme: Dark only
- typography: SF Mono + SF Pro (very Apple-native)
- fidelity: Clickable prototype — tabs/panels actually switch, mock data
- variations: Decide for me
- panel_layout: Decide for me
- name_brand: atomio
- killer_feature: a truly all in one code editor and debugger with all you need at hand

## Assistant

Got it. **atomio** — macOS desktop debugger for Expo/RN, Zed-minimal with Atom warmth, dark, green accent (#3ecf8e), SF Pro/Mono, clickable prototype. Killer pitch: all-in-one, everything at hand.

Let me set up and start building.

_[tool: update_todos]_

_[tool: copy_starter_component]_

_[tool: copy_starter_component]_

_[tool: read_file]_

The starter is light/Tahoe-styled — for a dark dev tool I'll just use the traffic lights and build my own dark chrome. Let me build the app.

I'll structure it as:
- `Atomio.html` — main entry
- `app.jsx` — main App component, routing between views
- `editor.jsx` — file tree + editor + tabs
- `panels.jsx` — debugger, console, simulator, devtools, profiler
- `palette.jsx` — command palette + project picker
- `data.js` — mock files, logs, breakpoints, components

Let me start with the main HTML and data layer.

_[tool: write_file]_

Now the design tokens / theme stylesheet:

_[tool: write_file]_

_[tool: snip]_

Now let me build the icon set + shared shell components:

_[tool: write_file]_

_[tool: snip]_

Now the editor view (file tree, tabs, code with breakpoints + active line + inline values):

_[tool: write_file]_

_[tool: write_file]_

_[tool: snip]_

Now command palette + project picker + main app:

_[tool: write_file]_

_[tool: snip]_

Now the main App that wires everything, plus tweaks:

_[tool: write_file]_

_[tool: snip]_

Now the main HTML entry. Let me check the tweaks panel API quickly:

_[tool: grep]_

I see — `TweakSection` uses `label` not `title`, and `TweakRadio` takes plain string options or `{value,label}` — let me fix app.jsx and finish the HTML.

_[tool: snip]_

_[tool: str_replace_edit]_

_[tool: write_file]_

_[tool: show_html]_

_[tool: done]_

_[tool: snip]_

_[tool: fork_verifier_agent]_

**atomio** is up.

What's there:

- **Launch screen** — recents list, project actions, simulator-ready hint.
- **Editor** — file tree + outline, tabbed editor with SF Mono syntax highlighting, gutter breakpoints, paused-line indicator with inline value pill, minimap.
- **Debugger panel** — control bar, call stack, variables (Local/Closure/Global), watch, breakpoints with hit counts.
- **Simulator** — device picker + live phone mock of the cart screen + perf chips.
- **Components / Profiler** — switch the right dock between debugger / component tree / profiler (frame chart + flame graph + suggestion).
- **Console** — live-streaming logs with level filters, evaluator input.
- **Command palette** — ⌘K or ⌘⇧P; grouped by category, fully keyboard-driven.
- **Tweaks** — accent color, right-panel choice, sim toggle, jump to launch, toggle paused state.

Top-right floating pill lets you jump between Launch / Editor / Palette. Click any breakpoint dot in the gutter to toggle, hit F5 to flip running ↔ paused.

## User

we need to also design a few icon options please, as well as a hero for our github repo pls

## Assistant

_[tool: snip]_

_[tool: copy_starter_component]_

_[tool: read_file]_

_[tool: snip]_

_[tool: write_file]_

_[tool: write_file]_

_[tool: done]_

_[tool: fork_verifier_agent]_

**brand.html** is up with:

- **6 app icon options** at 220×220 (export-ready as 1024×1024 SVG): orbits, brackets, lowercase 'a', window+phone, pulse waveform, caret monogram.
- **Product hero (1280×640)** — wordmark + headline + CTAs on the left, perspective window mock with live cart.tsx + floating phone on the right, feature chips along the bottom.
- **Social card (1280×640)** — centered orbit mark + wordmark on a quieter pulse background, for OpenGraph/repo social preview.

Drag any artboard to reorder, double-click a label to rename, click the expand icon to focus one fullscreen.

## User

Apply comment

## Assistant

_[tool: snip]_

_[tool: str_replace_edit]_

_[tool: done]_

_[tool: grep]_

_[tool: read_file]_

_[tool: snip]_

_[tool: str_replace_edit]_

_[tool: done]_

_[tool: str_replace_edit]_

_[tool: snip]_

Headline now reads **"React Native & Expo / debugging that gets / out of your way."** — and the social card subhead is updated to match.

