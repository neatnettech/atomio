//! Breakpoint registry.
//!
//! Tracks user-defined breakpoints and the corresponding server identifiers
//! returned by `Debugger.setBreakpointByUrl`. Pure model -- no I/O. The UI
//! layer mutates this map via [`BreakpointRegistry`] methods, then issues
//! the corresponding CDP request through the transport.
//!
//! ### Two-stage lifecycle
//!
//! 1. User clicks gutter → [`BreakpointRegistry::toggle`] returns
//!    [`ToggleOutcome::Added`] with a fresh [`BreakpointId`].
//! 2. UI sends `Debugger.setBreakpointByUrl`. The runtime responds with a
//!    `breakpointId` string and possibly a `locations` array showing where
//!    the engine resolved the breakpoint.
//! 3. UI calls [`BreakpointRegistry::attach_server_id`] /
//!    [`BreakpointRegistry::set_resolved_line`] so subsequent removes can
//!    target the right server-side breakpoint and the gutter dot reflects
//!    the actual line.
//!
//! Local [`BreakpointId`]s are stable across resolution, so the UI can key
//! its render list by them.

use std::collections::HashMap;

/// Local stable identifier for a breakpoint. Allocated by
/// [`BreakpointRegistry`]; never reused.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct BreakpointId(pub u64);

/// One breakpoint: where the user asked for it, plus state from the server.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Breakpoint {
    pub id: BreakpointId,
    /// Source URL the breakpoint targets (matches CDP `url`).
    pub url: String,
    /// Zero-based line number originally requested.
    pub requested_line: u32,
    /// Zero-based line the runtime resolved the breakpoint to. Equal to
    /// `requested_line` until the server reports a different one.
    pub resolved_line: u32,
    /// Optional zero-based column.
    pub column: Option<u32>,
    /// Optional condition expression. When present the runtime only halts
    /// when the expression evaluates truthy.
    pub condition: Option<String>,
    /// Server-assigned `breakpointId` once the registry has been notified
    /// of the response.
    pub server_id: Option<String>,
    /// User can disable a breakpoint without removing it.
    pub enabled: bool,
}

/// Outcome of [`BreakpointRegistry::toggle`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ToggleOutcome {
    /// A new breakpoint was created at the given line.
    Added(BreakpointId),
    /// An existing breakpoint at the given line was removed.
    Removed {
        local: BreakpointId,
        server_id: Option<String>,
    },
}

/// In-memory map of all breakpoints. Keyed by [`BreakpointId`].
#[derive(Debug, Default, Clone)]
pub struct BreakpointRegistry {
    by_id: HashMap<BreakpointId, Breakpoint>,
    next_id: u64,
}

impl BreakpointRegistry {
    pub fn new() -> Self {
        Self::default()
    }

    fn alloc_id(&mut self) -> BreakpointId {
        let id = BreakpointId(self.next_id);
        self.next_id += 1;
        id
    }

    /// Create a new breakpoint and return its local id.
    pub fn set(
        &mut self,
        url: impl Into<String>,
        line: u32,
        column: Option<u32>,
        condition: Option<String>,
    ) -> BreakpointId {
        let id = self.alloc_id();
        let bp = Breakpoint {
            id,
            url: url.into(),
            requested_line: line,
            resolved_line: line,
            column,
            condition,
            server_id: None,
            enabled: true,
        };
        self.by_id.insert(id, bp);
        id
    }

    /// Toggle a breakpoint at `(url, line)`. If one exists matching exactly,
    /// it's removed; otherwise a new one is created.
    pub fn toggle(&mut self, url: &str, line: u32) -> ToggleOutcome {
        if let Some(existing) = self
            .by_id
            .values()
            .find(|bp| bp.url == url && bp.requested_line == line)
            .map(|bp| (bp.id, bp.server_id.clone()))
        {
            self.by_id.remove(&existing.0);
            ToggleOutcome::Removed {
                local: existing.0,
                server_id: existing.1,
            }
        } else {
            ToggleOutcome::Added(self.set(url, line, None, None))
        }
    }

    /// Remove a breakpoint by local id. Returns the removed entry.
    pub fn remove(&mut self, id: BreakpointId) -> Option<Breakpoint> {
        self.by_id.remove(&id)
    }

    /// Look up a breakpoint by local id.
    pub fn get(&self, id: BreakpointId) -> Option<&Breakpoint> {
        self.by_id.get(&id)
    }

    /// Iterate over all breakpoints.
    pub fn iter(&self) -> impl Iterator<Item = &Breakpoint> {
        self.by_id.values()
    }

    /// Iterate over breakpoints matching a source URL.
    pub fn iter_for_url<'a>(&'a self, url: &'a str) -> impl Iterator<Item = &'a Breakpoint> + 'a {
        self.by_id.values().filter(move |bp| bp.url == url)
    }

    /// Number of breakpoints currently stored.
    pub fn len(&self) -> usize {
        self.by_id.len()
    }

    pub fn is_empty(&self) -> bool {
        self.by_id.is_empty()
    }

    /// Drop all breakpoints. Used on disconnect.
    pub fn clear(&mut self) {
        self.by_id.clear();
    }

    /// Record the server-assigned breakpoint id after a successful
    /// `Debugger.setBreakpointByUrl` response.
    pub fn attach_server_id(
        &mut self,
        id: BreakpointId,
        server_id: impl Into<String>,
    ) -> Option<&Breakpoint> {
        let bp = self.by_id.get_mut(&id)?;
        bp.server_id = Some(server_id.into());
        Some(&*bp)
    }

    /// Update the resolved line if the runtime relocated the breakpoint
    /// (e.g. it landed on a non-statement line and the engine moved it).
    pub fn set_resolved_line(&mut self, id: BreakpointId, line: u32) -> Option<&Breakpoint> {
        let bp = self.by_id.get_mut(&id)?;
        bp.resolved_line = line;
        Some(&*bp)
    }

    /// Toggle the enabled flag.
    pub fn set_enabled(&mut self, id: BreakpointId, enabled: bool) -> Option<&Breakpoint> {
        let bp = self.by_id.get_mut(&id)?;
        bp.enabled = enabled;
        Some(&*bp)
    }

    /// Find a local breakpoint by its server-assigned id.
    pub fn find_by_server_id(&self, server_id: &str) -> Option<&Breakpoint> {
        self.by_id
            .values()
            .find(|bp| bp.server_id.as_deref() == Some(server_id))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn url(s: &str) -> String {
        s.to_string()
    }

    #[test]
    fn set_assigns_unique_ids() {
        let mut reg = BreakpointRegistry::new();
        let a = reg.set(url("app.js"), 10, None, None);
        let b = reg.set(url("app.js"), 20, None, None);
        assert_ne!(a, b);
        assert_eq!(reg.len(), 2);
    }

    #[test]
    fn toggle_adds_then_removes() {
        let mut reg = BreakpointRegistry::new();
        let outcome1 = reg.toggle("app.js", 10);
        assert!(matches!(outcome1, ToggleOutcome::Added(_)));
        assert_eq!(reg.len(), 1);

        let outcome2 = reg.toggle("app.js", 10);
        assert!(matches!(outcome2, ToggleOutcome::Removed { .. }));
        assert!(reg.is_empty());
    }

    #[test]
    fn toggle_removed_carries_server_id() {
        let mut reg = BreakpointRegistry::new();
        let id = match reg.toggle("app.js", 10) {
            ToggleOutcome::Added(id) => id,
            _ => panic!("expected Added"),
        };
        reg.attach_server_id(id, "srv-42");

        match reg.toggle("app.js", 10) {
            ToggleOutcome::Removed { server_id, .. } => {
                assert_eq!(server_id.as_deref(), Some("srv-42"));
            }
            _ => panic!("expected Removed"),
        }
    }

    #[test]
    fn iter_for_url_filters() {
        let mut reg = BreakpointRegistry::new();
        reg.set(url("a.js"), 1, None, None);
        reg.set(url("a.js"), 5, None, None);
        reg.set(url("b.js"), 1, None, None);
        let a_count = reg.iter_for_url("a.js").count();
        let b_count = reg.iter_for_url("b.js").count();
        assert_eq!(a_count, 2);
        assert_eq!(b_count, 1);
    }

    #[test]
    fn attach_server_id_and_lookup() {
        let mut reg = BreakpointRegistry::new();
        let id = reg.set(url("a.js"), 1, None, None);
        reg.attach_server_id(id, "srv-1");
        assert_eq!(reg.find_by_server_id("srv-1").map(|bp| bp.id), Some(id));
        assert!(reg.find_by_server_id("missing").is_none());
    }

    #[test]
    fn resolved_line_can_drift() {
        let mut reg = BreakpointRegistry::new();
        let id = reg.set(url("a.js"), 10, None, None);
        assert_eq!(reg.get(id).unwrap().resolved_line, 10);
        reg.set_resolved_line(id, 12);
        assert_eq!(reg.get(id).unwrap().resolved_line, 12);
        // requested_line preserved.
        assert_eq!(reg.get(id).unwrap().requested_line, 10);
    }

    #[test]
    fn enable_disable() {
        let mut reg = BreakpointRegistry::new();
        let id = reg.set(url("a.js"), 1, None, None);
        assert!(reg.get(id).unwrap().enabled);
        reg.set_enabled(id, false);
        assert!(!reg.get(id).unwrap().enabled);
    }

    #[test]
    fn clear_drops_everything() {
        let mut reg = BreakpointRegistry::new();
        reg.set(url("a.js"), 1, None, None);
        reg.clear();
        assert!(reg.is_empty());
    }

    #[test]
    fn condition_round_trip() {
        let mut reg = BreakpointRegistry::new();
        let id = reg.set(url("a.js"), 1, None, Some("count > 5".into()));
        assert_eq!(reg.get(id).unwrap().condition.as_deref(), Some("count > 5"));
    }
}
