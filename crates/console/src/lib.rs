//! Console log model for atomio.
//!
//! [`Console`] is a ring-buffered list of [`LogEntry`] records produced by
//! the running app under debug. Each entry carries a [`LogLevel`], a
//! pre-formatted message string, an optional source location, and a
//! monotonic sequence number so the UI can stable-key entries across
//! re-renders.
//!
//! The crate is **pure model** -- no gpui, no async, no I/O. Higher layers
//! feed it [`LogEntry`] values parsed from CDP `Runtime.consoleAPICalled`
//! events, and the UI reads its `entries()` slice for rendering.
//!
//! ### Capacity
//!
//! The console is bounded by [`Console::with_capacity`] (default 5000 in
//! [`Console::new`]). When full, the oldest entry is dropped on push --
//! same model as a circular buffer. Sequence numbers are never reused, so
//! the UI can detect "you missed N entries" by gap detection if needed.

use serde_json::Value;

/// Severity level of a console entry, mirroring CDP `Runtime.consoleAPICalled.type`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum LogLevel {
    Log,
    Info,
    Warn,
    Error,
    Debug,
    Trace,
}

impl LogLevel {
    /// Parse a CDP console type string. Unknown values fall back to `Log`.
    pub fn from_cdp(s: &str) -> Self {
        match s {
            "info" => LogLevel::Info,
            "warning" | "warn" => LogLevel::Warn,
            "error" => LogLevel::Error,
            "debug" => LogLevel::Debug,
            "trace" => LogLevel::Trace,
            _ => LogLevel::Log,
        }
    }

    /// Short single-letter tag for compact UI rendering ("L", "W", "E"...).
    pub fn tag(&self) -> &'static str {
        match self {
            LogLevel::Log => "LOG",
            LogLevel::Info => "INF",
            LogLevel::Warn => "WRN",
            LogLevel::Error => "ERR",
            LogLevel::Debug => "DBG",
            LogLevel::Trace => "TRC",
        }
    }
}

/// Optional source location attached to a log entry.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SourceLocation {
    /// Source URL or file path (whatever the CDP event reports).
    pub url: String,
    /// Zero-based line number.
    pub line: u32,
    /// Zero-based column number.
    pub column: u32,
}

/// A single console entry.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LogEntry {
    /// Monotonic sequence number assigned by [`Console::push`]. Unique within
    /// a `Console` instance for its lifetime, even after wrap-around eviction.
    pub seq: u64,
    pub level: LogLevel,
    /// Pre-formatted message body (joined args, with simple value formatting).
    pub message: String,
    pub location: Option<SourceLocation>,
}

/// Format a CDP `Runtime.RemoteObject` argument into a display string.
///
/// CDP encodes console.log args as `{type, value, description, ...}`. We
/// pull `value` (for primitives) or `description` (for objects/functions).
pub fn format_remote_object(arg: &Value) -> String {
    if let Some(v) = arg.get("value") {
        match v {
            Value::String(s) => s.clone(),
            Value::Number(n) => n.to_string(),
            Value::Bool(b) => b.to_string(),
            Value::Null => "null".into(),
            other => other.to_string(),
        }
    } else if let Some(desc) = arg.get("description").and_then(|v| v.as_str()) {
        desc.to_string()
    } else if let Some(t) = arg.get("type").and_then(|v| v.as_str()) {
        format!("[{t}]")
    } else {
        "<unknown>".into()
    }
}

/// Build a [`LogEntry`] from a parsed CDP `Runtime.consoleAPICalled` event payload.
///
/// Returns `None` when the params object is missing required fields. The
/// caller still needs to feed it into a [`Console`] to assign a sequence number.
pub fn entry_from_console_api_called(
    params: &Value,
) -> Option<(LogLevel, String, Option<SourceLocation>)> {
    let level_str = params.get("type").and_then(|v| v.as_str()).unwrap_or("log");
    let level = LogLevel::from_cdp(level_str);

    let args = params.get("args").and_then(|v| v.as_array());
    let message = match args {
        Some(arr) if !arr.is_empty() => arr
            .iter()
            .map(format_remote_object)
            .collect::<Vec<_>>()
            .join(" "),
        _ => String::new(),
    };

    let location = params.get("stackTrace").and_then(|st| {
        st.get("callFrames")
            .and_then(|cf| cf.as_array())
            .and_then(|arr| arr.first())
            .and_then(|frame| {
                let url = frame.get("url")?.as_str()?.to_string();
                let line = frame.get("lineNumber")?.as_u64()? as u32;
                let column = frame.get("columnNumber")?.as_u64().unwrap_or(0) as u32;
                Some(SourceLocation { url, line, column })
            })
    });

    Some((level, message, location))
}

/// Bounded console log buffer.
#[derive(Debug, Clone)]
pub struct Console {
    entries: Vec<LogEntry>,
    capacity: usize,
    next_seq: u64,
}

impl Console {
    /// Create a console with the default 5000-entry capacity.
    pub fn new() -> Self {
        Self::with_capacity(5000)
    }

    /// Create a console with an explicit capacity.
    pub fn with_capacity(capacity: usize) -> Self {
        Self {
            entries: Vec::with_capacity(capacity.min(1024)),
            capacity: capacity.max(1),
            next_seq: 0,
        }
    }

    /// Push a new entry. Returns the assigned sequence number.
    ///
    /// Evicts the oldest entry if at capacity.
    pub fn push(
        &mut self,
        level: LogLevel,
        message: impl Into<String>,
        location: Option<SourceLocation>,
    ) -> u64 {
        let seq = self.next_seq;
        self.next_seq += 1;
        let entry = LogEntry {
            seq,
            level,
            message: message.into(),
            location,
        };
        if self.entries.len() >= self.capacity {
            self.entries.remove(0);
        }
        self.entries.push(entry);
        seq
    }

    /// Borrow the entries currently in the buffer, oldest first.
    pub fn entries(&self) -> &[LogEntry] {
        &self.entries
    }

    /// Number of entries currently held.
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    /// True when there are no entries.
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    /// Drop all entries. Sequence counter is preserved so the UI can detect
    /// the discontinuity if it cares.
    pub fn clear(&mut self) {
        self.entries.clear();
    }
}

impl Default for Console {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn level_from_cdp() {
        assert_eq!(LogLevel::from_cdp("log"), LogLevel::Log);
        assert_eq!(LogLevel::from_cdp("info"), LogLevel::Info);
        assert_eq!(LogLevel::from_cdp("warning"), LogLevel::Warn);
        assert_eq!(LogLevel::from_cdp("warn"), LogLevel::Warn);
        assert_eq!(LogLevel::from_cdp("error"), LogLevel::Error);
        assert_eq!(LogLevel::from_cdp("debug"), LogLevel::Debug);
        assert_eq!(LogLevel::from_cdp("trace"), LogLevel::Trace);
        assert_eq!(LogLevel::from_cdp("nonsense"), LogLevel::Log);
    }

    #[test]
    fn format_string_arg() {
        let v = serde_json::json!({"type": "string", "value": "hello"});
        assert_eq!(format_remote_object(&v), "hello");
    }

    #[test]
    fn format_number_arg() {
        let v = serde_json::json!({"type": "number", "value": 42});
        assert_eq!(format_remote_object(&v), "42");
    }

    #[test]
    fn format_object_arg_uses_description() {
        let v = serde_json::json!({"type": "object", "description": "Array(3)"});
        assert_eq!(format_remote_object(&v), "Array(3)");
    }

    #[test]
    fn format_unknown_arg() {
        let v = serde_json::json!({});
        assert_eq!(format_remote_object(&v), "<unknown>");
    }

    #[test]
    fn entry_from_full_payload() {
        let params = serde_json::json!({
            "type": "warn",
            "args": [
                {"type": "string", "value": "deprecated:"},
                {"type": "string", "value": "use foo() instead"}
            ],
            "stackTrace": {
                "callFrames": [{
                    "url": "app.bundle.js",
                    "lineNumber": 100,
                    "columnNumber": 5
                }]
            }
        });
        let (level, msg, loc) = entry_from_console_api_called(&params).unwrap();
        assert_eq!(level, LogLevel::Warn);
        assert_eq!(msg, "deprecated: use foo() instead");
        let loc = loc.unwrap();
        assert_eq!(loc.url, "app.bundle.js");
        assert_eq!(loc.line, 100);
        assert_eq!(loc.column, 5);
    }

    #[test]
    fn entry_without_stack_trace() {
        let params = serde_json::json!({
            "type": "log",
            "args": [{"type": "string", "value": "hi"}]
        });
        let (_, msg, loc) = entry_from_console_api_called(&params).unwrap();
        assert_eq!(msg, "hi");
        assert!(loc.is_none());
    }

    #[test]
    fn console_assigns_unique_seqs() {
        let mut c = Console::new();
        let a = c.push(LogLevel::Log, "first", None);
        let b = c.push(LogLevel::Log, "second", None);
        assert_eq!(a, 0);
        assert_eq!(b, 1);
        assert_ne!(a, b);
    }

    #[test]
    fn console_evicts_at_capacity() {
        let mut c = Console::with_capacity(2);
        c.push(LogLevel::Log, "a", None);
        c.push(LogLevel::Log, "b", None);
        c.push(LogLevel::Log, "c", None);
        assert_eq!(c.len(), 2);
        // Oldest ("a") evicted; "b" and "c" remain.
        assert_eq!(c.entries()[0].message, "b");
        assert_eq!(c.entries()[1].message, "c");
        // Seq numbers preserved on the survivors.
        assert_eq!(c.entries()[0].seq, 1);
        assert_eq!(c.entries()[1].seq, 2);
    }

    #[test]
    fn console_clear_preserves_seq_counter() {
        let mut c = Console::new();
        c.push(LogLevel::Log, "x", None);
        c.clear();
        let next = c.push(LogLevel::Log, "y", None);
        assert_eq!(next, 1);
    }

    #[test]
    fn level_tags_distinct() {
        let tags = [
            LogLevel::Log,
            LogLevel::Info,
            LogLevel::Warn,
            LogLevel::Error,
            LogLevel::Debug,
            LogLevel::Trace,
        ]
        .iter()
        .map(|l| l.tag())
        .collect::<Vec<_>>();
        let mut sorted = tags.clone();
        sorted.sort();
        sorted.dedup();
        assert_eq!(sorted.len(), tags.len());
    }
}
