//! Integration tests for [`debugger::transport::CdpTransport`] against a
//! mock CDP WebSocket server.
//!
//! Spins up a real WebSocket listener on a kernel-assigned port, has the
//! transport connect to it, and exercises request/response routing and
//! event delivery end-to-end. No actual Hermes / Metro process is
//! required, but the wire format and the async wiring are real.

use std::net::SocketAddr;
use std::time::Duration;

use debugger::cdp::{debugger_enable, runtime_enable, CdpMessage};
use debugger::transport::CdpTransport;
use futures_util::{SinkExt, StreamExt};
use serde_json::{json, Value};
use tokio::net::TcpListener;
use tokio::time::timeout;
use tokio_tungstenite::tungstenite::Message;

/// Start a mock CDP WebSocket server on an ephemeral port. The handler is
/// invoked once per inbound connection with read/write halves of the
/// upgraded stream. Returns the bound `ws://` URL plus the join handle.
async fn spawn_mock_server<F>(handler: F) -> (String, tokio::task::JoinHandle<()>)
where
    F: FnOnce(
            futures_util::stream::SplitSink<
                tokio_tungstenite::WebSocketStream<tokio::net::TcpStream>,
                Message,
            >,
            futures_util::stream::SplitStream<
                tokio_tungstenite::WebSocketStream<tokio::net::TcpStream>,
            >,
        ) -> std::pin::Pin<Box<dyn std::future::Future<Output = ()> + Send>>
        + Send
        + 'static,
{
    let listener = TcpListener::bind("127.0.0.1:0").await.expect("bind");
    let addr: SocketAddr = listener.local_addr().expect("local_addr");
    let url = format!("ws://{addr}/");

    let join = tokio::spawn(async move {
        let (stream, _peer) = listener.accept().await.expect("accept");
        let ws = tokio_tungstenite::accept_async(stream)
            .await
            .expect("ws accept");
        let (write, read) = ws.split();
        handler(write, read).await;
    });

    (url, join)
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn connect_succeeds_against_mock_server() {
    let (url, _join) = spawn_mock_server(|_write, mut read| {
        Box::pin(async move {
            // Hold the socket open until the test side closes.
            while read.next().await.is_some() {}
        })
    })
    .await;

    let transport = timeout(Duration::from_secs(5), CdpTransport::connect(&url))
        .await
        .expect("connect did not time out")
        .expect("transport connect");
    drop(transport);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn request_response_round_trip() {
    // Server echoes back `{"id": <id>, "result": {"ack": "<method>"}}` for
    // every text frame it receives. Verifies the transport's send +
    // subscribe pipeline at the wire level.
    let (url, _join) = spawn_mock_server(|mut write, mut read| {
        Box::pin(async move {
            while let Some(Ok(msg)) = read.next().await {
                if let Message::Text(text) = msg {
                    let req: Value = serde_json::from_str(&text).expect("valid json");
                    let id = req["id"].as_u64().expect("id");
                    let method = req["method"].as_str().unwrap_or("").to_string();
                    let resp = json!({
                        "id": id,
                        "result": {"ack": method},
                    });
                    write
                        .send(Message::Text(resp.to_string().into()))
                        .await
                        .expect("send response");
                }
            }
        })
    })
    .await;

    let transport = CdpTransport::connect(&url).await.expect("connect");
    let mut events = transport.subscribe();

    let req = runtime_enable();
    let expected_id = req.id;
    transport.send(&req).await.expect("send");

    let msg = timeout(Duration::from_secs(5), events.recv())
        .await
        .expect("response in time")
        .expect("broadcast ok");

    match msg {
        CdpMessage::Response { id, result } => {
            assert_eq!(id, expected_id);
            let value = result.expect("ok result");
            assert_eq!(value["ack"], "Runtime.enable");
        }
        other => panic!("expected Response, got {other:?}"),
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn unsolicited_event_delivered_to_subscriber() {
    // Server pushes a `Debugger.paused` event immediately on connect.
    // The transport must forward it through the broadcast channel
    // without any client-side request having been sent first.
    let (url, _join) = spawn_mock_server(|mut write, mut read| {
        Box::pin(async move {
            let event = json!({
                "method": "Debugger.paused",
                "params": {
                    "reason": "other",
                    "callFrames": []
                }
            });
            write
                .send(Message::Text(event.to_string().into()))
                .await
                .expect("push event");
            while read.next().await.is_some() {}
        })
    })
    .await;

    let transport = CdpTransport::connect(&url).await.expect("connect");
    let mut events = transport.subscribe();

    let msg = timeout(Duration::from_secs(5), events.recv())
        .await
        .expect("event in time")
        .expect("broadcast ok");

    match msg {
        CdpMessage::Event { method, params } => {
            assert_eq!(method, "Debugger.paused");
            assert_eq!(params["reason"], "other");
        }
        other => panic!("expected Event, got {other:?}"),
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn error_response_surfaces_as_err_result() {
    // Server replies with a CDP-style error envelope. The transport
    // must decode it into `Response { result: Err(_) }`.
    let (url, _join) = spawn_mock_server(|mut write, mut read| {
        Box::pin(async move {
            while let Some(Ok(msg)) = read.next().await {
                if let Message::Text(text) = msg {
                    let req: Value = serde_json::from_str(&text).expect("valid json");
                    let id = req["id"].as_u64().expect("id");
                    let resp = json!({
                        "id": id,
                        "error": {
                            "code": -32000,
                            "message": "Object reference no longer exists"
                        }
                    });
                    write
                        .send(Message::Text(resp.to_string().into()))
                        .await
                        .expect("send error");
                }
            }
        })
    })
    .await;

    let transport = CdpTransport::connect(&url).await.expect("connect");
    let mut events = transport.subscribe();

    let req = debugger_enable();
    let expected_id = req.id;
    transport.send(&req).await.expect("send");

    let msg = timeout(Duration::from_secs(5), events.recv())
        .await
        .expect("response in time")
        .expect("broadcast ok");

    match msg {
        CdpMessage::Response { id, result } => {
            assert_eq!(id, expected_id);
            let err = result.expect_err("expected Err");
            assert_eq!(err.code, -32000);
            assert!(err.message.contains("no longer exists"));
        }
        other => panic!("expected Response, got {other:?}"),
    }
}
