//! Script registry: tracks scripts the debug target reports via
//! `Debugger.scriptParsed`.
//!
//! Hermes (and CDP in general) emits `Debugger.scriptParsed` for every
//! script the runtime loads. Each carries a string `scriptId`, a `url`
//! (often a Metro path like `http://localhost:8081/index.bundle?...`),
//! optional `sourceMapURL`, and a content `hash`.
//!
//! The registry is pure model: parse + store. Fetching the actual source
//! text lives in [`crate::metro`]; resolving source maps lives in a
//! follow-up crate.

use serde_json::Value;
use std::collections::HashMap;

/// One script reported by `Debugger.scriptParsed`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Script {
    /// CDP `scriptId` -- opaque target-assigned identifier.
    pub id: String,
    /// Source URL, often a Metro path.
    pub url: String,
    /// Source map URL when present; resolved against `url` if relative.
    pub source_map_url: Option<String>,
    /// Content hash if reported.
    pub hash: Option<String>,
    /// Total number of lines reported by the runtime, when known.
    pub line_count: Option<u32>,
}

impl Script {
    /// Build a script from a parsed `Debugger.scriptParsed` event payload.
    /// Returns `None` when required fields are missing.
    pub fn from_script_parsed(params: &Value) -> Option<Self> {
        let id = params.get("scriptId").and_then(|v| v.as_str())?.to_string();
        let url = params.get("url").and_then(|v| v.as_str())?.to_string();
        let source_map_url = params
            .get("sourceMapURL")
            .and_then(|v| v.as_str())
            .filter(|s| !s.is_empty())
            .map(|s| s.to_string());
        let hash = params
            .get("hash")
            .and_then(|v| v.as_str())
            .map(|s| s.to_string());
        let line_count = params
            .get("endLine")
            .and_then(|v| v.as_u64())
            .map(|n| n as u32);
        Some(Self {
            id,
            url,
            source_map_url,
            hash,
            line_count,
        })
    }
}

/// In-memory map from `scriptId` to [`Script`].
#[derive(Debug, Default, Clone)]
pub struct ScriptRegistry {
    by_id: HashMap<String, Script>,
}

impl ScriptRegistry {
    pub fn new() -> Self {
        Self::default()
    }

    /// Insert or replace a script. Returns true if the entry is new.
    pub fn upsert(&mut self, script: Script) -> bool {
        self.by_id.insert(script.id.clone(), script).is_none()
    }

    /// Look up a script by its CDP id.
    pub fn get(&self, id: &str) -> Option<&Script> {
        self.by_id.get(id)
    }

    /// Find a script by URL (linear scan; the count of scripts in a single
    /// debug session is small, so this is fine).
    pub fn find_by_url(&self, url: &str) -> Option<&Script> {
        self.by_id.values().find(|s| s.url == url)
    }

    pub fn len(&self) -> usize {
        self.by_id.len()
    }

    pub fn is_empty(&self) -> bool {
        self.by_id.is_empty()
    }

    /// Iterate over all known scripts.
    pub fn iter(&self) -> impl Iterator<Item = &Script> {
        self.by_id.values()
    }

    /// Drop all entries. Used on disconnect.
    pub fn clear(&mut self) {
        self.by_id.clear();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_full_payload() {
        let params = serde_json::json!({
            "scriptId": "42",
            "url": "http://localhost:8081/index.bundle",
            "sourceMapURL": "index.bundle.map",
            "hash": "deadbeef",
            "startLine": 0,
            "endLine": 12345
        });
        let s = Script::from_script_parsed(&params).unwrap();
        assert_eq!(s.id, "42");
        assert_eq!(s.url, "http://localhost:8081/index.bundle");
        assert_eq!(s.source_map_url.as_deref(), Some("index.bundle.map"));
        assert_eq!(s.hash.as_deref(), Some("deadbeef"));
        assert_eq!(s.line_count, Some(12345));
    }

    #[test]
    fn empty_source_map_url_treated_as_none() {
        let params = serde_json::json!({
            "scriptId": "1",
            "url": "x.js",
            "sourceMapURL": ""
        });
        let s = Script::from_script_parsed(&params).unwrap();
        assert!(s.source_map_url.is_none());
    }

    #[test]
    fn missing_required_fields_returns_none() {
        let no_id = serde_json::json!({ "url": "x.js" });
        let no_url = serde_json::json!({ "scriptId": "1" });
        assert!(Script::from_script_parsed(&no_id).is_none());
        assert!(Script::from_script_parsed(&no_url).is_none());
    }

    #[test]
    fn registry_upsert_and_lookup() {
        let mut reg = ScriptRegistry::new();
        let s1 = Script {
            id: "a".into(),
            url: "x.js".into(),
            source_map_url: None,
            hash: None,
            line_count: None,
        };
        assert!(reg.upsert(s1.clone()));
        // Re-upsert with same id is not new.
        assert!(!reg.upsert(s1.clone()));
        assert_eq!(reg.len(), 1);
        assert_eq!(reg.get("a").unwrap(), &s1);
        assert_eq!(reg.find_by_url("x.js").unwrap().id, "a");
        assert!(reg.find_by_url("nope").is_none());
    }

    #[test]
    fn registry_clear() {
        let mut reg = ScriptRegistry::new();
        reg.upsert(Script {
            id: "a".into(),
            url: "x.js".into(),
            source_map_url: None,
            hash: None,
            line_count: None,
        });
        reg.clear();
        assert!(reg.is_empty());
    }
}
