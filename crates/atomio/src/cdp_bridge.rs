//! Bridge between the gpui UI thread and the async CDP debugger.
//!
//! gpui has its own executor; tokio-driven WebSocket I/O can't run on it
//! directly. We solve that by spawning a dedicated OS thread that owns a
//! `current_thread` tokio runtime. The UI sends commands to that thread
//! over a `std::sync::mpsc` channel; the worker pushes events back over
//! another `std::sync::mpsc`. The UI polls the receivers periodically via
//! a gpui timer.
//!
//! No async leaks into the UI module beyond the polling loop.

use std::collections::HashMap;
use std::sync::mpsc::{Receiver, Sender};
use std::sync::Arc;
use std::thread;

use console::{entry_from_console_api_called, LogLevel, SourceLocation};
use debugger::breakpoints::BreakpointId;
use debugger::cdp::{
    debugger_enable, debugger_pause, debugger_resume, debugger_step_into, debugger_step_out,
    debugger_step_over, get_properties, remove_breakpoint, runtime_enable, set_breakpoint_by_url,
    CdpMessage,
};
use debugger::metro::{fetch_source, scan_localhost};
use debugger::scripts::Script;
use debugger::transport::CdpTransport;
use inspector::{CallStack, Property};
use serde_json::Value;
use tokio::sync::Mutex;
use tracing::{debug, info, warn};

/// Commands the UI sends to the worker thread.
///
/// `SetBreakpoint` / `RemoveBreakpoint` aren't dispatched from the UI yet
/// (gutter UI ships in a follow-up PR) but the wire is in place so the
/// follow-up is purely UI-side.
#[allow(dead_code)]
#[derive(Debug, Clone)]
pub enum DebuggerCommand {
    /// Scan localhost, pick the first target, connect, enable Runtime + Debugger.
    Connect,
    /// Tear down the active connection (best effort).
    Disconnect,
    /// Fetch a source resource by URL and emit it back as
    /// [`DebuggerEvent::SourceFetched`]. Used to load script bodies and
    /// `.map` files referenced by `Debugger.scriptParsed`.
    FetchSource {
        url: String,
    },
    /// Set a breakpoint by source URL and line. Worker sends
    /// `Debugger.setBreakpointByUrl`; on response the registry is updated
    /// via [`DebuggerEvent::BreakpointSet`] carrying the matching local id.
    SetBreakpoint {
        local: BreakpointId,
        url: String,
        line: u32,
        column: Option<u32>,
        condition: Option<String>,
    },
    /// Remove a breakpoint by its server-assigned id.
    RemoveBreakpoint {
        server_id: String,
    },
    /// Fetch properties of a CDP `RemoteObject`. `tag` is an opaque token
    /// the UI uses to match the response to the right scope/property
    /// expansion (returned in [`DebuggerEvent::Properties`]).
    GetProperties {
        tag: u64,
        object_id: String,
        own_properties: bool,
    },
    /// Step controls -- map directly to CDP `Debugger.*` methods.
    Resume,
    StepOver,
    StepInto,
    StepOut,
    Pause,
}

/// Events the worker thread pushes back to the UI.
#[derive(Debug, Clone)]
pub enum DebuggerEvent {
    /// Connection state changed.
    State(ConnectionState),
    /// A console log entry was received.
    Log {
        level: LogLevel,
        message: String,
        location: Option<SourceLocation>,
    },
    /// A `Debugger.scriptParsed` event was parsed into a [`Script`].
    ScriptParsed(Script),
    /// Result of a [`DebuggerCommand::FetchSource`] request.
    SourceFetched {
        url: String,
        body: Result<String, String>,
    },
    /// `Debugger.setBreakpointByUrl` response received. Carries the local
    /// id we issued the request with so the UI can attach the server id.
    BreakpointSet {
        local: BreakpointId,
        server_id: String,
        line: Option<u32>,
    },
    /// `Debugger.breakpointResolved` event -- runtime relocated breakpoint.
    BreakpointResolved { server_id: String, line: u32 },
    /// `Debugger.paused` -- runtime hit a breakpoint or stepped. The raw
    /// call-frame array is decoded into a typed [`CallStack`].
    Paused {
        reason: String,
        call_stack: CallStack,
    },
    /// `Debugger.resumed` -- execution continued.
    Resumed,
    /// Result of [`DebuggerCommand::GetProperties`].
    Properties { tag: u64, properties: Vec<Property> },
}

/// Coarse connection state. Detailed CDP state lives in the worker.
#[derive(Debug, Clone)]
pub enum ConnectionState {
    Disconnected,
    Scanning,
    Connecting { ws_url: String },
    Connected { ws_url: String },
    Failed { reason: String },
}

/// Handle to a spawned debugger worker thread.
pub struct DebuggerBridge {
    pub commands: Sender<DebuggerCommand>,
    pub events: Receiver<DebuggerEvent>,
}

/// Spawn the worker thread and return a [`DebuggerBridge`] for the UI to use.
pub fn spawn() -> DebuggerBridge {
    let (cmd_tx, cmd_rx) = std::sync::mpsc::channel::<DebuggerCommand>();
    let (evt_tx, evt_rx) = std::sync::mpsc::channel::<DebuggerEvent>();

    thread::Builder::new()
        .name("atomio-cdp".into())
        .spawn(move || {
            let runtime = match tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()
            {
                Ok(rt) => rt,
                Err(e) => {
                    let _ = evt_tx.send(DebuggerEvent::State(ConnectionState::Failed {
                        reason: format!("tokio runtime init failed: {e}"),
                    }));
                    return;
                }
            };
            runtime.block_on(worker_loop(cmd_rx, evt_tx));
        })
        .expect("failed to spawn atomio-cdp thread");

    DebuggerBridge {
        commands: cmd_tx,
        events: evt_rx,
    }
}

/// Pending CDP request id → local breakpoint id awaiting response.
/// Populated when we send `Debugger.setBreakpointByUrl`; drained when the
/// matching response arrives in the event forwarder.
type PendingBreakpoints = Arc<Mutex<HashMap<u64, BreakpointId>>>;
/// CDP request id → caller-supplied tag awaiting a `Runtime.getProperties`
/// response.
type PendingProperties = Arc<Mutex<HashMap<u64, u64>>>;

/// The worker's async main loop. One outstanding CDP connection at a time.
async fn worker_loop(cmd_rx: Receiver<DebuggerCommand>, evt_tx: Sender<DebuggerEvent>) {
    // Transport held in Arc so command handlers and the event forwarder can
    // both reach it. `None` until Connect succeeds; cleared on Disconnect.
    let transport: Arc<Mutex<Option<Arc<CdpTransport>>>> = Arc::new(Mutex::new(None));
    let pending: PendingBreakpoints = Arc::new(Mutex::new(HashMap::new()));
    let pending_props: PendingProperties = Arc::new(Mutex::new(HashMap::new()));

    loop {
        // Block on a command from the UI. Channel close = shutdown.
        let cmd = match cmd_rx.recv() {
            Ok(c) => c,
            Err(_) => return,
        };

        match cmd {
            DebuggerCommand::Disconnect => {
                info!(target: "atomio::bridge", "disconnect requested");
                *transport.lock().await = None;
                pending.lock().await.clear();
                let _ = evt_tx.send(DebuggerEvent::State(ConnectionState::Disconnected));
            }
            DebuggerCommand::FetchSource { url } => {
                debug!(target: "atomio::bridge", url = %url, "fetch source");
                let evt_tx = evt_tx.clone();
                tokio::spawn(async move {
                    let body = fetch_source(&url).await;
                    let _ = evt_tx.send(DebuggerEvent::SourceFetched { url, body });
                });
            }
            DebuggerCommand::Connect => {
                connect(&transport, &pending, &pending_props, &evt_tx).await;
            }
            DebuggerCommand::SetBreakpoint {
                local,
                url,
                line,
                column,
                condition: _condition,
            } => {
                let Some(t) = transport.lock().await.clone() else {
                    warn!(target: "atomio::bridge", "set breakpoint while disconnected");
                    continue;
                };
                let req = set_breakpoint_by_url(&url, line, column);
                let req_id = req.id;
                debug!(
                    target: "atomio::bridge",
                    local = ?local,
                    req_id,
                    url = %url,
                    line,
                    "-> setBreakpointByUrl"
                );
                pending.lock().await.insert(req_id, local);
                if let Err(e) = t.send(&req).await {
                    warn!(target: "atomio::bridge", error = %e, "send setBreakpointByUrl failed");
                    pending.lock().await.remove(&req_id);
                }
            }
            DebuggerCommand::RemoveBreakpoint { server_id } => {
                let Some(t) = transport.lock().await.clone() else {
                    continue;
                };
                debug!(target: "atomio::bridge", server_id = %server_id, "-> removeBreakpoint");
                let _ = t.send(&remove_breakpoint(&server_id)).await;
            }
            DebuggerCommand::GetProperties {
                tag,
                object_id,
                own_properties,
            } => {
                let Some(t) = transport.lock().await.clone() else {
                    continue;
                };
                let req = get_properties(&object_id, own_properties);
                let req_id = req.id;
                debug!(
                    target: "atomio::bridge",
                    tag,
                    req_id,
                    object_id = %object_id,
                    "-> getProperties"
                );
                pending_props.lock().await.insert(req_id, tag);
                if let Err(e) = t.send(&req).await {
                    warn!(target: "atomio::bridge", error = %e, "send getProperties failed");
                    pending_props.lock().await.remove(&req_id);
                }
            }
            DebuggerCommand::Resume => send_no_args(&transport, debugger_resume(), "resume").await,
            DebuggerCommand::StepOver => {
                send_no_args(&transport, debugger_step_over(), "stepOver").await
            }
            DebuggerCommand::StepInto => {
                send_no_args(&transport, debugger_step_into(), "stepInto").await
            }
            DebuggerCommand::StepOut => {
                send_no_args(&transport, debugger_step_out(), "stepOut").await
            }
            DebuggerCommand::Pause => send_no_args(&transport, debugger_pause(), "pause").await,
        }
    }
}

/// Helper for fire-and-forget step/pause/resume requests.
async fn send_no_args(
    transport: &Arc<Mutex<Option<Arc<CdpTransport>>>>,
    request: debugger::cdp::CdpRequest,
    label: &str,
) {
    let Some(t) = transport.lock().await.clone() else {
        warn!(target: "atomio::bridge", label, "send while disconnected");
        return;
    };
    debug!(target: "atomio::bridge", label, "-> step/control");
    let _ = t.send(&request).await;
}

/// Establish a CDP connection and spawn the event forwarder.
async fn connect(
    transport_slot: &Arc<Mutex<Option<Arc<CdpTransport>>>>,
    pending: &PendingBreakpoints,
    pending_props: &PendingProperties,
    evt_tx: &Sender<DebuggerEvent>,
) {
    info!(target: "atomio::bridge", "connect requested");
    let _ = evt_tx.send(DebuggerEvent::State(ConnectionState::Scanning));
    let targets = scan_localhost().await;
    if targets.is_empty() {
        let reason = "no Metro targets found on localhost (is `expo start` running with a sim/device booted?)".to_string();
        warn!(target: "atomio::bridge", "{}", reason);
        let _ = evt_tx.send(DebuggerEvent::State(ConnectionState::Failed { reason }));
        return;
    }
    for (base, t) in &targets {
        info!(
            target: "atomio::bridge",
            base = %base,
            id = %t.id,
            title = %t.title,
            desc = %t.description,
            "candidate target"
        );
    }
    let pick = targets
        .iter()
        .find(|(_, t)| !t.title.starts_with("host.exp.Exponent") && !t.description.contains("UI ["))
        .or_else(|| targets.first())
        .cloned();
    let Some((_, target)) = pick else {
        let reason = "target list non-empty but selection failed".to_string();
        warn!(target: "atomio::bridge", "{}", reason);
        let _ = evt_tx.send(DebuggerEvent::State(ConnectionState::Failed { reason }));
        return;
    };
    let ws_url = target.web_socket_debugger_url.clone();
    info!(
        target: "atomio::bridge",
        target_id = %target.id,
        title = %target.title,
        ws_url = %ws_url,
        "picked target"
    );
    let _ = evt_tx.send(DebuggerEvent::State(ConnectionState::Connecting {
        ws_url: ws_url.clone(),
    }));

    let transport = match CdpTransport::connect(&ws_url).await {
        Ok(t) => Arc::new(t),
        Err(e) => {
            warn!(target: "atomio::bridge", error = %e, "transport connect failed");
            let _ = evt_tx.send(DebuggerEvent::State(ConnectionState::Failed {
                reason: e.to_string(),
            }));
            return;
        }
    };

    let _ = transport.send(&runtime_enable()).await;
    let _ = transport.send(&debugger_enable()).await;

    let _ = evt_tx.send(DebuggerEvent::State(ConnectionState::Connected {
        ws_url: ws_url.clone(),
    }));

    let mut events = transport.subscribe();
    let evt_tx = evt_tx.clone();
    let pending = pending.clone();
    let pending_props = pending_props.clone();
    tokio::spawn(async move {
        while let Ok(msg) = events.recv().await {
            match msg {
                CdpMessage::Event { method, params } => match method.as_str() {
                    "Runtime.consoleAPICalled" => {
                        if let Some((level, message, location)) =
                            entry_from_console_api_called(&params)
                        {
                            let _ = evt_tx.send(DebuggerEvent::Log {
                                level,
                                message,
                                location,
                            });
                        }
                    }
                    "Debugger.scriptParsed" => {
                        if let Some(script) = Script::from_script_parsed(&params) {
                            let _ = evt_tx.send(DebuggerEvent::ScriptParsed(script));
                        }
                    }
                    "Debugger.breakpointResolved" => {
                        if let Some((server_id, line)) = parse_breakpoint_resolved(&params) {
                            let _ =
                                evt_tx.send(DebuggerEvent::BreakpointResolved { server_id, line });
                        }
                    }
                    "Debugger.paused" => {
                        let reason = params
                            .get("reason")
                            .and_then(|v| v.as_str())
                            .unwrap_or("other")
                            .to_string();
                        let frames = params
                            .get("callFrames")
                            .cloned()
                            .unwrap_or(Value::Array(vec![]));
                        let call_stack = CallStack::from_cdp(&frames);
                        let _ = evt_tx.send(DebuggerEvent::Paused { reason, call_stack });
                    }
                    "Debugger.resumed" => {
                        let _ = evt_tx.send(DebuggerEvent::Resumed);
                    }
                    _ => {}
                },
                CdpMessage::Response { id, result } => {
                    // Match against pending breakpoint requests.
                    let local_bp = pending.lock().await.remove(&id);
                    if let Some(local) = local_bp {
                        match result {
                            Ok(value) => {
                                if let Some((server_id, line)) =
                                    parse_set_breakpoint_response(&value)
                                {
                                    let _ = evt_tx.send(DebuggerEvent::BreakpointSet {
                                        local,
                                        server_id,
                                        line,
                                    });
                                }
                            }
                            Err(e) => {
                                warn!(
                                    target: "atomio::bridge",
                                    code = e.code,
                                    msg = %e.message,
                                    "setBreakpointByUrl error"
                                );
                            }
                        }
                        continue;
                    }
                    // Match against pending getProperties requests.
                    let prop_tag = pending_props.lock().await.remove(&id);
                    if let Some(tag) = prop_tag {
                        match result {
                            Ok(value) => {
                                let properties = Property::parse_response(&value);
                                let _ = evt_tx.send(DebuggerEvent::Properties { tag, properties });
                            }
                            Err(e) => {
                                warn!(
                                    target: "atomio::bridge",
                                    code = e.code,
                                    msg = %e.message,
                                    "getProperties error"
                                );
                            }
                        }
                    }
                }
            }
        }
    });

    *transport_slot.lock().await = Some(transport);
}

/// Pull `breakpointId` and the first resolved line out of a
/// `Debugger.setBreakpointByUrl` success response.
fn parse_set_breakpoint_response(value: &Value) -> Option<(String, Option<u32>)> {
    let server_id = value.get("breakpointId")?.as_str()?.to_string();
    let line = value
        .get("locations")
        .and_then(|l| l.as_array())
        .and_then(|arr| arr.first())
        .and_then(|loc| loc.get("lineNumber"))
        .and_then(|v| v.as_u64())
        .map(|n| n as u32);
    Some((server_id, line))
}

/// Extract `breakpointId` + resolved line from a `Debugger.breakpointResolved` event.
fn parse_breakpoint_resolved(params: &Value) -> Option<(String, u32)> {
    let server_id = params.get("breakpointId")?.as_str()?.to_string();
    let line = params
        .get("location")
        .and_then(|loc| loc.get("lineNumber"))
        .and_then(|v| v.as_u64())? as u32;
    Some((server_id, line))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_set_breakpoint_with_locations() {
        let v = serde_json::json!({
            "breakpointId": "srv-1",
            "locations": [{"scriptId": "42", "lineNumber": 17, "columnNumber": 0}]
        });
        let (id, line) = parse_set_breakpoint_response(&v).unwrap();
        assert_eq!(id, "srv-1");
        assert_eq!(line, Some(17));
    }

    #[test]
    fn parse_set_breakpoint_no_locations() {
        let v = serde_json::json!({"breakpointId": "srv-1", "locations": []});
        let (id, line) = parse_set_breakpoint_response(&v).unwrap();
        assert_eq!(id, "srv-1");
        assert!(line.is_none());
    }

    #[test]
    fn parse_breakpoint_resolved_event() {
        let p = serde_json::json!({
            "breakpointId": "srv-2",
            "location": {"scriptId": "1", "lineNumber": 99, "columnNumber": 4}
        });
        let (id, line) = parse_breakpoint_resolved(&p).unwrap();
        assert_eq!(id, "srv-2");
        assert_eq!(line, 99);
    }

    #[test]
    fn parse_breakpoint_resolved_missing_line() {
        let p = serde_json::json!({"breakpointId": "x"});
        assert!(parse_breakpoint_resolved(&p).is_none());
    }
}
