# atomio — Development Practices

This document is the single source of truth for *how* we build atomio. It's deliberately opinionated and tuned for a **solo, after-hours, macOS-first, fully open-source** project. Every rule here exists to protect one of three things: **focus**, **momentum**, or **trust** (of future contributors and users).

If a rule ever gets in the way of shipping, open a PR to change the rule before breaking it.

---

## 1. Guiding Principles

1. **Trunk is always green.** `main` must always build, test, and run. No exceptions.
2. **Small, reversible commits.** Every commit is a checkpoint. If in doubt, commit.
3. **Ship in public.** The repo is the dev log. Commit messages, PR descriptions, and issues are written for an audience of strangers, not for future-you alone.
4. **Say no by default.** Every "yes" to a feature is a "no" to shipping v1.0. The [ROADMAP](./ROADMAP.md) won't-do list is law.
5. **Automate what you'll forget.** CI, lints, formatters, and release scripts exist so the project doesn't depend on the maintainer's memory.

---

## 2. Branching Strategy

**Trunk-based development with short-lived feature branches.** Not GitFlow. Not release branches. No `develop`.

### Branches

| Branch | Purpose | Protected |
|---|---|---|
| `main` | Always releasable. All work lands here. | yes |
| `feat/<slug>` | New feature, short-lived (<1 week) | — |
| `fix/<slug>` | Bug fix | — |
| `chore/<slug>` | Tooling, docs, CI, refactor | — |
| `spike/<slug>` | Throwaway experiment — never merged | — |

### Rules

- Branch off `main`. Rebase onto `main` before merging (no merge commits on `main` — linear history).
- **Squash-merge** feature branches into `main` via PR. One PR = one logical change = one commit on `main`.
- Delete the branch after merge.
- Never force-push to `main`. Force-push to your own feature branches is fine.
- If a branch lives longer than a week, it's probably too big — split it.

### Solo workflow shortcut

When working solo and the change is obviously safe (docs, typo, trivial refactor), committing directly to `main` is allowed. **Anything touching `editor_core`, `language`, `ai`, or `ext_host_node` goes through a PR** — even solo — so CI runs and history stays reviewable.

---

## 3. Commit Messages

We use **[Conventional Commits](https://www.conventionalcommits.org/)**. This is non-negotiable because release tooling (changelogs, version bumps) will depend on it from v0.1.

### Format

```
<type>(<scope>): <subject>

<body — why, not what>

<footer — refs, breaking changes>
```

### Types

| Type | When |
|---|---|
| `feat` | New user-visible feature |
| `fix` | Bug fix |
| `perf` | Performance improvement |
| `refactor` | Code change that doesn't change behavior |
| `docs` | Documentation only |
| `test` | Tests only |
| `build` | Build system, dependencies |
| `ci` | GitHub Actions, release tooling |
| `chore` | Everything else |

### Scopes

Match the crate or top-level area: `editor_core`, `atomio`, `language`, `ai`, `ext_host_node`, `workspace`, `theme`, `sdk-ts`, `docs`, `ci`, `release`.

### Rules

- Subject: imperative mood, lowercase, no trailing period, ≤72 chars.
- Body: wrap at 72 chars. Explain **why**, not what — the diff shows what.
- Reference issues with `Refs #12`, `Closes #12`.
- Breaking changes: `BREAKING CHANGE:` footer **and** `!` after type, e.g. `feat(sdk-ts)!: rename workspace.observe → workspace.watch`.

### Examples

```
feat(editor_core): add rope-backed Buffer with insert/remove

Wraps ropey so downstream crates can depend on a stable API while we
iterate on the underlying representation. CRDT layer will slot in here
later without breaking callers.

Refs #1
```

```
fix(atomio): prevent panic when opening empty file

Rope::from_str("") is valid; the crash was in the selection init path.

Closes #7
```

---

## 4. Pull Requests

Every PR, even solo, follows this template:

```markdown
## What
One sentence describing the change.

## Why
The problem or the motivation. Link issue if one exists.

## How
Brief description of the approach. Call out anything non-obvious.

## Test plan
- [ ] `cargo test --workspace` passes
- [ ] Manual: describe what you clicked / typed / measured
- [ ] New tests cover the change (or: why not)

## Screenshots / GIFs
(for anything user-visible)
```

### Merging rules

- CI must be green.
- At least one `cargo test --workspace` run locally before pushing.
- Squash-merge. PR title becomes the commit subject — make it a Conventional Commit.
- Self-merge is allowed (solo project) but only after CI passes.

---

## 5. Versioning & Releases

**Semantic Versioning**, with a twist until v1.0:

- **v0.x.y** — breaking changes allowed on minor bumps (`0.1` → `0.2`). Patch (`0.1.0` → `0.1.1`) stays backward-compatible.
- **v1.0.0** onward — strict SemVer. Breaking changes only on major bumps.

### Git tags

- Every release is an annotated git tag: `v0.0.0`, `v0.1.0`, `v0.1.1`, …
- Tag format: `vMAJOR.MINOR.PATCH`. No `release-` prefix. No tag on every commit.
- Pre-releases: `v0.4.0-rc.1`, `v0.4.0-beta.2`.

### Release flow

1. Bump version in `Cargo.toml` (`workspace.package.version`).
2. Update `CHANGELOG.md` (generated from Conventional Commits — automated later).
3. Commit: `chore(release): v0.1.0`.
4. Tag: `git tag -a v0.1.0 -m "v0.1.0 — it edits"`.
5. Push: `git push && git push --tags`.
6. GitHub Actions `release` job builds, codesigns, notarizes, and attaches the `.dmg` to a GitHub Release.
7. Write the release notes by hand on GitHub — they're marketing copy, not a changelog dump.

### CHANGELOG

- `CHANGELOG.md` lives at repo root.
- [Keep a Changelog](https://keepachangelog.com/) format.
- One section per version. `[Unreleased]` at the top.
- Until v0.1, maintained by hand. After v0.1, generated from Conventional Commits.

---

## 6. Code Quality

### Formatting & linting

- `cargo fmt --all` — must pass. Enforced by CI.
- `cargo clippy --all-targets --all-features -- -D warnings` — must pass. Enforced by CI.
- No `#[allow(...)]` without a comment explaining why.
- No `unwrap()` / `expect()` in non-test code except where a panic is the *correct* behavior (invariant violations). Document it with a `// SAFETY:` or `// INVARIANT:` comment.

### Testing

- Unit tests live in `#[cfg(test)] mod tests` at the bottom of each module.
- Integration tests live in `crates/<crate>/tests/`.
- Every bug fix includes a regression test. Every new public API includes at least one test.
- Target: **>70% line coverage on `editor_core`**. The rest of the workspace: tests where they pay for themselves, not for ritual.
- Benchmarks use `criterion` and live in `crates/<crate>/benches/`. Perf regressions are release blockers.

### Dependencies

- Every new dependency needs a one-line justification in the commit message.
- Prefer the standard library. Prefer small, focused crates. Audit with `cargo audit` in CI.
- Git dependencies are allowed **only** for `gpui` (not on crates.io) and **only** pinned to a specific revision.

### Unsafe

- `unsafe` requires a `// SAFETY:` comment explaining every invariant.
- `unsafe` code is never merged without a second pair of eyes (for solo work: sleep on it, review in the morning).

---

## 7. GitHub Actions

All workflows live in `.github/workflows/`. Each file does one thing.

| Workflow | Trigger | Purpose |
|---|---|---|
| `ci.yml` | push to `main`, PR | fmt, clippy, build, test |
| `release.yml` | tag `v*` | build, codesign, notarize, publish .dmg |
| `audit.yml` | weekly cron, PR changing `Cargo.lock` | `cargo audit` for CVEs |
| `docs.yml` | push to `main` touching `docs/**` | deploy GitHub Pages site |

### Security rules

- **Never interpolate untrusted input** (issue titles, PR bodies, branch names) directly into `run:` blocks. Pass via `env:` and quote.
- Pin actions to a commit SHA, not `@v4`, for anything that touches secrets.
- Secrets (`APPLE_ID`, `MACOS_CERT_P12`, etc.) live in repo secrets, never in logs. Use `::add-mask::` for anything derived from a secret.
- Workflows are read-only by default (`permissions: contents: read`); grant write per-job only where needed.

---

## 8. GitHub Issues

Issues are the public roadmap **below** the milestones. They exist for tracking, not for conversation.

### Labels (minimal set — resist the urge to add more)

| Label | Meaning |
|---|---|
| `type:bug` | Something is broken |
| `type:feat` | New capability |
| `type:chore` | Tooling, docs, refactor |
| `area:editor-core` | Scoped to that crate/area |
| `area:language` | LSP / tree-sitter |
| `area:ai` | Agent runtime |
| `area:ext-host` | VS Code compat |
| `area:ci` | Build & release |
| `good-first-issue` | Contributor-friendly |
| `help-wanted` | Maintainer explicitly wants help |
| `blocked` | Waiting on something external |
| `wontfix` | See ROADMAP won't-do list |

No `P0`/`P1`/`priority:*` labels. Priority = milestone. If it's not in a milestone, it's not prioritized.

### Milestones

One milestone per roadmap version: `v0.0`, `v0.1`, `v0.2`, …. Issues without a milestone are triage.

### Triage cadence

**Weekly, 30 minutes max.** (Burnout rule.) Every issue is either:
- Assigned a milestone, or
- Labeled `wontfix` with a one-line reason, or
- Closed as stale (no activity in 60 days + not in a milestone).

### Issue templates

Lives in `.github/ISSUE_TEMPLATE/`:
- `bug_report.yml` — repro steps, expected, actual, version, OS.
- `feature_request.yml` — problem, proposed solution, alternatives considered, roadmap check.

---

## 9. Documentation

| Doc | Lives in | Audience |
|---|---|---|
| `README.md` | root | First-time visitor. Manifesto + quick build. |
| `docs/ROADMAP.md` | `/docs` | Contributors & followers. The plan. |
| `docs/dev-practices.md` | `/docs` | Contributors. This file. |
| `CHANGELOG.md` | root | Users upgrading between versions. |
| `docs/architecture.md` | `/docs` | Contributors. High-level system design. (TODO v0.1) |
| `docs/sdk/` | `/docs` | Plugin authors. (TODO v1.0) |
| Rustdoc (`///`) | inline | Anyone reading the code |

### Rules

- Every public item in every crate gets a `///` doc comment before v1.0.
- Every `TODO` in code has an owner (or `TODO(v0.x):` tied to a roadmap version). No anonymous TODOs.
- Docs that describe *what* the code does get stale; docs that describe *why* stay useful. Prefer the why.

---

## 10. Secrets & Safety

- **Never commit secrets.** `.env` is in `.gitignore`. If you commit one by accident: rotate the secret immediately, then clean history.
- Apple codesign certificates, notarization credentials, and any API keys live in GitHub repo secrets only.
- Never log full paths, env vars, or anything derived from a secret in CI.
- Dependency updates go through `cargo audit` weekly.

---

## 11. Dogfooding

From v0.1 onward, every release must be usable enough to edit *this documentation* in atomio itself. If it isn't, the release isn't ready.

---

## 12. Changing This Document

This file is a contract. To change it:
1. Open a PR titled `docs(dev-practices): <change>`.
2. Explain *why* in the body.
3. Self-merge after CI passes (solo) or get a review (once contributors exist).

Practices evolve. Rules should bend before they break — but always deliberately, in writing.
