//! React component tree model for atomio.
//!
//! Holds a flat node table keyed by an opaque `NodeId`, plus a roots vector
//! and per-node child lists, so updates can mutate sub-trees without
//! re-allocating the world. Pure model -- no I/O.
//!
//! ### Wire format note
//!
//! React DevTools backend emits incremental "operations" arrays
//! (mount/unmount/update/reorder) over its own protocol. We don't decode
//! those bytes here -- the bridge layer will translate them into the
//! semantic methods on [`ComponentTree`] (`mount`, `unmount`,
//! `set_props`, `set_state`, `reorder_children`). That keeps this crate
//! testable with hand-written fixtures and lets the protocol decoder
//! evolve independently.
//!
//! For early integration, [`ComponentTree::seed_from_devtools_snapshot`]
//! accepts a generic JSON shape: an array of `{ id, name, props?, state?,
//! children? }` objects, recursive, with no parent pointers. Useful for
//! seeding the UI from a one-shot dump before incremental ops are wired.

use serde_json::Value;
use std::collections::HashMap;

/// Opaque node identifier. Allocated by the protocol decoder; the model
/// just stores them. Strings rather than ints because React DevTools uses
/// stringy ids and Hermes' fusebox bridge mirrors that.
pub type NodeId = String;

/// One component instance in the tree.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Component {
    pub id: NodeId,
    /// Display name -- typically the component or HTML/native tag (e.g.
    /// `View`, `Text`, `App`, `MyButton`).
    pub name: String,
    /// Optional element key.
    pub key: Option<String>,
    /// Props as raw JSON. UI lazy-renders by drilling in.
    pub props: Value,
    /// Local state as raw JSON. Empty object when none.
    pub state: Value,
    /// Children in render order.
    pub children: Vec<NodeId>,
    /// Parent id, if any. Roots have `None`.
    pub parent: Option<NodeId>,
}

impl Component {
    /// Convenience: create a node with empty props/state and no children.
    pub fn leaf(id: impl Into<NodeId>, name: impl Into<String>) -> Self {
        Self {
            id: id.into(),
            name: name.into(),
            key: None,
            props: Value::Object(Default::default()),
            state: Value::Object(Default::default()),
            children: Vec::new(),
            parent: None,
        }
    }
}

/// Component tree: flat node table + roots list.
#[derive(Debug, Default, Clone)]
pub struct ComponentTree {
    nodes: HashMap<NodeId, Component>,
    roots: Vec<NodeId>,
}

impl ComponentTree {
    pub fn new() -> Self {
        Self::default()
    }

    /// Insert or replace a node. Re-parents under `parent` (or root).
    /// Doesn't auto-add to parent's children -- callers (or
    /// [`ComponentTree::mount`]) handle that.
    pub fn upsert(&mut self, node: Component) {
        let id = node.id.clone();
        let parent = node.parent.clone();
        self.nodes.insert(id.clone(), node);
        if parent.is_none() && !self.roots.contains(&id) {
            self.roots.push(id);
        }
    }

    /// Mount a new node at the end of `parent`'s children. When `parent`
    /// is `None`, the node becomes a root.
    pub fn mount(&mut self, mut node: Component, parent: Option<NodeId>) {
        let id = node.id.clone();
        node.parent = parent.clone();
        self.nodes.insert(id.clone(), node);
        match parent {
            Some(pid) => {
                if let Some(p) = self.nodes.get_mut(&pid) {
                    if !p.children.contains(&id) {
                        p.children.push(id);
                    }
                }
            }
            None => {
                if !self.roots.contains(&id) {
                    self.roots.push(id);
                }
            }
        }
    }

    /// Remove a node and the entire sub-tree rooted at it.
    pub fn unmount(&mut self, id: &str) {
        // Detach from parent / roots first.
        let parent = self.nodes.get(id).and_then(|n| n.parent.clone());
        if let Some(pid) = &parent {
            if let Some(p) = self.nodes.get_mut(pid) {
                p.children.retain(|c| c != id);
            }
        } else {
            self.roots.retain(|r| r != id);
        }
        // Recursively drop sub-tree.
        let mut stack = vec![id.to_string()];
        while let Some(cur) = stack.pop() {
            if let Some(node) = self.nodes.remove(&cur) {
                stack.extend(node.children);
            }
        }
    }

    /// Replace the props of an existing node. No-op if missing.
    pub fn set_props(&mut self, id: &str, props: Value) {
        if let Some(n) = self.nodes.get_mut(id) {
            n.props = props;
        }
    }

    /// Replace the state of an existing node. No-op if missing.
    pub fn set_state(&mut self, id: &str, state: Value) {
        if let Some(n) = self.nodes.get_mut(id) {
            n.state = state;
        }
    }

    /// Reorder children of a node. Children not present in `order` are
    /// silently dropped from the list. Callers should pass a complete order.
    pub fn reorder_children(&mut self, id: &str, order: Vec<NodeId>) {
        if let Some(n) = self.nodes.get_mut(id) {
            n.children = order;
        }
    }

    pub fn get(&self, id: &str) -> Option<&Component> {
        self.nodes.get(id)
    }

    pub fn roots(&self) -> &[NodeId] {
        &self.roots
    }

    pub fn len(&self) -> usize {
        self.nodes.len()
    }

    pub fn is_empty(&self) -> bool {
        self.nodes.is_empty()
    }

    /// Drop everything. Used on disconnect.
    pub fn clear(&mut self) {
        self.nodes.clear();
        self.roots.clear();
    }

    /// Produce a depth-first list of `(id, depth)` for rendering. Caller
    /// gets ordered ids ready to map onto rows.
    pub fn flatten(&self) -> Vec<(NodeId, u32)> {
        let mut out = Vec::with_capacity(self.nodes.len());
        for r in &self.roots {
            self.walk(r, 0, &mut out);
        }
        out
    }

    fn walk(&self, id: &str, depth: u32, out: &mut Vec<(NodeId, u32)>) {
        if let Some(node) = self.nodes.get(id) {
            out.push((node.id.clone(), depth));
            for c in &node.children {
                self.walk(c, depth + 1, out);
            }
        }
    }

    /// Seed the tree from a one-shot devtools-style snapshot. Each entry
    /// is `{ id, name, key?, props?, state?, children? }` with `children`
    /// recursive. Wipes existing state.
    pub fn seed_from_devtools_snapshot(&mut self, snapshot: &Value) {
        self.clear();
        if let Some(arr) = snapshot.as_array() {
            for entry in arr {
                self.seed_one(entry, None);
            }
        }
    }

    fn seed_one(&mut self, value: &Value, parent: Option<NodeId>) {
        let Some(obj) = value.as_object() else {
            return;
        };
        let Some(id) = obj.get("id").and_then(|v| v.as_str()) else {
            return;
        };
        let name = obj
            .get("name")
            .and_then(|v| v.as_str())
            .unwrap_or("Unknown")
            .to_string();
        let key = obj
            .get("key")
            .and_then(|v| v.as_str())
            .map(|s| s.to_string());
        let props = obj
            .get("props")
            .cloned()
            .unwrap_or(Value::Object(Default::default()));
        let state = obj
            .get("state")
            .cloned()
            .unwrap_or(Value::Object(Default::default()));
        let node = Component {
            id: id.to_string(),
            name,
            key,
            props,
            state,
            children: Vec::new(),
            parent: parent.clone(),
        };
        self.mount(node, parent.clone());
        if let Some(arr) = obj.get("children").and_then(|v| v.as_array()) {
            for child in arr {
                self.seed_one(child, Some(id.to_string()));
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mount_creates_root_then_child() {
        let mut t = ComponentTree::new();
        t.mount(Component::leaf("1", "App"), None);
        t.mount(Component::leaf("2", "View"), Some("1".into()));
        assert_eq!(t.roots(), &["1".to_string()]);
        assert_eq!(t.get("1").unwrap().children, vec!["2".to_string()]);
        assert_eq!(t.get("2").unwrap().parent.as_deref(), Some("1"));
    }

    #[test]
    fn unmount_drops_subtree() {
        let mut t = ComponentTree::new();
        t.mount(Component::leaf("1", "App"), None);
        t.mount(Component::leaf("2", "View"), Some("1".into()));
        t.mount(Component::leaf("3", "Text"), Some("2".into()));
        t.unmount("2");
        assert_eq!(t.len(), 1);
        assert!(t.get("2").is_none());
        assert!(t.get("3").is_none());
        assert!(t.get("1").unwrap().children.is_empty());
    }

    #[test]
    fn set_props_and_state() {
        let mut t = ComponentTree::new();
        t.mount(Component::leaf("1", "App"), None);
        t.set_props("1", serde_json::json!({"title": "hi"}));
        t.set_state("1", serde_json::json!({"count": 3}));
        let n = t.get("1").unwrap();
        assert_eq!(n.props["title"], "hi");
        assert_eq!(n.state["count"], 3);
    }

    #[test]
    fn reorder_children_swaps() {
        let mut t = ComponentTree::new();
        t.mount(Component::leaf("1", "App"), None);
        t.mount(Component::leaf("a", "A"), Some("1".into()));
        t.mount(Component::leaf("b", "B"), Some("1".into()));
        t.reorder_children("1", vec!["b".into(), "a".into()]);
        assert_eq!(
            t.get("1").unwrap().children,
            vec!["b".to_string(), "a".to_string()]
        );
    }

    #[test]
    fn flatten_dfs_order() {
        let mut t = ComponentTree::new();
        t.mount(Component::leaf("1", "App"), None);
        t.mount(Component::leaf("a", "A"), Some("1".into()));
        t.mount(Component::leaf("a1", "A1"), Some("a".into()));
        t.mount(Component::leaf("b", "B"), Some("1".into()));
        let flat = t.flatten();
        let ids: Vec<_> = flat.iter().map(|(i, _)| i.clone()).collect();
        assert_eq!(ids, vec!["1", "a", "a1", "b"]);
        let depths: Vec<_> = flat.iter().map(|(_, d)| *d).collect();
        assert_eq!(depths, vec![0, 1, 2, 1]);
    }

    #[test]
    fn seed_from_snapshot() {
        let snap = serde_json::json!([
            {
                "id": "1",
                "name": "App",
                "props": {"x": 1},
                "children": [
                    {"id": "2", "name": "View", "children": [
                        {"id": "3", "name": "Text"}
                    ]}
                ]
            }
        ]);
        let mut t = ComponentTree::new();
        t.seed_from_devtools_snapshot(&snap);
        assert_eq!(t.len(), 3);
        assert_eq!(t.flatten().len(), 3);
        assert_eq!(t.get("1").unwrap().props["x"], 1);
        assert_eq!(t.get("3").unwrap().parent.as_deref(), Some("2"));
    }

    #[test]
    fn clear_empties() {
        let mut t = ComponentTree::new();
        t.mount(Component::leaf("1", "App"), None);
        t.clear();
        assert!(t.is_empty());
        assert!(t.roots().is_empty());
    }
}
