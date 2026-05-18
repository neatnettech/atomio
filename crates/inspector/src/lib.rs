//! Variable inspector + scope/call-stack model for atomio.
//!
//! Pure model crate. CDP `Debugger.paused` events carry a `callFrames`
//! array; each frame has a `scopeChain` of named scopes (Local / Closure /
//! Global / etc.) plus a `this` reference. Scopes contain a `RemoteObject`
//! whose properties are fetched lazily via `Runtime.getProperties`.
//!
//! This crate parses those payloads into typed structures the UI can
//! render, plus a [`Watch`] list for user-defined expressions evaluated on
//! each pause.
//!
//! ### What lives here
//!
//! - [`CallFrame`] / [`CallStack`] -- decoded paused-execution stack.
//! - [`Scope`] / [`ScopeKind`] -- one scope per stack frame.
//! - [`RemoteValue`] -- a CDP `RemoteObject` simplified for display.
//! - [`Property`] -- one entry inside a `Runtime.getProperties` response.
//! - [`Watch`] / [`WatchList`] -- user-typed expressions + last result.
//!
//! No I/O, no async. The bridge layer feeds parsed JSON into the parsers
//! here and surfaces their structs to the UI.

use serde_json::Value;

// ---------------------------------------------------------------------------
// Call stack
// ---------------------------------------------------------------------------

/// One frame from a `Debugger.paused.callFrames` entry.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CallFrame {
    /// CDP `callFrameId` -- opaque target-side handle used to evaluate
    /// expressions in this frame's context.
    pub id: String,
    /// Function name shown in the UI. Empty string when unavailable.
    pub function_name: String,
    /// CDP `scriptId` of the script this frame is in.
    pub script_id: String,
    /// Source URL when known (resolved against the script registry by the
    /// UI; not populated by the parser here).
    pub url: Option<String>,
    /// Zero-based line number in the script.
    pub line: u32,
    /// Zero-based column number in the script.
    pub column: u32,
    /// Per-frame scope chain (innermost first).
    pub scopes: Vec<Scope>,
}

impl CallFrame {
    /// Parse one entry from `Debugger.paused.callFrames`.
    pub fn from_cdp(value: &Value) -> Option<Self> {
        let id = value.get("callFrameId")?.as_str()?.to_string();
        let function_name = value
            .get("functionName")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string();
        let location = value.get("location")?;
        let script_id = location.get("scriptId")?.as_str()?.to_string();
        let line = location.get("lineNumber")?.as_u64()? as u32;
        let column = location
            .get("columnNumber")
            .and_then(|v| v.as_u64())
            .unwrap_or(0) as u32;
        let scopes = value
            .get("scopeChain")
            .and_then(|v| v.as_array())
            .map(|arr| arr.iter().filter_map(Scope::from_cdp).collect())
            .unwrap_or_default();
        Some(Self {
            id,
            function_name,
            script_id,
            url: None,
            line,
            column,
            scopes,
        })
    }
}

/// Decoded list of stack frames.
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct CallStack {
    pub frames: Vec<CallFrame>,
    /// Index of the currently selected frame for variable / evaluate ops.
    pub selected: usize,
}

impl CallStack {
    /// Parse the full `Debugger.paused.callFrames` array.
    pub fn from_cdp(frames: &Value) -> Self {
        let frames = frames
            .as_array()
            .map(|arr| arr.iter().filter_map(CallFrame::from_cdp).collect())
            .unwrap_or_default();
        Self {
            frames,
            selected: 0,
        }
    }

    pub fn is_empty(&self) -> bool {
        self.frames.is_empty()
    }

    pub fn len(&self) -> usize {
        self.frames.len()
    }

    /// The currently selected frame, if any.
    pub fn selected_frame(&self) -> Option<&CallFrame> {
        self.frames.get(self.selected)
    }

    /// Move the selection to `index`, clamped to the valid range. No-op
    /// when the stack is empty.
    pub fn select(&mut self, index: usize) {
        if self.frames.is_empty() {
            return;
        }
        self.selected = index.min(self.frames.len() - 1);
    }

    /// Drop all frames. Used on resume / disconnect.
    pub fn clear(&mut self) {
        self.frames.clear();
        self.selected = 0;
    }
}

// ---------------------------------------------------------------------------
// Scopes
// ---------------------------------------------------------------------------

/// Kinds of scopes CDP exposes. We mirror the common subset; unknown
/// values fall back to [`ScopeKind::Other`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ScopeKind {
    Local,
    Closure,
    Global,
    Block,
    Catch,
    With,
    Module,
    Script,
    Eval,
    Other,
}

impl ScopeKind {
    pub fn parse(s: &str) -> Self {
        match s {
            "local" => ScopeKind::Local,
            "closure" => ScopeKind::Closure,
            "global" => ScopeKind::Global,
            "block" => ScopeKind::Block,
            "catch" => ScopeKind::Catch,
            "with" => ScopeKind::With,
            "module" => ScopeKind::Module,
            "script" => ScopeKind::Script,
            "eval" => ScopeKind::Eval,
            _ => ScopeKind::Other,
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            ScopeKind::Local => "Local",
            ScopeKind::Closure => "Closure",
            ScopeKind::Global => "Global",
            ScopeKind::Block => "Block",
            ScopeKind::Catch => "Catch",
            ScopeKind::With => "With",
            ScopeKind::Module => "Module",
            ScopeKind::Script => "Script",
            ScopeKind::Eval => "Eval",
            ScopeKind::Other => "Other",
        }
    }
}

/// One scope inside a [`CallFrame`]. The actual property list is fetched
/// lazily via `Runtime.getProperties` against [`Scope::object_id`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Scope {
    pub kind: ScopeKind,
    /// CDP `objectId` of the scope object. Used to fetch properties.
    pub object_id: String,
    /// Optional human label (e.g. "Block", function name for closures).
    pub name: Option<String>,
}

impl Scope {
    pub fn from_cdp(value: &Value) -> Option<Self> {
        let kind = value
            .get("type")
            .and_then(|v| v.as_str())
            .map(ScopeKind::parse)
            .unwrap_or(ScopeKind::Other);
        let object_id = value
            .get("object")
            .and_then(|o| o.get("objectId"))
            .and_then(|v| v.as_str())?
            .to_string();
        let name = value
            .get("name")
            .and_then(|v| v.as_str())
            .map(|s| s.to_string());
        Some(Self {
            kind,
            object_id,
            name,
        })
    }
}

// ---------------------------------------------------------------------------
// Remote values + properties
// ---------------------------------------------------------------------------

/// Simplified `RemoteObject` for display. Original CDP type is richer; we
/// keep only what the variables pane needs.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RemoteValue {
    /// Display string -- `"hello"` for strings, `42` for numbers, `Array(3)`
    /// for arrays etc. Built by `RemoteValue::display_for`.
    pub display: String,
    /// CDP `objectId` when the value is expandable (object/array/function).
    pub object_id: Option<String>,
    /// Whether the value can be expanded to fetch properties.
    pub expandable: bool,
    /// Original CDP `type` ("string"/"number"/"object"/...) for theming.
    pub type_str: String,
}

impl RemoteValue {
    /// Build a `RemoteValue` from a CDP `RemoteObject` JSON payload.
    pub fn from_cdp(value: &Value) -> Self {
        let type_str = value
            .get("type")
            .and_then(|v| v.as_str())
            .unwrap_or("undefined")
            .to_string();
        let object_id = value
            .get("objectId")
            .and_then(|v| v.as_str())
            .map(|s| s.to_string());
        let display = Self::display_for(value);
        let expandable = matches!(type_str.as_str(), "object" | "function") && object_id.is_some();
        Self {
            display,
            object_id,
            expandable,
            type_str,
        }
    }

    fn display_for(value: &Value) -> String {
        // BigInt, NaN, Infinity, -0 arrive here -- they have no JSON-serializable
        // `value`, only this side channel. Use it verbatim ("123n", "NaN", ...).
        if let Some(uv) = value.get("unserializableValue").and_then(|v| v.as_str()) {
            return uv.to_string();
        }
        if let Some(v) = value.get("value") {
            return match v {
                Value::String(s) => format_string_literal(s),
                Value::Number(n) => n.to_string(),
                Value::Bool(b) => b.to_string(),
                Value::Null => "null".into(),
                other => other.to_string(),
            };
        }
        // Map/Set previews carry the first few entries; surface them so the
        // pill reads `Map(2) {"a" => 1, "b" => 2}` instead of just `Map(2)`.
        if let Some(preview) = value.get("preview") {
            if let Some(s) = format_preview(preview) {
                return s;
            }
        }
        if let Some(d) = value.get("description").and_then(|v| v.as_str()) {
            return format_description(d);
        }
        if let Some(t) = value.get("type").and_then(|v| v.as_str()) {
            format!("[{t}]")
        } else {
            "undefined".into()
        }
    }
}

/// Max characters of payload to keep before appending an ellipsis. Tuned for
/// the inline-pill width; watches and scopes accept the same cutoff so a value
/// looks identical no matter which pane surfaces it.
const MAX_INLINE_LEN: usize = 80;

fn truncate_chars(s: &str, max: usize) -> String {
    if s.chars().count() <= max {
        return s.to_string();
    }
    let head: String = s.chars().take(max).collect();
    format!("{head}...")
}

fn format_string_literal(s: &str) -> String {
    // Strip newlines/tabs/CRs so a multi-line string literal stays on one row
    // -- the pill is single-line and otherwise visually explodes.
    let flattened: String = s
        .chars()
        .map(|c| match c {
            '\n' | '\r' | '\t' => ' ',
            c => c,
        })
        .collect();
    format!("\"{}\"", truncate_chars(&flattened, MAX_INLINE_LEN))
}

fn format_description(d: &str) -> String {
    // Errors carry their full stack in `description`; functions carry the
    // whole body. Collapse to the first line so the pill stays single-row.
    let first = d.lines().next().unwrap_or(d);
    truncate_chars(first, MAX_INLINE_LEN)
}

fn format_preview(preview: &Value) -> Option<String> {
    let subtype = preview.get("subtype").and_then(|v| v.as_str())?;
    let entries = preview.get("entries").and_then(|v| v.as_array())?;
    let desc = preview
        .get("description")
        .and_then(|v| v.as_str())
        .unwrap_or(match subtype {
            "map" => "Map",
            "set" => "Set",
            _ => return None,
        });
    if entries.is_empty() {
        return Some(desc.to_string());
    }
    let mut items: Vec<String> = Vec::new();
    for entry in entries.iter().take(3) {
        match subtype {
            "map" => {
                let k = entry.get("key").map(preview_entry_short)?;
                let v = entry.get("value").map(preview_entry_short)?;
                items.push(format!("{k} => {v}"));
            }
            "set" => {
                let v = entry.get("value").map(preview_entry_short)?;
                items.push(v);
            }
            _ => return None,
        }
    }
    let body = if entries.len() > 3 {
        format!("{}, ...", items.join(", "))
    } else {
        items.join(", ")
    };
    Some(format!("{desc} {{{body}}}"))
}

fn preview_entry_short(v: &Value) -> String {
    let t = v.get("type").and_then(|x| x.as_str()).unwrap_or("");
    if let Some(raw) = v.get("value").and_then(|x| x.as_str()) {
        return if t == "string" {
            format!("\"{raw}\"")
        } else {
            raw.to_string()
        };
    }
    if let Some(d) = v.get("description").and_then(|x| x.as_str()) {
        return d.lines().next().unwrap_or(d).to_string();
    }
    format!("[{t}]")
}

/// One row in a `Runtime.getProperties` response.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Property {
    pub name: String,
    pub value: Option<RemoteValue>,
    /// True for own (vs inherited) properties.
    pub own: bool,
    /// True when CDP marks the property as enumerable.
    pub enumerable: bool,
}

impl Property {
    pub fn from_cdp(value: &Value) -> Option<Self> {
        let name = value.get("name")?.as_str()?.to_string();
        let val = value.get("value").map(RemoteValue::from_cdp);
        let own = value.get("isOwn").and_then(|v| v.as_bool()).unwrap_or(true);
        let enumerable = value
            .get("enumerable")
            .and_then(|v| v.as_bool())
            .unwrap_or(true);
        Some(Self {
            name,
            value: val,
            own,
            enumerable,
        })
    }

    /// Parse the `result` array from a `Runtime.getProperties` response.
    pub fn parse_response(result: &Value) -> Vec<Self> {
        result
            .get("result")
            .and_then(|v| v.as_array())
            .map(|arr| arr.iter().filter_map(Self::from_cdp).collect())
            .unwrap_or_default()
    }
}

// ---------------------------------------------------------------------------
// Watch expressions
// ---------------------------------------------------------------------------

/// One user-typed watch expression and its last evaluated value.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Watch {
    pub expression: String,
    /// `Some(Ok(...))` after a successful evaluate; `Some(Err(...))` on
    /// CDP exception; `None` before first evaluation.
    pub last: Option<Result<RemoteValue, String>>,
}

impl Watch {
    pub fn new(expression: impl Into<String>) -> Self {
        Self {
            expression: expression.into(),
            last: None,
        }
    }
}

/// User-managed list of watch expressions.
#[derive(Debug, Default, Clone)]
pub struct WatchList {
    items: Vec<Watch>,
}

impl WatchList {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn add(&mut self, expression: impl Into<String>) -> usize {
        self.items.push(Watch::new(expression));
        self.items.len() - 1
    }

    pub fn remove(&mut self, index: usize) -> Option<Watch> {
        if index < self.items.len() {
            Some(self.items.remove(index))
        } else {
            None
        }
    }

    pub fn iter(&self) -> impl Iterator<Item = &Watch> {
        self.items.iter()
    }

    pub fn iter_mut(&mut self) -> impl Iterator<Item = &mut Watch> {
        self.items.iter_mut()
    }

    pub fn len(&self) -> usize {
        self.items.len()
    }

    pub fn is_empty(&self) -> bool {
        self.items.is_empty()
    }

    pub fn get(&self, index: usize) -> Option<&Watch> {
        self.items.get(index)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn frame_payload() -> Value {
        serde_json::json!({
            "callFrameId": "frame-1",
            "functionName": "checkout",
            "location": {"scriptId": "42", "lineNumber": 17, "columnNumber": 4},
            "scopeChain": [
                {
                    "type": "local",
                    "object": {"type": "object", "objectId": "obj-local"}
                },
                {
                    "type": "closure",
                    "object": {"type": "object", "objectId": "obj-closure"},
                    "name": "outer"
                },
                {
                    "type": "global",
                    "object": {"type": "object", "objectId": "obj-global"}
                }
            ]
        })
    }

    #[test]
    fn parse_call_frame_full() {
        let f = CallFrame::from_cdp(&frame_payload()).unwrap();
        assert_eq!(f.id, "frame-1");
        assert_eq!(f.function_name, "checkout");
        assert_eq!(f.script_id, "42");
        assert_eq!(f.line, 17);
        assert_eq!(f.column, 4);
        assert_eq!(f.scopes.len(), 3);
        assert_eq!(f.scopes[0].kind, ScopeKind::Local);
        assert_eq!(f.scopes[1].kind, ScopeKind::Closure);
        assert_eq!(f.scopes[1].name.as_deref(), Some("outer"));
        assert_eq!(f.scopes[2].kind, ScopeKind::Global);
    }

    #[test]
    fn parse_call_stack_and_selection() {
        let frames = serde_json::json!([frame_payload(), frame_payload()]);
        let mut stack = CallStack::from_cdp(&frames);
        assert_eq!(stack.len(), 2);
        assert_eq!(stack.selected, 0);
        stack.select(1);
        assert_eq!(stack.selected, 1);
        stack.select(99); // clamps
        assert_eq!(stack.selected, 1);
        stack.clear();
        assert!(stack.is_empty());
    }

    #[test]
    fn scope_kind_round_trip() {
        for s in [
            "local", "closure", "global", "block", "catch", "with", "module", "script", "eval",
        ] {
            assert_ne!(ScopeKind::parse(s).label(), "Other");
        }
        assert_eq!(ScopeKind::parse("nonsense"), ScopeKind::Other);
    }

    #[test]
    fn remote_value_string() {
        let v = serde_json::json!({"type": "string", "value": "hi"});
        let r = RemoteValue::from_cdp(&v);
        assert_eq!(r.display, "\"hi\"");
        assert!(!r.expandable);
        assert!(r.object_id.is_none());
    }

    #[test]
    fn remote_value_object_expandable() {
        let v = serde_json::json!({
            "type": "object",
            "objectId": "obj-1",
            "description": "Array(3)"
        });
        let r = RemoteValue::from_cdp(&v);
        assert_eq!(r.display, "Array(3)");
        assert!(r.expandable);
        assert_eq!(r.object_id.as_deref(), Some("obj-1"));
    }

    #[test]
    fn remote_value_function_expandable() {
        let v = serde_json::json!({
            "type": "function",
            "objectId": "obj-2",
            "description": "function checkout()"
        });
        let r = RemoteValue::from_cdp(&v);
        assert!(r.expandable);
    }

    #[test]
    fn remote_value_undefined_fallback() {
        let v = serde_json::json!({"type": "undefined"});
        let r = RemoteValue::from_cdp(&v);
        assert_eq!(r.display, "[undefined]");
    }

    #[test]
    fn remote_value_bigint_via_unserializable() {
        let v = serde_json::json!({
            "type": "bigint",
            "unserializableValue": "9007199254740993n",
            "description": "9007199254740993n",
        });
        let r = RemoteValue::from_cdp(&v);
        assert_eq!(r.display, "9007199254740993n");
    }

    #[test]
    fn remote_value_nan_and_infinity() {
        for raw in ["NaN", "Infinity", "-Infinity", "-0"] {
            let v = serde_json::json!({"type": "number", "unserializableValue": raw});
            assert_eq!(RemoteValue::from_cdp(&v).display, raw);
        }
    }

    #[test]
    fn remote_value_long_string_truncated() {
        let long: String = "x".repeat(200);
        let v = serde_json::json!({"type": "string", "value": long});
        let r = RemoteValue::from_cdp(&v);
        // Quotes + 80 chars + "..." == 84 chars total.
        assert!(r.display.starts_with('"'));
        assert!(r.display.ends_with("...\""));
        assert_eq!(r.display.chars().count(), 1 + 80 + 3 + 1);
    }

    #[test]
    fn remote_value_multiline_string_flattened() {
        let v = serde_json::json!({"type": "string", "value": "line1\nline2\tend"});
        let r = RemoteValue::from_cdp(&v);
        assert_eq!(r.display, "\"line1 line2 end\"");
    }

    #[test]
    fn remote_value_error_description_collapsed() {
        let v = serde_json::json!({
            "type": "object",
            "subtype": "error",
            "objectId": "obj-err",
            "description": "Error: kaboom\n    at checkout (app.ts:17:4)\n    at run (app.ts:3:2)",
        });
        let r = RemoteValue::from_cdp(&v);
        assert_eq!(r.display, "Error: kaboom");
        assert!(r.expandable);
    }

    #[test]
    fn remote_value_long_description_truncated() {
        let body: String = "a".repeat(150);
        let desc = format!("function huge() {{ {body} }}");
        let v = serde_json::json!({
            "type": "function",
            "objectId": "obj-fn",
            "description": desc,
        });
        let r = RemoteValue::from_cdp(&v);
        assert!(r.display.ends_with("..."));
        assert_eq!(r.display.chars().count(), 80 + 3);
    }

    #[test]
    fn remote_value_map_preview_entries() {
        let v = serde_json::json!({
            "type": "object",
            "subtype": "map",
            "objectId": "obj-map",
            "description": "Map(2)",
            "preview": {
                "subtype": "map",
                "description": "Map(2)",
                "entries": [
                    {
                        "key":   {"type": "string", "value": "a"},
                        "value": {"type": "number", "value": "1"}
                    },
                    {
                        "key":   {"type": "string", "value": "b"},
                        "value": {"type": "number", "value": "2"}
                    }
                ]
            }
        });
        let r = RemoteValue::from_cdp(&v);
        assert_eq!(r.display, "Map(2) {\"a\" => 1, \"b\" => 2}");
    }

    #[test]
    fn remote_value_set_preview_entries() {
        let v = serde_json::json!({
            "type": "object",
            "subtype": "set",
            "objectId": "obj-set",
            "description": "Set(3)",
            "preview": {
                "subtype": "set",
                "description": "Set(3)",
                "entries": [
                    {"value": {"type": "string", "value": "x"}},
                    {"value": {"type": "string", "value": "y"}},
                    {"value": {"type": "string", "value": "z"}}
                ]
            }
        });
        let r = RemoteValue::from_cdp(&v);
        assert_eq!(r.display, "Set(3) {\"x\", \"y\", \"z\"}");
    }

    #[test]
    fn remote_value_map_preview_overflows_with_ellipsis() {
        let v = serde_json::json!({
            "type": "object",
            "subtype": "map",
            "objectId": "obj-map-big",
            "preview": {
                "subtype": "map",
                "description": "Map(5)",
                "entries": [
                    {"key": {"type": "string", "value": "a"}, "value": {"type": "number", "value": "1"}},
                    {"key": {"type": "string", "value": "b"}, "value": {"type": "number", "value": "2"}},
                    {"key": {"type": "string", "value": "c"}, "value": {"type": "number", "value": "3"}},
                    {"key": {"type": "string", "value": "d"}, "value": {"type": "number", "value": "4"}},
                    {"key": {"type": "string", "value": "e"}, "value": {"type": "number", "value": "5"}}
                ]
            }
        });
        let r = RemoteValue::from_cdp(&v);
        assert!(r
            .display
            .starts_with("Map(5) {\"a\" => 1, \"b\" => 2, \"c\" => 3, ..."));
        assert!(r.display.ends_with("...}"));
    }

    #[test]
    fn remote_value_empty_map_preview_falls_back_to_description() {
        let v = serde_json::json!({
            "type": "object",
            "subtype": "map",
            "objectId": "obj-empty-map",
            "preview": {
                "subtype": "map",
                "description": "Map(0)",
                "entries": []
            }
        });
        let r = RemoteValue::from_cdp(&v);
        assert_eq!(r.display, "Map(0)");
    }

    #[test]
    fn parse_properties_response() {
        let resp = serde_json::json!({
            "result": [
                {
                    "name": "items",
                    "value": {"type": "object", "objectId": "obj-x", "description": "Array(3)"},
                    "isOwn": true,
                    "enumerable": true
                },
                {
                    "name": "total",
                    "value": {"type": "number", "value": 126.5},
                    "isOwn": true,
                    "enumerable": true
                }
            ]
        });
        let props = Property::parse_response(&resp);
        assert_eq!(props.len(), 2);
        assert_eq!(props[0].name, "items");
        assert!(props[0].value.as_ref().unwrap().expandable);
        assert_eq!(props[1].name, "total");
        assert_eq!(props[1].value.as_ref().unwrap().display, "126.5");
    }

    #[test]
    fn property_default_own_enumerable() {
        let v = serde_json::json!({"name": "x", "value": {"type": "number", "value": 1}});
        let p = Property::from_cdp(&v).unwrap();
        assert!(p.own);
        assert!(p.enumerable);
    }

    #[test]
    fn watchlist_lifecycle() {
        let mut w = WatchList::new();
        assert!(w.is_empty());
        let idx = w.add("count * 2");
        assert_eq!(idx, 0);
        assert_eq!(w.len(), 1);
        let removed = w.remove(0).unwrap();
        assert_eq!(removed.expression, "count * 2");
        assert!(w.is_empty());
        assert!(w.remove(0).is_none());
    }
}
