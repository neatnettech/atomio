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

**Trunk-based development with release channels via tags.** One long-lived branch (`main`). Stability is a *tag*, not a *branch*. This is how Zed, Helix, Chromium, and modern Rust projects ship.

No `develop`. No `preview`. No `staging`. No release branches (unless we ever need to LTS-maintain a historical major version).

### Branches

| Branch | Purpose | Protected |
|---|---|---|
| `main` | The trunk. Default branch. Always green. All work lands here. | yes |
| `feat/<slug>` | New feature, short-lived (<1 week) | — |
| `fix/<slug>` | Bug fix | — |
| `chore/<slug>` | Tooling, docs, CI, refactor | — |
| `spike/<slug>` | Throwaway experiment — never merged | — |
| `release-please--*` | Auto-opened by release-please bot | — |

### Release channels (tags, not branches)

Stability lives in tags and GitHub Environments, not branches:

| Tag / ref | Channel | Cut by | Gate |
|---|---|---|---|
| `nightly` (moving tag) | latest green `main` | CI, auto on every push | green CI |
| `vX.Y.Z-alpha.N` | alpha | maintainer, manual | CI + smoke test + docs |
| `vX.Y.Z-beta.N` | beta | maintainer, manual | CI + feature freeze + benches + no P0 |
| `vX.Y.Z` | stable | `release-please` bot PR → merge | everything above + 24h soak + hand-written notes |
| `vX.Y.Z+1` patch | stable hotfix | `fix:` commit → bot → merge | same as stable |

All version tags are **annotated and signed**: `git tag -s vX.Y.Z -m "..."`.
`nightly` is a lightweight moving tag advanced by CI on every green `main`.

### GitHub Environments (the policy layer)

Three environments, configured in repo settings. They enforce release gates mechanically so solo-after-hours discipline doesn't have to.

| Environment | Used by | Secrets | Approval | Wait timer |
|---|---|---|---|---|
| `nightly` | nightly workflow | none | none | none |
| `staging` | alpha/beta workflow | none | none | none |
| `production` | stable release workflow | `APPLE_ID`, `APPLE_TEAM_ID`, `APPLE_APP_PASSWORD`, `MACOS_CERT_P12`, `MACOS_CERT_PASSWORD` | required reviewer: self | 24 hours |

The 24h soak on `production` means **even if you tag a stable release at 2am, nothing ships until you explicitly approve it the next day**. This is the single most important guardrail for solo after-hours work.

### Rules

- Branch off `main`. Rebase onto `main` before merging (linear history — no merge commits).
- **Squash-merge** feature branches into `main` via PR. One PR = one logical change = one commit on `main`. The PR title *is* the commit message and must be a Conventional Commit.
- Delete the branch after merge.
- Never force-push to `main`. Force-push to your own feature branches is fine.
- If a branch lives longer than a week, it's probably too big — split it.
- **`main` is always green.** If CI breaks on `main`, revert-first is the default move. A broken trunk leaks the whole model.

### Branch protection rules on `main`

Configured in repo settings (documented here so it's reproducible):

- Require PR before merge.
- Require status checks to pass: `ci / check`, `audit / audit`, `pr-title / validate`.
- Require branches to be up to date before merge.
- Require signed commits.
- Require linear history.
- Disallow force push.
- **Include administrators** — yes, even the sole maintainer. Rules you can silently bypass are not rules.

### Solo workflow shortcut

When working solo and the change is obviously safe (docs typo, CI tweak, this file itself), committing directly to `main` is allowed **only** until branch protection is enabled. After v0.0.0 is tagged, protection turns on and **everything** goes through a PR. No exceptions — the automation downstream (changelog, release-please, environment gating) depends on every commit flowing through the same pipe.

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

**Semantic Versioning** with automated bumps driven by Conventional Commits.

- **v0.x.y** — breaking changes allowed on *minor* bumps (`0.1` → `0.2`). Patch (`0.1.0` → `0.1.1`) stays backward-compatible.
- **v1.0.0** onward — strict SemVer. Breaking changes only on major bumps.

### Version bumps are automated

Versions are not bumped by hand. [`release-please`](https://github.com/googleapis/release-please) reads commit history on `main` and opens a "release PR" that:

- Bumps `workspace.package.version` in `Cargo.toml`.
- Updates `CHANGELOG.md` from Conventional Commits.
- Proposes the next tag.

Bump rules:

- `feat:` → minor bump (`0.1.0` → `0.2.0` in v0.x; `1.1.0` → `1.2.0` post-v1).
- `fix:` / `perf:` → patch bump.
- `feat!:` or `BREAKING CHANGE:` footer → major bump post-v1, minor bump in v0.x.

Merging the release PR triggers the tag + release workflow. **This is the only way versions change.** No manual version edits on `main`.

### Tags and channels

| Tag | Channel | How it's cut |
|---|---|---|
| `nightly` (moving) | nightly | CI advances it on every green `main` push |
| `vX.Y.Z-alpha.N` | alpha | maintainer manually runs the "cut alpha" workflow |
| `vX.Y.Z-beta.N` | beta | maintainer manually runs the "cut beta" workflow (requires feature freeze) |
| `vX.Y.Z` | stable | merging a release-please PR |

All version tags are **annotated and signed** (`git tag -s`). `nightly` is a lightweight moving tag; it gets force-updated every green build and is not signed. Never rely on `nightly` for reproducible builds — use an immutable `vX.Y.Z-*` tag instead.

### Release flow — stable

1. release-please has a PR open titled `chore(main): release 0.1.0`. Review the proposed CHANGELOG.
2. Merge the PR.
3. CI tags `v0.1.0`, triggers `release.yml` targeting the `production` environment.
4. Environment's 24-hour wait timer holds the release as a draft. **Sleep on it.**
5. Next day: run the release smoke test (see Definition of Done below). Approve the environment.
6. Release publishes: `.app` bundle codesigned, notarized, stapled, packaged as `.dmg`, uploaded to GitHub Releases.
7. Write the release notes by hand on the GitHub Release — they're marketing copy, not a changelog dump.

### Release flow — alpha / beta

1. From a green `main`, run the "cut alpha" (or beta) workflow manually via `workflow_dispatch`.
2. Workflow computes the next pre-release identifier, creates `vX.Y.Z-alpha.N`, builds, ships to the `staging` environment (no soak timer).
3. Pre-releases are marked "Pre-release" on GitHub and do not update the `latest` release pointer.

### Release flow — nightly

Nothing to do. Every green `main` push moves the `nightly` tag forward, builds, and publishes as a GitHub Pre-release with the tag name `nightly`. Old nightly artifacts are garbage-collected to keep the Releases page clean.

### Release flow — hotfix

No release branches. Hotfix flow is identical to normal work:

1. `git checkout -b fix/<slug>` from `main`.
2. PR, CI, squash-merge.
3. release-please sees the `fix:` commit and opens a patch-bump PR.
4. Merge → tag → release as above.

### Definition of Done (per channel)

| Channel | Gate |
|---|---|
| Commit on `main` | CI green, PR reviewed (self-review counts solo). |
| `nightly` | Green CI. Everything else is automatic. |
| alpha | CI green + manual 5-minute smoke test + docs updated for any new user-facing surface. |
| beta | alpha gate + feature freeze (no `feat:` commits between beta cut and stable) + `cargo bench` run + no open P0 issues. |
| stable | beta gate + 24h environment soak + hand-written release notes + launch-ready demo GIF or video. |

### CHANGELOG

- `CHANGELOG.md` lives at repo root.
- [Keep a Changelog](https://keepachangelog.com/) format.
- **Maintained by release-please.** Do not edit by hand; edit commit messages instead.
- The `[Unreleased]` section reflects commits landed on `main` since the last tag and is rebuilt automatically.

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
