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
}

/// Look at `root` and figure out what kind of project it is.
///
/// Reads `package.json` if present (best effort -- malformed JSON
/// degrades to `Generic` with no name). Checks for Expo markers
/// (`app.json`, `app.config.js`, `app.config.ts`) and native-shell
/// dirs (`ios/`, `android/`) to disambiguate Expo vs bare RN.
///
/// Never panics. I/O errors degrade to [`ProjectKind::Unknown`].
pub fn detect_kind(root: &Path) -> Manifest {
    let pkg_path = root.join("package.json");
    if !pkg_path.exists() {
        return Manifest {
            kind: ProjectKind::Unknown,
            name: None,
            version: None,
            manifest_path: None,
        };
    }

    let (name, version, deps_have) = read_package_json(&pkg_path);

    let has_expo_dep = deps_have("expo");
    let has_rn_dep = deps_have("react-native");
    let has_app_json = root.join("app.json").exists()
        || root.join("app.config.js").exists()
        || root.join("app.config.ts").exists();
    let has_native_dirs = root.join("ios").is_dir() || root.join("android").is_dir();

    let kind = if has_expo_dep || has_app_json {
        ProjectKind::Expo
    } else if has_rn_dep && has_native_dirs {
        ProjectKind::ReactNative
    } else {
        ProjectKind::Generic
    };

    Manifest {
        kind,
        name,
        version,
        manifest_path: Some(PathBuf::from("package.json")),
    }
}

/// Returns `(name, version, dep_has)` from `package.json`. The closure
/// answers "is `<dep>` in dependencies or devDependencies?" so callers
/// don't have to round-trip JSON values. Missing / malformed JSON
/// yields `(None, None, |_| false)`.
fn read_package_json(
    path: &Path,
) -> (
    Option<String>,
    Option<String>,
    impl Fn(&str) -> bool + 'static,
) {
    let Ok(text) = fs::read_to_string(path) else {
        return (None, None, dep_has_static(serde_json::Value::Null));
    };
    let Ok(v) = serde_json::from_str::<serde_json::Value>(&text) else {
        return (None, None, dep_has_static(serde_json::Value::Null));
    };
    let name = v
        .get("name")
        .and_then(|n| n.as_str())
        .map(|s| s.to_string());
    let version = v
        .get("version")
        .and_then(|n| n.as_str())
        .map(|s| s.to_string());
    (name, version, dep_has_static(v))
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
}
