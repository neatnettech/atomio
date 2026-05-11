//! Embedded asset source for the gpui app.
//!
//! gpui's `svg()` element loads paths through an `AssetSource`. Rather
//! than ship a separate assets dir we'd have to find at runtime, embed
//! the SVGs into the binary with `include_bytes!` and resolve by path.
//!
//! Paths are flat, no extension fuss -- `"icons/<name>.svg"` lines up
//! with the file layout under `assets/`. Unknown paths return `Ok(None)`
//! which gpui treats as "skip drawing this svg" rather than erroring.

use std::borrow::Cow;

use gpui::{AssetSource, Result, SharedString};

/// All icon assets used by the activity bar / toolbars. Each entry is
/// `("path", include_bytes!(...))`. Add new icons here.
const ICONS: &[(&str, &[u8])] = &[
    (
        "icons/files.svg",
        include_bytes!("../assets/icons/files.svg"),
    ),
    (
        "icons/debugger.svg",
        include_bytes!("../assets/icons/debugger.svg"),
    ),
    (
        "icons/network.svg",
        include_bytes!("../assets/icons/network.svg"),
    ),
    (
        "icons/simulator.svg",
        include_bytes!("../assets/icons/simulator.svg"),
    ),
    (
        "icons/components.svg",
        include_bytes!("../assets/icons/components.svg"),
    ),
    (
        "icons/profiler.svg",
        include_bytes!("../assets/icons/profiler.svg"),
    ),
    (
        "icons/console.svg",
        include_bytes!("../assets/icons/console.svg"),
    ),
];

/// Asset source backed by [`ICONS`]. Cheap to clone (the table is
/// `'static`) so this is safe to hand to `Application::with_assets`.
#[derive(Clone, Copy, Default)]
pub struct EmbeddedAssets;

impl AssetSource for EmbeddedAssets {
    fn load(&self, path: &str) -> Result<Option<Cow<'static, [u8]>>> {
        for (p, bytes) in ICONS {
            if *p == path {
                return Ok(Some(Cow::Borrowed(*bytes)));
            }
        }
        Ok(None)
    }

    fn list(&self, path: &str) -> Result<Vec<SharedString>> {
        // Return every entry whose path starts with the requested prefix.
        // gpui only uses this for asset enumeration; the empty prefix
        // case returns all known paths.
        Ok(ICONS
            .iter()
            .filter(|(p, _)| p.starts_with(path))
            .map(|(p, _)| SharedString::from(*p))
            .collect())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn load_known_icon() {
        let src = EmbeddedAssets;
        let bytes = src.load("icons/files.svg").unwrap().unwrap();
        assert!(!bytes.is_empty());
        // Sanity check that we got SVG, not some other file's bytes.
        let head = std::str::from_utf8(&bytes[..bytes.len().min(64)]).unwrap_or("");
        assert!(head.contains("<svg"));
    }

    #[test]
    fn load_unknown_returns_none() {
        let src = EmbeddedAssets;
        assert!(src.load("icons/missing.svg").unwrap().is_none());
    }

    #[test]
    fn list_prefix_filters_correctly() {
        let src = EmbeddedAssets;
        let all = src.list("").unwrap();
        assert_eq!(all.len(), ICONS.len());
        let only_icons = src.list("icons/").unwrap();
        assert_eq!(only_icons.len(), ICONS.len());
        let nothing = src.list("nope/").unwrap();
        assert!(nothing.is_empty());
    }

    #[test]
    fn every_dockpane_has_an_icon() {
        // Keeps the icon table in sync with the activity bar -- if a new
        // pane lands without a corresponding asset entry the test fires.
        let src = EmbeddedAssets;
        for name in [
            "files",
            "debugger",
            "network",
            "simulator",
            "components",
            "profiler",
            "console",
        ] {
            let path = format!("icons/{name}.svg");
            assert!(
                src.load(&path).unwrap().is_some(),
                "missing icon for {name}"
            );
        }
    }
}
