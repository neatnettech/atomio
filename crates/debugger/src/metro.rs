//! Metro bundler discovery.
//!
//! When Expo starts a development server, it exposes a Chrome DevTools
//! Protocol compatible endpoint. The standard way to find debuggable
//! targets is to GET `/json/list` (or `/json`) on the Metro port.
//!
//! This module provides:
//!
//! - [`DebugTarget`] -- a parsed entry from the `/json/list` response.
//! - [`discover_targets`] -- fetch and parse targets from a Metro URL.
//! - [`scan_localhost`] -- probe common Expo ports (8081, 19000-19006, 8082).

use serde::Deserialize;
use std::time::Duration;

/// A debuggable target advertised by Metro / Hermes.
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DebugTarget {
    /// Unique target identifier.
    pub id: String,
    /// Human-readable title (e.g. "Hermes React Native").
    pub title: String,
    /// Target type (usually "page" or "node").
    #[serde(rename = "type")]
    pub target_type: String,
    /// DevTools frontend URL (browser-based).
    #[serde(default)]
    pub devtools_frontend_url: String,
    /// WebSocket URL for CDP connection. This is what we connect to.
    pub web_socket_debugger_url: String,
    /// Optional description.
    #[serde(default)]
    pub description: String,
}

/// Fetch debuggable targets from a Metro bundler at `base_url`.
///
/// `base_url` should be something like `http://localhost:8081`. The function
/// appends `/json/list` and parses the JSON array response.
///
/// Returns an empty vec on network error or non-JSON response -- callers
/// should treat that as "no targets found at this URL" rather than a hard
/// failure.
pub async fn discover_targets(base_url: &str) -> Vec<DebugTarget> {
    let url = format!("{}/json/list", base_url.trim_end_matches('/'));
    let Ok(response) = tokio::time::timeout(Duration::from_secs(2), fetch_json(&url)).await else {
        return Vec::new();
    };
    response.unwrap_or_default()
}

/// Probe common Expo / Metro ports on localhost and return all targets found.
///
/// Scans ports concurrently with a 2-second timeout per port.
pub async fn scan_localhost() -> Vec<(String, DebugTarget)> {
    let ports = [8081, 19000, 19001, 19002, 19003, 19004, 19005, 19006, 8082];
    let mut handles = Vec::new();
    for port in ports {
        let base_url = format!("http://localhost:{port}");
        handles.push(tokio::spawn(async move {
            let targets = discover_targets(&base_url).await;
            targets
                .into_iter()
                .map(move |t| (base_url.clone(), t))
                .collect::<Vec<_>>()
        }));
    }
    let mut all = Vec::new();
    for handle in handles {
        if let Ok(results) = handle.await {
            all.extend(results);
        }
    }
    all
}

/// Minimal HTTP GET that returns parsed JSON. Uses raw TCP + HTTP/1.1 to
/// avoid pulling in a heavy HTTP client dependency (reqwest, hyper, etc.).
///
/// This is intentionally bare-bones: no redirects, no TLS, no keep-alive.
/// It's only used for localhost Metro discovery where simplicity beats
/// feature coverage.
async fn fetch_json(url: &str) -> Result<Vec<DebugTarget>, FetchError> {
    let parsed: url::Url = url.parse().map_err(|_| FetchError::InvalidUrl)?;
    let host = parsed.host_str().ok_or(FetchError::InvalidUrl)?;
    let port = parsed.port().unwrap_or(80);
    let path = parsed.path();

    let addr = format!("{host}:{port}");
    let stream = tokio::net::TcpStream::connect(&addr)
        .await
        .map_err(|_| FetchError::Connect)?;

    let request = format!(
        "GET {path} HTTP/1.1\r\nHost: {host}:{port}\r\nAccept: application/json\r\nConnection: close\r\n\r\n"
    );

    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    let mut stream = stream;
    stream
        .write_all(request.as_bytes())
        .await
        .map_err(|_| FetchError::Io)?;

    let mut buf = Vec::new();
    stream
        .read_to_end(&mut buf)
        .await
        .map_err(|_| FetchError::Io)?;

    let response = String::from_utf8_lossy(&buf);

    // Split headers from body at the first empty line.
    let body = response
        .split_once("\r\n\r\n")
        .map(|(_, b)| b)
        .unwrap_or("");

    serde_json::from_str(body).map_err(|_| FetchError::Parse)
}

#[derive(Debug)]
enum FetchError {
    InvalidUrl,
    Connect,
    Io,
    Parse,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_debug_target() {
        let json = r#"[{
            "id": "page1",
            "title": "Hermes React Native",
            "type": "page",
            "devtoolsFrontendUrl": "",
            "webSocketDebuggerUrl": "ws://localhost:8081/inspector/debug?device=0&page=1",
            "description": ""
        }]"#;
        let targets: Vec<DebugTarget> = serde_json::from_str(json).unwrap();
        assert_eq!(targets.len(), 1);
        assert_eq!(targets[0].id, "page1");
        assert!(targets[0].web_socket_debugger_url.starts_with("ws://"));
    }

    #[test]
    fn parse_empty_list() {
        let json = "[]";
        let targets: Vec<DebugTarget> = serde_json::from_str(json).unwrap();
        assert!(targets.is_empty());
    }

    #[test]
    fn parse_target_with_extra_fields() {
        // Metro sometimes includes fields we don't model. Ensure we don't
        // fail on unknown fields.
        let json = r#"[{
            "id": "x",
            "title": "T",
            "type": "page",
            "webSocketDebuggerUrl": "ws://localhost:8081/ws",
            "vm": "Hermes",
            "reactNative": { "logicalDeviceId": "abc" }
        }]"#;
        let targets: Vec<DebugTarget> = serde_json::from_str(json).unwrap();
        assert_eq!(targets.len(), 1);
    }
}
