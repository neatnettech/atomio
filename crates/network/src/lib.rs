//! Network inspector model for atomio.
//!
//! Decodes CDP `Network` domain events into a typed [`NetworkRequest`]
//! lifecycle. Pure model -- no I/O, no async. The bridge layer feeds raw
//! event payloads in via `NetworkRegistry::on_*` handlers; the UI reads
//! [`NetworkRegistry::iter`] for rendering and [`NetworkRegistry::get`]
//! for the request detail view.
//!
//! ### CDP events handled
//!
//! - `Network.requestWillBeSent` -- creates entry in `Pending` state
//! - `Network.responseReceived` -- transitions to `Headers`
//! - `Network.loadingFinished` -- transitions to `Done`
//! - `Network.loadingFailed` -- transitions to `Failed`
//! - WebSocket frames (sent/received) collected separately on the entry
//!
//! Hermes / Metro emit a subset of these. We tolerate missing fields.

use serde_json::Value;
use std::collections::HashMap;

/// HTTP method, parsed from the request payload.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Method {
    Get,
    Post,
    Put,
    Patch,
    Delete,
    Head,
    Options,
    Other,
}

impl Method {
    pub fn parse(s: &str) -> Self {
        match s.to_ascii_uppercase().as_str() {
            "GET" => Method::Get,
            "POST" => Method::Post,
            "PUT" => Method::Put,
            "PATCH" => Method::Patch,
            "DELETE" => Method::Delete,
            "HEAD" => Method::Head,
            "OPTIONS" => Method::Options,
            _ => Method::Other,
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Method::Get => "GET",
            Method::Post => "POST",
            Method::Put => "PUT",
            Method::Patch => "PATCH",
            Method::Delete => "DEL",
            Method::Head => "HEAD",
            Method::Options => "OPT",
            Method::Other => "???",
        }
    }
}

/// Resource type bucket from CDP `Network.requestWillBeSent.type`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ResourceKind {
    Xhr,
    Fetch,
    Document,
    Stylesheet,
    Script,
    Image,
    Font,
    WebSocket,
    EventSource,
    Other,
}

impl ResourceKind {
    pub fn from_cdp(s: &str) -> Self {
        match s {
            "XHR" => Self::Xhr,
            "Fetch" => Self::Fetch,
            "Document" => Self::Document,
            "Stylesheet" => Self::Stylesheet,
            "Script" => Self::Script,
            "Image" => Self::Image,
            "Font" => Self::Font,
            "WebSocket" => Self::WebSocket,
            "EventSource" => Self::EventSource,
            _ => Self::Other,
        }
    }
}

/// Lifecycle state.
#[derive(Debug, Clone, PartialEq)]
pub enum RequestState {
    /// `requestWillBeSent` seen; no response yet.
    Pending,
    /// `responseReceived` seen; downloading or done.
    Headers { status: u16, mime: String },
    /// `loadingFinished` seen.
    Done {
        status: u16,
        mime: String,
        encoded_bytes: u64,
        duration_ms: f64,
    },
    /// `loadingFailed` seen.
    Failed { reason: String },
}

/// One captured request with its lifecycle state and metadata.
#[derive(Debug, Clone)]
pub struct NetworkRequest {
    /// CDP `requestId`.
    pub id: String,
    pub method: Method,
    pub url: String,
    pub kind: ResourceKind,
    pub state: RequestState,
    /// CDP timestamp (seconds, monotonic) when request was sent.
    pub start_ts: f64,
    /// Request headers seen so far.
    pub req_headers: Vec<(String, String)>,
    /// Response headers, populated on `responseReceived`.
    pub resp_headers: Vec<(String, String)>,
    /// Optional WebSocket frames captured for upgraded requests.
    pub ws_frames: Vec<WsFrame>,
}

/// One WebSocket frame (sent or received).
#[derive(Debug, Clone)]
pub struct WsFrame {
    pub direction: WsDirection,
    pub opcode: u8,
    pub payload: String,
    pub ts: f64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WsDirection {
    Sent,
    Received,
}

/// In-memory registry of captured requests, ordered by arrival.
#[derive(Debug, Default, Clone)]
pub struct NetworkRegistry {
    by_id: HashMap<String, usize>,
    items: Vec<NetworkRequest>,
    capacity: usize,
}

impl NetworkRegistry {
    pub fn new() -> Self {
        Self::with_capacity(2000)
    }

    pub fn with_capacity(capacity: usize) -> Self {
        Self {
            by_id: HashMap::new(),
            items: Vec::new(),
            capacity: capacity.max(1),
        }
    }

    /// Apply a `Network.requestWillBeSent` payload.
    pub fn on_request_will_be_sent(&mut self, params: &Value) -> Option<&NetworkRequest> {
        let id = params.get("requestId")?.as_str()?.to_string();
        let req = params.get("request")?;
        let url = req.get("url")?.as_str()?.to_string();
        let method = Method::parse(req.get("method")?.as_str()?);
        let kind = params
            .get("type")
            .and_then(|v| v.as_str())
            .map(ResourceKind::from_cdp)
            .unwrap_or(ResourceKind::Other);
        let start_ts = params
            .get("timestamp")
            .and_then(|v| v.as_f64())
            .unwrap_or(0.0);
        let req_headers = parse_headers(req.get("headers"));

        let entry = NetworkRequest {
            id: id.clone(),
            method,
            url,
            kind,
            state: RequestState::Pending,
            start_ts,
            req_headers,
            resp_headers: Vec::new(),
            ws_frames: Vec::new(),
        };
        self.insert(entry);
        self.get(&id)
    }

    /// Apply a `Network.responseReceived` payload.
    pub fn on_response_received(&mut self, params: &Value) -> Option<&NetworkRequest> {
        let id = params.get("requestId")?.as_str()?;
        let resp = params.get("response")?;
        let status = resp.get("status").and_then(|v| v.as_u64()).unwrap_or(0) as u16;
        let mime = resp
            .get("mimeType")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string();
        let resp_headers = parse_headers(resp.get("headers"));
        let item = self.get_mut(id)?;
        item.resp_headers = resp_headers;
        item.state = RequestState::Headers {
            status,
            mime: mime.clone(),
        };
        // also update kind if responseReceived knows better
        if let Some(t) = params.get("type").and_then(|v| v.as_str()) {
            item.kind = ResourceKind::from_cdp(t);
        }
        self.get(id)
    }

    /// Apply a `Network.loadingFinished` payload.
    pub fn on_loading_finished(&mut self, params: &Value) -> Option<&NetworkRequest> {
        let id = params.get("requestId")?.as_str()?;
        let end_ts = params
            .get("timestamp")
            .and_then(|v| v.as_f64())
            .unwrap_or(0.0);
        let encoded = params
            .get("encodedDataLength")
            .and_then(|v| v.as_f64())
            .unwrap_or(0.0) as u64;
        let item = self.get_mut(id)?;
        let (status, mime) = match &item.state {
            RequestState::Headers { status, mime } => (*status, mime.clone()),
            _ => (0, String::new()),
        };
        let duration_ms = ((end_ts - item.start_ts) * 1000.0).max(0.0);
        item.state = RequestState::Done {
            status,
            mime,
            encoded_bytes: encoded,
            duration_ms,
        };
        self.get(id)
    }

    /// Apply a `Network.loadingFailed` payload.
    pub fn on_loading_failed(&mut self, params: &Value) -> Option<&NetworkRequest> {
        let id = params.get("requestId")?.as_str()?;
        let reason = params
            .get("errorText")
            .and_then(|v| v.as_str())
            .unwrap_or("loading failed")
            .to_string();
        let item = self.get_mut(id)?;
        item.state = RequestState::Failed { reason };
        self.get(id)
    }

    /// Apply a `Network.webSocketFrameSent` / `webSocketFrameReceived`.
    pub fn on_ws_frame(
        &mut self,
        direction: WsDirection,
        params: &Value,
    ) -> Option<&NetworkRequest> {
        let id = params.get("requestId")?.as_str()?;
        let ts = params
            .get("timestamp")
            .and_then(|v| v.as_f64())
            .unwrap_or(0.0);
        let frame = params.get("response")?;
        let opcode = frame.get("opcode").and_then(|v| v.as_u64()).unwrap_or(0) as u8;
        let payload = frame
            .get("payloadData")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string();
        let item = self.get_mut(id)?;
        item.ws_frames.push(WsFrame {
            direction,
            opcode,
            payload,
            ts,
        });
        self.get(id)
    }

    /// Drop everything. Used on disconnect.
    pub fn clear(&mut self) {
        self.by_id.clear();
        self.items.clear();
    }

    /// Iterate over all captured requests in arrival order.
    pub fn iter(&self) -> impl Iterator<Item = &NetworkRequest> {
        self.items.iter()
    }

    pub fn len(&self) -> usize {
        self.items.len()
    }

    pub fn is_empty(&self) -> bool {
        self.items.is_empty()
    }

    pub fn get(&self, id: &str) -> Option<&NetworkRequest> {
        self.by_id.get(id).and_then(|i| self.items.get(*i))
    }

    fn get_mut(&mut self, id: &str) -> Option<&mut NetworkRequest> {
        let i = *self.by_id.get(id)?;
        self.items.get_mut(i)
    }

    /// Insert or replace by id. Maintains capacity by FIFO eviction.
    fn insert(&mut self, entry: NetworkRequest) {
        if let Some(&i) = self.by_id.get(&entry.id) {
            self.items[i] = entry;
            return;
        }
        if self.items.len() >= self.capacity {
            // Evict oldest.
            let evicted = self.items.remove(0);
            self.by_id.remove(&evicted.id);
            // Reindex remaining.
            for (i, item) in self.items.iter().enumerate() {
                self.by_id.insert(item.id.clone(), i);
            }
        }
        let idx = self.items.len();
        self.by_id.insert(entry.id.clone(), idx);
        self.items.push(entry);
    }
}

fn parse_headers(value: Option<&Value>) -> Vec<(String, String)> {
    let Some(obj) = value.and_then(|v| v.as_object()) else {
        return Vec::new();
    };
    obj.iter()
        .filter_map(|(k, v)| v.as_str().map(|s| (k.clone(), s.to_string())))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn method_parsing() {
        assert_eq!(Method::parse("get"), Method::Get);
        assert_eq!(Method::parse("POST"), Method::Post);
        assert_eq!(Method::parse("WAT"), Method::Other);
    }

    #[test]
    fn lifecycle_pending_to_done() {
        let mut reg = NetworkRegistry::new();
        let will = serde_json::json!({
            "requestId": "1",
            "request": {"url": "https://api/x", "method": "GET", "headers": {"Accept": "*/*"}},
            "type": "Fetch",
            "timestamp": 100.0,
        });
        reg.on_request_will_be_sent(&will).unwrap();
        let resp = serde_json::json!({
            "requestId": "1",
            "response": {"status": 200, "mimeType": "application/json", "headers": {"X": "y"}},
            "type": "Fetch",
        });
        reg.on_response_received(&resp).unwrap();
        let fin = serde_json::json!({
            "requestId": "1",
            "encodedDataLength": 123,
            "timestamp": 100.5,
        });
        let item = reg.on_loading_finished(&fin).unwrap();
        match &item.state {
            RequestState::Done {
                status,
                encoded_bytes,
                duration_ms,
                ..
            } => {
                assert_eq!(*status, 200);
                assert_eq!(*encoded_bytes, 123);
                assert!((duration_ms - 500.0).abs() < 1.0);
            }
            other => panic!("expected Done, got {:?}", other),
        }
        assert_eq!(item.method, Method::Get);
        assert_eq!(item.kind, ResourceKind::Fetch);
        assert_eq!(item.req_headers.len(), 1);
        assert_eq!(item.resp_headers.len(), 1);
    }

    #[test]
    fn loading_failed_transitions() {
        let mut reg = NetworkRegistry::new();
        reg.on_request_will_be_sent(&serde_json::json!({
            "requestId": "x",
            "request": {"url": "u", "method": "GET"},
            "timestamp": 1.0,
        }))
        .unwrap();
        reg.on_loading_failed(&serde_json::json!({
            "requestId": "x",
            "errorText": "DNS",
        }))
        .unwrap();
        match &reg.get("x").unwrap().state {
            RequestState::Failed { reason } => assert_eq!(reason, "DNS"),
            _ => panic!("expected Failed"),
        }
    }

    #[test]
    fn ws_frames_collected() {
        let mut reg = NetworkRegistry::new();
        reg.on_request_will_be_sent(&serde_json::json!({
            "requestId": "ws",
            "request": {"url": "ws://x", "method": "GET"},
            "type": "WebSocket",
            "timestamp": 0.0,
        }))
        .unwrap();
        reg.on_ws_frame(
            WsDirection::Sent,
            &serde_json::json!({
                "requestId": "ws",
                "timestamp": 1.0,
                "response": {"opcode": 1, "payloadData": "ping"},
            }),
        )
        .unwrap();
        reg.on_ws_frame(
            WsDirection::Received,
            &serde_json::json!({
                "requestId": "ws",
                "timestamp": 1.1,
                "response": {"opcode": 1, "payloadData": "pong"},
            }),
        )
        .unwrap();
        let item = reg.get("ws").unwrap();
        assert_eq!(item.ws_frames.len(), 2);
        assert_eq!(item.ws_frames[0].direction, WsDirection::Sent);
        assert_eq!(item.ws_frames[1].payload, "pong");
    }

    #[test]
    fn capacity_evicts_oldest() {
        let mut reg = NetworkRegistry::with_capacity(2);
        for i in 0..3 {
            reg.on_request_will_be_sent(&serde_json::json!({
                "requestId": format!("{i}"),
                "request": {"url": "u", "method": "GET"},
                "timestamp": 0.0,
            }))
            .unwrap();
        }
        assert_eq!(reg.len(), 2);
        assert!(reg.get("0").is_none());
        assert!(reg.get("1").is_some());
        assert!(reg.get("2").is_some());
    }

    #[test]
    fn missing_required_fields_returns_none() {
        let mut reg = NetworkRegistry::new();
        let v = serde_json::json!({"request": {}});
        assert!(reg.on_request_will_be_sent(&v).is_none());
    }

    #[test]
    fn clear_drops_everything() {
        let mut reg = NetworkRegistry::new();
        reg.on_request_will_be_sent(&serde_json::json!({
            "requestId": "1",
            "request": {"url": "u", "method": "GET"},
            "timestamp": 0.0,
        }));
        reg.clear();
        assert!(reg.is_empty());
    }
}
