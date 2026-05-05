//! CDP WebSocket transport.
//!
//! [`CdpTransport`] manages a single WebSocket connection to a Hermes debug
//! endpoint. It provides:
//!
//! - `send` -- serialize and send a [`CdpRequest`].
//! - `events` -- an async stream of incoming [`CdpMessage`]s (responses and
//!   events) via a tokio broadcast channel.
//! - `connect` -- establish the WebSocket and spawn the read loop.
//!
//! The transport is intentionally low-level. Higher-level concerns (matching
//! responses to requests, managing breakpoints, building a scope tree) live
//! in sibling modules or upstream crates.

use crate::cdp::{CdpMessage, CdpRequest, RawCdpMessage};
use futures_util::{SinkExt, StreamExt};
use tokio::sync::{broadcast, mpsc};
use tokio_tungstenite::tungstenite::Message;
use tracing::{debug, info, trace, warn};

/// Capacity of the broadcast channel for incoming CDP messages.
const EVENT_CHANNEL_CAPACITY: usize = 256;

/// A live CDP connection over WebSocket.
pub struct CdpTransport {
    /// Send requests to the write loop.
    tx: mpsc::Sender<String>,
    /// Subscribe to incoming messages (responses + events).
    events: broadcast::Sender<CdpMessage>,
}

/// Errors from the transport layer.
#[derive(Debug)]
pub enum TransportError {
    /// WebSocket connection failed.
    Connect(String),
    /// Failed to serialize or send a message.
    Send(String),
}

impl std::fmt::Display for TransportError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            TransportError::Connect(e) => write!(f, "CDP connect failed: {e}"),
            TransportError::Send(e) => write!(f, "CDP send failed: {e}"),
        }
    }
}

impl std::error::Error for TransportError {}

impl CdpTransport {
    /// Connect to a CDP WebSocket endpoint and spawn the read/write loops.
    ///
    /// `ws_url` is the `webSocketDebuggerUrl` from Metro's `/json/list`.
    /// Returns a transport handle; the connection runs on the provided
    /// tokio runtime until the WebSocket closes or the transport is dropped.
    pub async fn connect(ws_url: &str) -> Result<Self, TransportError> {
        info!(target: "atomio::cdp", url = %ws_url, "connecting WebSocket");
        let (ws_stream, _) = tokio_tungstenite::connect_async(ws_url)
            .await
            .map_err(|e| {
                warn!(target: "atomio::cdp", url = %ws_url, error = %e, "WebSocket connect failed");
                TransportError::Connect(e.to_string())
            })?;
        debug!(target: "atomio::cdp", url = %ws_url, "WebSocket open");

        let (mut ws_write, mut ws_read) = ws_stream.split();
        let (tx, mut rx) = mpsc::channel::<String>(64);
        let (events_tx, _) = broadcast::channel::<CdpMessage>(EVENT_CHANNEL_CAPACITY);
        let events_tx_clone = events_tx.clone();

        // Write loop: forward serialized requests to the WebSocket.
        tokio::spawn(async move {
            while let Some(msg) = rx.recv().await {
                if ws_write.send(Message::Text(msg.into())).await.is_err() {
                    break;
                }
            }
        });

        // Read loop: parse incoming frames and broadcast them.
        tokio::spawn(async move {
            let mut frame_count: u64 = 0;
            while let Some(frame_result) = ws_read.next().await {
                let frame = match frame_result {
                    Ok(f) => f,
                    Err(e) => {
                        warn!(target: "atomio::cdp", error = %e, "WebSocket read error");
                        break;
                    }
                };
                match frame {
                    Message::Text(text) => {
                        frame_count += 1;
                        let text_str: &str = &text;
                        trace!(target: "atomio::cdp::raw", bytes = text_str.len(), "<- frame");
                        match serde_json::from_str::<RawCdpMessage>(text_str) {
                            Ok(raw) => {
                                let preview = match (&raw.method, raw.id) {
                                    (Some(m), _) => format!("event {m}"),
                                    (None, Some(id)) => format!("response id={id}"),
                                    _ => "<unknown>".into(),
                                };
                                debug!(
                                    target: "atomio::cdp",
                                    n = frame_count,
                                    bytes = text_str.len(),
                                    kind = %preview,
                                    "<- frame"
                                );
                                if let Some(msg) = CdpMessage::from_raw(raw) {
                                    // Ignore send errors -- just means no subscribers yet.
                                    let _ = events_tx_clone.send(msg);
                                }
                            }
                            Err(e) => {
                                warn!(
                                    target: "atomio::cdp",
                                    error = %e,
                                    sample = %text_str.chars().take(120).collect::<String>(),
                                    "failed to parse CDP frame"
                                );
                            }
                        }
                    }
                    Message::Close(reason) => {
                        info!(target: "atomio::cdp", ?reason, "remote sent close frame");
                        break;
                    }
                    Message::Ping(_) | Message::Pong(_) => {}
                    other => {
                        debug!(target: "atomio::cdp", ?other, "ignoring non-text frame");
                    }
                }
            }
            info!(
                target: "atomio::cdp",
                frames_received = frame_count,
                "read loop exited (WebSocket closed)"
            );
        });

        Ok(Self {
            tx,
            events: events_tx,
        })
    }

    /// Send a CDP request. Returns the request ID for response matching.
    pub async fn send(&self, request: &CdpRequest) -> Result<u64, TransportError> {
        let json =
            serde_json::to_string(request).map_err(|e| TransportError::Send(e.to_string()))?;
        debug!(
            target: "atomio::cdp",
            id = request.id,
            method = %request.method,
            "-> send"
        );
        self.tx
            .send(json)
            .await
            .map_err(|e| TransportError::Send(e.to_string()))?;
        Ok(request.id)
    }

    /// Subscribe to incoming CDP messages. Each subscriber gets its own
    /// receiver that sees all messages from the point of subscription.
    pub fn subscribe(&self) -> broadcast::Receiver<CdpMessage> {
        self.events.subscribe()
    }
}

#[cfg(test)]
mod tests {
    // Transport tests require a real WebSocket server, which we don't spin
    // up in unit tests. Integration tests against a mock Metro/Hermes
    // endpoint will live in tests/ once we have one.
    //
    // For now, we verify that the module compiles and the types are wired
    // correctly. The CDP message parsing is thoroughly tested in cdp.rs.

    use super::*;

    #[test]
    fn transport_error_display() {
        let err = TransportError::Connect("connection refused".into());
        assert!(err.to_string().contains("connection refused"));
    }
}
