//! Manifest detection and parsing.
//!
//! atomio cares about three project flavours:
//!
//! - **Expo** -- `package.json` has `expo` dependency, or `app.json` /
//!   `app.config.{js,ts}` exists. Most common case.
//! - **React Native (bare)** -- `package.json` has `react-native` but no
//!   `expo`, and an `ios/` or `android/` dir exists. Rarer but worth
//!   distinguishing because the run commands differ.
//! - **Generic** -- any directory with a `package.json`. The dev server
//!   command is unknown but the tree + terminal still work.
//!
//! Anything without a `package.json` (or other recognised marker) opens
//! as **`Generic` without a manifest**, treating the directory as a
//! plain file tree.
//!
//! In addition the detector flags **monorepo roots** (pnpm-workspace,
//! lerna, or `workspaces` in `package.json`). When the root itself
//! has no Expo / RN deps, we look one level deeper under the
//! conventional `apps/*` and `packages/*` layouts for an Expo or RN
//! sub-package so the title bar reflects the actual workflow rather
//! than the monorepo shell.

use std::fs;
use std::path::{Path, PathBuf};

/// Flavour of the opened project. Drives the "suggested commands"
/// shown in the terminal pane and the badge in the status bar.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProjectKind {
    /// Expo-managed app. Run with `npx expo start`.
    Expo,
    /// Bare React Native. Run with `npx react-native start` +
    /// `npx react-native run-ios` / `run-android`.
    ReactNative,
    /// Any other JS project (Node lib, Next.js, etc.). Useful for the
    /// tree + terminal but no dev-server auto-suggest.
    Generic,
    /// Directory without a recognised manifest. Open as a plain tree.
    Unknown,
}

/// Parsed view of the project's manifest. Built by [`detect_kind`].
/// Lightweight on purpose -- only the fields atomio actually reads.
#[derive(Debug, Clone)]
pub struct Manifest {
    /// What flavour we detected.
    pub kind: ProjectKind,
    /// `name` field from `package.json`, if any. Used in the title bar
    /// and recents list.
    pub name: Option<String>,
    /// `version` field from `package.json`, if any.
    pub version: Option<String>,
    /// Path to the `package.json` we read, relative to project root.
    /// `None` when the project has no `package.json` (Unknown kind).
    pub manifest_path: Option<PathBuf>,
    /// Whether the project root looks like a JS / TS monorepo: it has
    /// either a `pnpm-workspace.yaml`, a `lerna.json`, or a
    /// `workspaces` field in its root `package.json`. For monorepos
    /// with an Expo or RN sub-package under `apps/*` or `packages/*`,
    /// [`Manifest::kind`] reflects the first such sub-package found
    /// (top-level deps still take precedence).
    pub is_monorepo: bool,
}

/// Look at `root` and figure out what kind of project it is.
///
/// Reads `package.json` if present (best effort -- malformed JSON
/// degrades to `Generic` with no name). Checks for Expo markers
/// (`app.json`, `app.config.js`, `app.config.ts`) and native-shell
/// dirs (`ios/`, `android/`) to disambiguate Expo vs bare RN.
///
/// Recognises monorepo roots via `pnpm-workspace.yaml`, `lerna.json`,
/// or a `workspaces` field in `package.json`, and -- when the root
/// doesn't carry RN / Expo deps itself -- promotes the kind to match
/// the first sub-package found under `apps/*` or `packages/*`.
///
/// Never panics. I/O errors degrade to [`ProjectKind::Unknown`].
pub fn detect_kind(root: &Path) -> Manifest {
    let pkg_path = root.join("package.json");
    let pkg_exists = pkg_path.exists();

    // Safe to call unconditionally: missing / unreadable / malformed
    // package.json all collapse to (None, None, no-op closure, false).
    let (name, version, deps_have, has_workspaces_field) = read_package_json(&pkg_path);

    let is_monorepo = has_workspaces_field
        || root.join("pnpm-workspace.yaml").exists()
        || root.join("lerna.json").exists();

    if !pkg_exists && !is_monorepo {
        return Manifest {
            kind: ProjectKind::Unknown,
            name: None,
            version: None,
            manifest_path: None,
            is_monorepo: false,
        };
    }

    let has_expo_dep = deps_have("expo");
    let has_rn_dep = deps_have("react-native");
    let has_app_json = root.join("app.json").exists()
        || root.join("app.config.js").exists()
        || root.join("app.config.ts").exists();
    let has_native_dirs = root.join("ios").is_dir() || root.join("android").is_dir();

    let mut kind = if has_expo_dep || has_app_json {
        ProjectKind::Expo
    } else if has_rn_dep && has_native_dirs {
        ProjectKind::ReactNative
    } else if pkg_exists {
        ProjectKind::Generic
    } else {
        ProjectKind::Unknown
    };

    // Monorepo fallback: only promote when the root itself didn't
    // already resolve to an app flavour. This keeps direct
    // expo/react-native deps authoritative.
    if is_monorepo && matches!(kind, ProjectKind::Generic | ProjectKind::Unknown) {
        if let Some(sub) = detect_workspace_app_kind(root) {
            kind = sub;
        }
    }

    Manifest {
        kind,
        name,
        version,
        manifest_path: pkg_exists.then(|| PathBuf::from("package.json")),
        is_monorepo,
    }
}

/// Scan first-level sub-packages under `apps/` and `packages/` for an
/// Expo or RN app. Returns the first match (`Expo` wins over `RN`
/// within a single sub-package). We intentionally don't resolve the
/// monorepo's `workspaces` globs -- the two conventional dir names
/// cover ~95% of pnpm / yarn workspaces RN repos in the wild and
/// avoid the cost (and complexity) of glob expansion.
fn detect_workspace_app_kind(root: &Path) -> Option<ProjectKind> {
    for parent in ["apps", "packages"] {
        let dir = root.join(parent);
        let Ok(entries) = fs::read_dir(&dir) else {
            continue;
        };
        for entry in entries.flatten() {
            let sub = entry.path();
            if !sub.is_dir() {
                continue;
            }
            if let Some(kind) = classify_sub_package(&sub) {
                return Some(kind);
            }
        }
    }
    None
}

/// Treat a single sub-package directory: look for Expo / RN markers
/// using the same rules as the root detector. `None` when nothing
/// matches.
fn classify_sub_package(sub: &Path) -> Option<ProjectKind> {
    let pkg = sub.join("package.json");
    if !pkg.exists() {
        return None;
    }
    let (_, _, deps_have, _) = read_package_json(&pkg);
    let has_expo_dep = deps_have("expo");
    let has_rn_dep = deps_have("react-native");
    let has_app_json = sub.join("app.json").exists()
        || sub.join("app.config.js").exists()
        || sub.join("app.config.ts").exists();
    let has_native_dirs = sub.join("ios").is_dir() || sub.join("android").is_dir();
    if has_expo_dep || has_app_json {
        return Some(ProjectKind::Expo);
    }
    if has_rn_dep && has_native_dirs {
        return Some(ProjectKind::ReactNative);
    }
    None
}

/// Returns `(name, version, dep_has, has_workspaces_field)` from
/// `package.json`. The closure answers "is `<dep>` in dependencies or
/// devDependencies?" so callers don't have to round-trip JSON values.
/// `has_workspaces_field` is true when the manifest has a top-level
/// `workspaces` field in either the array or `{ packages: [...] }`
/// shape -- both shapes mark a yarn / npm workspaces root.
/// Missing / malformed JSON yields `(None, None, |_| false, false)`.
fn read_package_json(
    path: &Path,
) -> (
    Option<String>,
    Option<String>,
    impl Fn(&str) -> bool + 'static,
    bool,
) {
    let Ok(text) = fs::read_to_string(path) else {
        return (None, None, dep_has_static(serde_json::Value::Null), false);
    };
    let Ok(v) = serde_json::from_str::<serde_json::Value>(&text) else {
        return (None, None, dep_has_static(serde_json::Value::Null), false);
    };
    let name = v
        .get("name")
        .and_then(|n| n.as_str())
        .map(|s| s.to_string());
    let version = v
        .get("version")
        .and_then(|n| n.as_str())
        .map(|s| s.to_string());
    let has_workspaces_field = match v.get("workspaces") {
        Some(serde_json::Value::Array(a)) => !a.is_empty(),
        Some(serde_json::Value::Object(o)) => o
            .get("packages")
            .and_then(|p| p.as_array())
            .is_some_and(|a| !a.is_empty()),
        _ => false,
    };
    (name, version, dep_has_static(v), has_workspaces_field)
}

/// Build the dep-has closure from the parsed `package.json` value.
fn dep_has_static(v: serde_json::Value) -> impl Fn(&str) -> bool + 'static {
    move |dep: &str| {
        let in_section = |section: &str| {
            v.get(section)
                .and_then(|d| d.as_object())
                .is_some_and(|m| m.contains_key(dep))
        };
        in_section("dependencies")
            || in_section("devDependencies")
            || in_section("peerDependencies")
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    use tempfile::TempDir;

    fn write(dir: &Path, name: &str, body: &str) {
        let mut f = fs::File::create(dir.join(name)).unwrap();
        f.write_all(body.as_bytes()).unwrap();
    }

    #[test]
    fn empty_dir_is_unknown() {
        let tmp = TempDir::new().unwrap();
        let m = detect_kind(tmp.path());
        assert_eq!(m.kind, ProjectKind::Unknown);
        assert!(m.manifest_path.is_none());
        assert!(!m.is_monorepo);
    }

    #[test]
    fn package_json_with_expo_dep_is_expo() {
        let tmp = TempDir::new().unwrap();
        write(
            tmp.path(),
            "package.json",
            r#"{"name":"my-app","version":"1.0.0","dependencies":{"expo":"^51"}}"#,
        );
        let m = detect_kind(tmp.path());
        assert_eq!(m.kind, ProjectKind::Expo);
        assert_eq!(m.name.as_deref(), Some("my-app"));
        assert_eq!(m.version.as_deref(), Some("1.0.0"));
    }

    #[test]
    fn app_json_alone_implies_expo() {
        let tmp = TempDir::new().unwrap();
        write(tmp.path(), "package.json", r#"{"name":"x"}"#);
        write(tmp.path(), "app.json", "{}");
        let m = detect_kind(tmp.path());
        assert_eq!(m.kind, ProjectKind::Expo);
    }

    #[test]
    fn bare_rn_needs_native_dirs() {
        let tmp = TempDir::new().unwrap();
        write(
            tmp.path(),
            "package.json",
            r#"{"dependencies":{"react-native":"0.74"}}"#,
        );
        // Without ios/android, falls back to Generic.
        let m = detect_kind(tmp.path());
        assert_eq!(m.kind, ProjectKind::Generic);
        fs::create_dir(tmp.path().join("ios")).unwrap();
        let m2 = detect_kind(tmp.path());
        assert_eq!(m2.kind, ProjectKind::ReactNative);
    }

    #[test]
    fn malformed_package_json_degrades_to_generic() {
        let tmp = TempDir::new().unwrap();
        write(tmp.path(), "package.json", "{not valid json");
        let m = detect_kind(tmp.path());
        assert_eq!(m.kind, ProjectKind::Generic);
        assert!(m.name.is_none());
        assert!(m.manifest_path.is_some());
    }

    #[test]
    fn devdeps_count_too() {
        let tmp = TempDir::new().unwrap();
        write(
            tmp.path(),
            "package.json",
            r#"{"devDependencies":{"expo":"^51"}}"#,
        );
        assert_eq!(detect_kind(tmp.path()).kind, ProjectKind::Expo);
    }

    /// Write `body` to `dir/<rel>`, creating any intermediate dirs.
    fn write_nested(dir: &Path, rel: &str, body: &str) {
        let p = dir.join(rel);
        if let Some(parent) = p.parent() {
            fs::create_dir_all(parent).unwrap();
        }
        fs::write(p, body).unwrap();
    }

    #[test]
    fn pnpm_workspace_yaml_marks_monorepo() {
        let tmp = TempDir::new().unwrap();
        write(tmp.path(), "package.json", r#"{"name":"shell"}"#);
        write(tmp.path(), "pnpm-workspace.yaml", "packages:\n  - apps/*\n");
        let m = detect_kind(tmp.path());
        assert!(m.is_monorepo);
        // No Expo / RN sub-packages, so kind stays Generic.
        assert_eq!(m.kind, ProjectKind::Generic);
    }

    #[test]
    fn workspaces_array_in_package_json_marks_monorepo() {
        let tmp = TempDir::new().unwrap();
        write(
            tmp.path(),
            "package.json",
            r#"{"name":"shell","workspaces":["apps/*","packages/*"]}"#,
        );
        let m = detect_kind(tmp.path());
        assert!(m.is_monorepo);
    }

    #[test]
    fn workspaces_object_with_packages_marks_monorepo() {
        // The legacy `{ packages: [...] }` shape is what yarn 1
        // emitted; still seen in older repos.
        let tmp = TempDir::new().unwrap();
        write(
            tmp.path(),
            "package.json",
            r#"{"name":"shell","workspaces":{"packages":["apps/*"]}}"#,
        );
        let m = detect_kind(tmp.path());
        assert!(m.is_monorepo);
    }

    #[test]
    fn empty_workspaces_array_does_not_mark_monorepo() {
        let tmp = TempDir::new().unwrap();
        write(
            tmp.path(),
            "package.json",
            r#"{"name":"shell","workspaces":[]}"#,
        );
        let m = detect_kind(tmp.path());
        assert!(!m.is_monorepo);
    }

    #[test]
    fn lerna_json_marks_monorepo() {
        let tmp = TempDir::new().unwrap();
        write(tmp.path(), "package.json", r#"{"name":"shell"}"#);
        write(tmp.path(), "lerna.json", r#"{"version":"independent"}"#);
        let m = detect_kind(tmp.path());
        assert!(m.is_monorepo);
    }

    #[test]
    fn monorepo_with_expo_subpackage_resolves_to_expo() {
        let tmp = TempDir::new().unwrap();
        write(
            tmp.path(),
            "package.json",
            r#"{"name":"shell","workspaces":["apps/*"]}"#,
        );
        write_nested(
            tmp.path(),
            "apps/mobile/package.json",
            r#"{"name":"mobile","dependencies":{"expo":"^51"}}"#,
        );
        let m = detect_kind(tmp.path());
        assert!(m.is_monorepo);
        assert_eq!(m.kind, ProjectKind::Expo);
        // Root name still wins over sub-package name for display.
        assert_eq!(m.name.as_deref(), Some("shell"));
    }

    #[test]
    fn monorepo_with_rn_subpackage_under_packages_resolves_to_rn() {
        let tmp = TempDir::new().unwrap();
        write(tmp.path(), "package.json", r#"{"name":"shell"}"#);
        write(
            tmp.path(),
            "pnpm-workspace.yaml",
            "packages:\n  - 'packages/*'\n",
        );
        write_nested(
            tmp.path(),
            "packages/native/package.json",
            r#"{"name":"native","dependencies":{"react-native":"0.74"}}"#,
        );
        fs::create_dir_all(tmp.path().join("packages/native/ios")).unwrap();
        let m = detect_kind(tmp.path());
        assert!(m.is_monorepo);
        assert_eq!(m.kind, ProjectKind::ReactNative);
    }

    #[test]
    fn monorepo_with_root_expo_dep_keeps_root_authoritative() {
        // Root has Expo deps + workspaces field; sub-package has RN.
        // The root dep should win even though both are present.
        let tmp = TempDir::new().unwrap();
        write(
            tmp.path(),
            "package.json",
            r#"{"name":"shell","workspaces":["apps/*"],"dependencies":{"expo":"^51"}}"#,
        );
        write_nested(
            tmp.path(),
            "apps/legacy/package.json",
            r#"{"dependencies":{"react-native":"0.74"}}"#,
        );
        fs::create_dir_all(tmp.path().join("apps/legacy/ios")).unwrap();
        let m = detect_kind(tmp.path());
        assert!(m.is_monorepo);
        assert_eq!(m.kind, ProjectKind::Expo);
    }

    #[test]
    fn pnpm_workspace_without_root_package_json_is_recognised() {
        // Unusual but valid: pnpm workspace where the root is just a
        // pnpm-workspace.yaml + sub-packages, no root package.json.
        let tmp = TempDir::new().unwrap();
        write(tmp.path(), "pnpm-workspace.yaml", "packages:\n  - apps/*\n");
        write_nested(
            tmp.path(),
            "apps/mobile/package.json",
            r#"{"dependencies":{"expo":"^51"}}"#,
        );
        let m = detect_kind(tmp.path());
        assert!(m.is_monorepo);
        assert_eq!(m.kind, ProjectKind::Expo);
        // No root manifest, so manifest_path stays None.
        assert!(m.manifest_path.is_none());
    }

    #[test]
    fn sub_package_with_no_rn_or_expo_falls_through() {
        let tmp = TempDir::new().unwrap();
        write(
            tmp.path(),
            "package.json",
            r#"{"name":"shell","workspaces":["packages/*"]}"#,
        );
        write_nested(
            tmp.path(),
            "packages/lib/package.json",
            r#"{"name":"lib","dependencies":{"lodash":"4"}}"#,
        );
        let m = detect_kind(tmp.path());
        assert!(m.is_monorepo);
        assert_eq!(m.kind, ProjectKind::Generic);
    }
}
