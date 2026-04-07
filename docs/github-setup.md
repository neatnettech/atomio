# atomio — GitHub repository setup

This document is the manual companion to [`dev-practices.md`](dev-practices.md). Everything listed here must be configured **through the GitHub web UI** because it is not expressible in files checked into the repo. Follow it in order when bootstrapping the repo for the first time (or when rebuilding it on a fresh GitHub account).

Every step is annotated with **why** it matters, so the same contract that lives in `dev-practices.md` is reproduced mechanically in the GitHub settings. If you change anything in this file, update `dev-practices.md` in the same PR.

---

## 0. Prerequisites

- You are the repository owner or an admin.
- The repository already exists at `https://github.com/<owner>/atomio` and has `main` as the default branch.
- An initial `main` has been pushed (`git push -u origin main`).
- An Apple Developer account, if you plan to ship stable releases (can be deferred until you are close to `v0.1.0`).

---

## 1. General repository settings

**Path:** `Settings → General`.

| Setting | Value | Why |
|---|---|---|
| Default branch | `main` | Trunk-based development — one long-lived branch. |
| Features → Wikis | off | We keep docs in `docs/` so they live with the code. |
| Features → Projects | on | Useful for roadmap tracking. Optional. |
| Features → Discussions | off until v0.1.0 | Discussion noise is a maintainer burden before there is anything to discuss. |
| Features → Issues | on | We track work in issues (see `dev-practices.md` §8). |
| Pull Requests → Allow merge commits | off | We enforce linear history. |
| Pull Requests → Allow squash merging | on | The only allowed merge style. |
| Pull Requests → Allow rebase merging | off | Rebases happen on the feature branch before the squash. |
| Pull Requests → Always suggest updating pull request branches | on | Reduces stale rebase surprises. |
| Pull Requests → Allow auto-merge | on | Required for Dependabot auto-merge. |
| Pull Requests → Automatically delete head branches | on | Keeps the branch list clean. |
| Archives → Include Git LFS objects in archives | off | We do not use LFS. |

---

## 2. Branch protection on `main`

**Path:** `Settings → Branches → Branch protection rules → Add rule`.

**Branch name pattern:** `main`

Enable:

- **Require a pull request before merging**
  - Require approvals: **0** while solo, **1** once contributors exist
  - Dismiss stale pull request approvals when new commits are pushed: **on**
  - Require review from Code Owners: **off** (no `CODEOWNERS` yet)
- **Require status checks to pass before merging**
  - Require branches to be up to date before merging: **on**
  - Required status checks (add them one by one — they only appear after each workflow has run at least once):
    - `check` (from `ci.yml`)
    - `cargo-audit` (from `audit.yml`)
    - `validate` (from `pr-title.yml`)
- **Require conversation resolution before merging**: on
- **Require signed commits**: on (see §6)
- **Require linear history**: on
- **Require deployments to succeed before merging**: off (we do not deploy from PRs)
- **Lock branch**: off
- **Do not allow bypassing the above settings**: on
- **Restrict who can push to matching branches**: off (we use the PR gate; nothing pushes directly)
- **Rules applied to everyone including administrators**: **on**
  - *This is load-bearing.* Rules you can silently bypass are not rules. The solo-maintainer pit of failure is "I will just force-push this one time" at midnight.
- **Allow force pushes**: off
- **Allow deletions**: off

---

## 3. GitHub Environments

**Path:** `Settings → Environments → New environment`.

Create three environments. Each enforces a different policy gate mapped to a release channel.

### 3.1 `nightly`

- **Deployment branches and tags**: Selected branches and tags → `main`.
- **Wait timer**: 0 minutes.
- **Required reviewers**: none.
- **Environment secrets**: none.
- **Environment variables**: none.

*Why:* Nightlies are unsigned, automatic, and produced from every green `main`. No gates, no secrets.

### 3.2 `staging`

- **Deployment branches and tags**: All branches and tags.
- **Wait timer**: 0 minutes.
- **Required reviewers**: none.
- **Environment secrets**: none.
- **Environment variables**: none.

*Why:* Alpha and beta pre-releases go through this environment. They are manually triggered by the maintainer via `cut-prerelease.yml`, so the "gate" is the fact that the workflow only runs on `workflow_dispatch`. No secrets because pre-releases are not codesigned yet.

### 3.3 `production`

- **Deployment branches and tags**: Protected branches and tags only.
- **Wait timer**: **1440 minutes (24 hours).**
- **Required reviewers**: **yourself (the repo owner).**
- **Environment secrets** (add once you have an Apple Developer account; can be deferred):
  - `APPLE_ID` — Apple ID email.
  - `APPLE_TEAM_ID` — 10-character Team ID from the Apple Developer portal.
  - `APPLE_APP_PASSWORD` — App-Specific Password for `notarytool`.
  - `MACOS_CERT_P12` — base64-encoded Developer ID Application certificate (`.p12`).
  - `MACOS_CERT_PASSWORD` — password for the `.p12`.
- **Environment variables**: none.

*Why:* The 24-hour wait timer plus self-approval is the single most important solo-after-hours guardrail. Even if you tag `v0.1.0` at 2am by accident, nothing ships to users until you approve the deployment the next day. The Apple secrets live **only** in this environment, so only the `release.yml` job can read them — not CI, not nightly, not prerelease cuts.

---

## 4. Issue labels

**Path:** `Issues → Labels`.

GitHub ships with a default set of labels that conflict with ours. Delete them all and create the minimal set from `dev-practices.md` §8:

| Label | Color suggestion |
|---|---|
| `type:bug` | `#d73a4a` (red) |
| `type:feat` | `#a2eeef` (teal) |
| `type:chore` | `#cccccc` (gray) |
| `area:editor-core` | `#0366d6` (blue) |
| `area:language` | `#0366d6` |
| `area:ai` | `#0366d6` |
| `area:ext-host` | `#0366d6` |
| `area:ci` | `#0366d6` |
| `good-first-issue` | `#7057ff` (purple) |
| `help-wanted` | `#008672` (green) |
| `blocked` | `#e99695` (light red) |
| `wontfix` | `#ffffff` (white) |

Do not add priority labels (`P0`, `P1`, etc.). Priority is a milestone, not a label.

---

## 5. Milestones

**Path:** `Issues → Milestones → New milestone`.

One milestone per roadmap version. Create the next two only — do not clutter the list with distant milestones that will change.

| Milestone | Description |
|---|---|
| `v0.0` | Hello, atom. Architecture scaffold. |
| `v0.1` | It edits. First usable build. |

Add future milestones (`v0.2`, `v0.3`, …) as you approach them, not ahead of time.

---

## 6. Signed commits

Branch protection requires signed commits. To set this up on your local machine once:

```sh
# Use SSH-signed commits (simpler than GPG, supported since git 2.34).
ssh-keygen -t ed25519 -C "your-email@example.com"
git config --global gpg.format ssh
git config --global user.signingkey ~/.ssh/id_ed25519.pub
git config --global commit.gpgsign true
git config --global tag.gpgsign true
```

Then add the **public** key to GitHub twice:

1. `Settings → SSH and GPG keys → New SSH key → Key type: Authentication key` (for push auth — if not using HTTPS + token).
2. `Settings → SSH and GPG keys → New SSH key → Key type: Signing key` (for commit signature verification).

Verify with:

```sh
git log --show-signature -1
```

Every commit on `main` must show `Good "git" signature`.

---

## 7. Actions permissions

**Path:** `Settings → Actions → General`.

- **Actions permissions**: Allow `<owner>`, and select non-<owner>, actions and reusable workflows → on.
- **Allow actions created by GitHub**: on.
- **Allow actions by Marketplace verified creators**: on.
- **Allow specified actions and reusable workflows**: add these (exact match):
  - `googleapis/release-please-action@*`
  - `amannn/action-semantic-pull-request@*`
  - `dtolnay/rust-toolchain@*`
  - `Swatinem/rust-cache@*`
  - `rustsec/audit-check@*`
  - `actions/checkout@*`
- **Workflow permissions**: Read repository contents and packages permissions. (Individual workflows elevate per-job where needed via `permissions:` blocks.)
- **Allow GitHub Actions to create and approve pull requests**: **on** (required for release-please and Dependabot).

---

## 8. Secrets (repository-level)

**Path:** `Settings → Secrets and variables → Actions → Repository secrets`.

No repository-level secrets are needed for v0.0 through v0.3. All codesigning/notarization secrets live in the `production` environment (see §3.3), not here. Keeping them environment-scoped means `ci.yml`, `nightly.yml`, and `cut-prerelease.yml` cannot accidentally read them even if compromised.

---

## 9. Dependabot

The `.github/dependabot.yml` file in the repo handles weekly updates for GitHub Actions and cargo dependencies. No UI clicks needed — it activates automatically once the file is on `main`.

**Optional hardening:** `Settings → Code security and analysis`:

- **Dependency graph**: on.
- **Dependabot alerts**: on.
- **Dependabot security updates**: on.
- **Dependabot version updates**: on (picks up the yaml).
- **Secret scanning**: on.
- **Push protection**: on (blocks pushes that contain credentials).

---

## 10. First-run checklist

Before pushing the first commit that expects CI to gate merges:

- [ ] Repository exists, `main` pushed.
- [ ] §1 General settings applied.
- [ ] §7 Actions permissions configured (needs to happen before §2 because status checks only appear after at least one workflow run).
- [ ] Push a dummy PR (e.g. whitespace change) to trigger `ci.yml`, `audit.yml`, and `pr-title.yml` at least once. Close without merging.
- [ ] §2 Branch protection added, required status checks picked from the now-available list.
- [ ] §3 Environments created. `nightly` and `staging` with no secrets; `production` with 24h wait timer and self-approval (Apple secrets deferred).
- [ ] §4 Labels set up.
- [ ] §5 Milestones `v0.0` and `v0.1` created.
- [ ] §6 Signed commits configured locally.
- [ ] §9 Dependabot alerts + push protection on.

Once all of the above is green, the trunk-based + release-channels policy documented in `dev-practices.md` is enforced mechanically, not by discipline.

---

## 11. Changing this document

This file is a contract with GitHub's UI. It should be updated in the same PR as any change to `dev-practices.md` §2, §5, or §7. To change it:

1. Open a PR titled `docs(github-setup): <change>`.
2. Explain *why* in the body.
3. Self-merge after CI passes.
