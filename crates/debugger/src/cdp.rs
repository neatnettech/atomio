//! CDP JSON-RPC message types.
//!
//! The Chrome DevTools Protocol uses a simple JSON-RPC-like framing over
//! WebSocket. Each message from the client is a request with a numeric `id`;
//! the server responds with a matching `id` plus a `result` or `error`.
//! Server-initiated notifications ("events") have a `method` and `params`
//! but no `id`.
//!
//! We model the subset of the protocol that Hermes exposes. Unknown methods
//! are preserved as raw `serde_json::Value` so the transport layer never
//! drops data.

use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::sync::atomic::{AtomicU64, Ordering};

/// Global request-ID counter. Each outgoing request gets a unique ID.
static NEXT_ID: AtomicU64 = AtomicU64::new(1);

/// Allocate the next unique request ID.
pub fn next_request_id() -> u64 {
    NEXT_ID.fetch_add(1, Ordering::Relaxed)
}

// ---------------------------------------------------------------------------
// Outgoing requests
// ---------------------------------------------------------------------------

/// A CDP JSON-RPC request sent to the debug target.
#[derive(Debug, Clone, Serialize)]
pub struct CdpRequest {
    pub id: u64,
    pub method: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub params: Option<Value>,
}

impl CdpRequest {
    /// Create a request with no params.
    pub fn new(method: impl Into<String>) -> Self {
        Self {
            id: next_request_id(),
            method: method.into(),
            params: None,
        }
    }

    /// Create a request with params.
    pub fn with_params(method: impl Into<String>, params: Value) -> Self {
        Self {
            id: next_request_id(),
            method: method.into(),
            params: Some(params),
        }
    }
}

// ---------------------------------------------------------------------------
// Incoming messages
// ---------------------------------------------------------------------------

/// A raw incoming CDP message, before we know whether it's a response or event.
#[derive(Debug, Clone, Deserialize)]
pub struct RawCdpMessage {
    /// Present on responses, absent on events.
    pub id: Option<u64>,
    /// Present on events and (redundantly) on some responses.
    pub method: Option<String>,
    /// Response payload on success.
    pub result: Option<Value>,
    /// Response payload on error.
    pub error: Option<CdpError>,
    /// Event parameters.
    pub params: Option<Value>,
}

/// CDP error object.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct CdpError {
    pub code: i64,
    pub message: String,
    pub data: Option<Value>,
}

/// Parsed CDP message -- either a response to one of our requests or a
/// server-pushed event.
#[derive(Debug, Clone)]
pub enum CdpMessage {
    /// Response to a request we sent.
    Response {
        id: u64,
        result: Result<Value, CdpError>,
    },
    /// Server-initiated event.
    Event { method: String, params: Value },
}

impl CdpMessage {
    /// Parse a raw incoming message into a typed variant.
    pub fn from_raw(raw: RawCdpMessage) -> Option<Self> {
        if let Some(id) = raw.id {
            // It's a response.
            let result = if let Some(err) = raw.error {
                Err(err)
            } else {
                Ok(raw.result.unwrap_or(Value::Null))
            };
            Some(CdpMessage::Response { id, result })
        } else if let Some(method) = raw.method {
            // It's an event.
            Some(CdpMessage::Event {
                method,
                params: raw.params.unwrap_or(Value::Null),
            })
        } else {
            None
        }
    }
}

// ---------------------------------------------------------------------------
// Convenience builders for common CDP methods
// ---------------------------------------------------------------------------

/// `Runtime.enable` -- start receiving runtime events.
pub fn runtime_enable() -> CdpRequest {
    CdpRequest::new("Runtime.enable")
}

/// `Debugger.enable` -- start receiving debugger events (scriptParsed, paused, etc.).
pub fn debugger_enable() -> CdpRequest {
    CdpRequest::new("Debugger.enable")
}

/// `Debugger.setBreakpointByUrl` -- set a breakpoint by source URL and line.
pub fn set_breakpoint_by_url(url: &str, line: u32, column: Option<u32>) -> CdpRequest {
    let mut params = serde_json::json!({
        "url": url,
        "lineNumber": line,
    });
    if let Some(col) = column {
        params["columnNumber"] = serde_json::json!(col);
    }
    CdpRequest::with_params("Debugger.setBreakpointByUrl", params)
}

/// `Debugger.removeBreakpoint` -- remove a breakpoint by its server-assigned ID.
pub fn remove_breakpoint(breakpoint_id: &str) -> CdpRequest {
    CdpRequest::with_params(
        "Debugger.removeBreakpoint",
        serde_json::json!({ "breakpointId": breakpoint_id }),
    )
}

/// `Debugger.resume` -- continue execution after a pause.
pub fn debugger_resume() -> CdpRequest {
    CdpRequest::new("Debugger.resume")
}

/// `Debugger.stepOver` -- step to the next statement.
pub fn debugger_step_over() -> CdpRequest {
    CdpRequest::new("Debugger.stepOver")
}

/// `Debugger.stepInto` -- step into a function call.
pub fn debugger_step_into() -> CdpRequest {
    CdpRequest::new("Debugger.stepInto")
}

/// `Debugger.stepOut` -- step out of the current function.
pub fn debugger_step_out() -> CdpRequest {
    CdpRequest::new("Debugger.stepOut")
}

/// `Debugger.pause` -- pause execution immediately.
pub fn debugger_pause() -> CdpRequest {
    CdpRequest::new("Debugger.pause")
}

/// `Runtime.getProperties` -- get properties of an object by its remote ID.
pub fn get_properties(object_id: &str, own_properties: bool) -> CdpRequest {
    CdpRequest::with_params(
        "Runtime.getProperties",
        serde_json::json!({
            "objectId": object_id,
            "ownProperties": own_properties,
        }),
    )
}

/// `Runtime.evaluate` -- evaluate an expression in the current context.
pub fn evaluate(expression: &str) -> CdpRequest {
    CdpRequest::with_params(
        "Runtime.evaluate",
        serde_json::json!({ "expression": expression }),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn request_serializes_without_params() {
        let req = CdpRequest {
            id: 1,
            method: "Runtime.enable".into(),
            params: None,
        };
        let json = serde_json::to_value(&req).unwrap();
        assert_eq!(json["method"], "Runtime.enable");
        assert_eq!(json["id"], 1);
        assert!(json.get("params").is_none());
    }

    #[test]
    fn request_serializes_with_params() {
        let req = set_breakpoint_by_url("file:///app.js", 10, Some(5));
        let json = serde_json::to_value(&req).unwrap();
        assert_eq!(json["method"], "Debugger.setBreakpointByUrl");
        assert_eq!(json["params"]["lineNumber"], 10);
        assert_eq!(json["params"]["columnNumber"], 5);
    }

    #[test]
    fn parse_response_success() {
        let raw: RawCdpMessage =
            serde_json::from_str(r#"{"id": 1, "result": {"debuggerId": "abc"}}"#).unwrap();
        let msg = CdpMessage::from_raw(raw).unwrap();
        match msg {
            CdpMessage::Response { id, result } => {
                assert_eq!(id, 1);
                assert!(result.is_ok());
            }
            _ => panic!("expected Response"),
        }
    }

    #[test]
    fn parse_response_error() {
        let raw: RawCdpMessage = serde_json::from_str(
            r#"{"id": 2, "error": {"code": -32601, "message": "method not found"}}"#,
        )
        .unwrap();
        let msg = CdpMessage::from_raw(raw).unwrap();
        match msg {
            CdpMessage::Response { id, result } => {
                assert_eq!(id, 2);
                let err = result.unwrap_err();
                assert_eq!(err.code, -32601);
            }
            _ => panic!("expected Response"),
        }
    }

    #[test]
    fn parse_event() {
        let raw: RawCdpMessage = serde_json::from_str(
            r#"{"method": "Debugger.scriptParsed", "params": {"scriptId": "42", "url": "app.js"}}"#,
        )
        .unwrap();
        let msg = CdpMessage::from_raw(raw).unwrap();
        match msg {
            CdpMessage::Event { method, params } => {
                assert_eq!(method, "Debugger.scriptParsed");
                assert_eq!(params["scriptId"], "42");
            }
            _ => panic!("expected Event"),
        }
    }

    #[test]
    fn parse_console_event() {
        let raw: RawCdpMessage = serde_json::from_str(
            r#"{"method": "Runtime.consoleAPICalled", "params": {"type": "log", "args": [{"type": "string", "value": "hello"}]}}"#,
        )
        .unwrap();
        let msg = CdpMessage::from_raw(raw).unwrap();
        match msg {
            CdpMessage::Event { method, params } => {
                assert_eq!(method, "Runtime.consoleAPICalled");
                assert_eq!(params["args"][0]["value"], "hello");
            }
            _ => panic!("expected Event"),
        }
    }

    #[test]
    fn unique_request_ids() {
        let a = next_request_id();
        let b = next_request_id();
        assert_ne!(a, b);
    }
}
