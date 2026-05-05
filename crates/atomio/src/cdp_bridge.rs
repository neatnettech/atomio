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

use std::sync::mpsc::{Receiver, Sender};
use std::thread;

use console::{entry_from_console_api_called, LogLevel, SourceLocation};
use debugger::cdp::{debugger_enable, runtime_enable, CdpMessage};
use debugger::metro::scan_localhost;
use debugger::transport::CdpTransport;

/// Commands the UI sends to the worker thread.
#[derive(Debug, Clone)]
pub enum DebuggerCommand {
    /// Scan localhost, pick the first target, connect, enable Runtime + Debugger.
    Connect,
    /// Tear down the active connection (best effort).
    Disconnect,
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

/// The worker's async main loop. One outstanding CDP connection at a time.
#[allow(unused_assignments, unused_variables)]
async fn worker_loop(cmd_rx: Receiver<DebuggerCommand>, evt_tx: Sender<DebuggerEvent>) {
    // The transport is held here purely to keep the WebSocket open; reading
    // it isn't necessary because messages flow through the broadcast
    // channel that the read loop owns. We assign and drop it deliberately.
    let mut current: Option<CdpTransport> = None;

    loop {
        // Block on a command. Commands come from the UI thread over a
        // sync channel; treat closure as shutdown.
        let cmd = match cmd_rx.recv() {
            Ok(c) => c,
            Err(_) => return,
        };

        match cmd {
            DebuggerCommand::Disconnect => {
                current = None;
                let _ = evt_tx.send(DebuggerEvent::State(ConnectionState::Disconnected));
            }
            DebuggerCommand::Connect => {
                let _ = evt_tx.send(DebuggerEvent::State(ConnectionState::Scanning));
                let targets = scan_localhost().await;
                let Some((_, target)) = targets.into_iter().next() else {
                    let _ = evt_tx.send(DebuggerEvent::State(ConnectionState::Failed {
                        reason: "no Metro targets found on localhost".into(),
                    }));
                    continue;
                };
                let ws_url = target.web_socket_debugger_url.clone();
                let _ = evt_tx.send(DebuggerEvent::State(ConnectionState::Connecting {
                    ws_url: ws_url.clone(),
                }));

                let transport = match CdpTransport::connect(&ws_url).await {
                    Ok(t) => t,
                    Err(e) => {
                        let _ = evt_tx.send(DebuggerEvent::State(ConnectionState::Failed {
                            reason: e.to_string(),
                        }));
                        continue;
                    }
                };

                // Enable the domains we care about. Errors are non-fatal:
                // the transport may still receive events even if one
                // request fails.
                let _ = transport.send(&runtime_enable()).await;
                let _ = transport.send(&debugger_enable()).await;

                let _ = evt_tx.send(DebuggerEvent::State(ConnectionState::Connected {
                    ws_url: ws_url.clone(),
                }));

                // Subscribe and forward log events. We spawn this as a
                // background task so the worker can keep polling commands.
                let mut events = transport.subscribe();
                let evt_tx = evt_tx.clone();
                tokio::spawn(async move {
                    while let Ok(msg) = events.recv().await {
                        if let CdpMessage::Event { method, params } = msg {
                            if method == "Runtime.consoleAPICalled" {
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
                        }
                    }
                });

                current = Some(transport);
            }
        }
    }
}
