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
use tracing::{debug, info, warn};

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
    debug!(target: "atomio::metro", url = %url, "GET /json/list");
    match tokio::time::timeout(Duration::from_secs(2), fetch_json(&url)).await {
        Ok(Ok(targets)) => {
            debug!(
                target: "atomio::metro",
                base = %base_url,
                count = targets.len(),
                "discovered targets"
            );
            targets
        }
        Ok(Err(err)) => {
            debug!(
                target: "atomio::metro",
                base = %base_url,
                error = %err,
                "no targets (fetch error)"
            );
            Vec::new()
        }
        Err(_) => {
            debug!(target: "atomio::metro", base = %base_url, "no targets (timeout)");
            Vec::new()
        }
    }
}

/// Probe common Expo / Metro ports on localhost and return all targets found.
///
/// Scans ports concurrently with a 2-second timeout per port.
pub async fn scan_localhost() -> Vec<(String, DebugTarget)> {
    let ports = [8081, 19000, 19001, 19002, 19003, 19004, 19005, 19006, 8082];
    info!(target: "atomio::metro", ports = ?ports, "scanning Metro on localhost");
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
    if all.is_empty() {
        warn!(target: "atomio::metro", "no Metro targets on any port — is `expo start` running and a sim/device booted?");
    } else {
        info!(
            target: "atomio::metro",
            count = all.len(),
            first_url = %all[0].1.web_socket_debugger_url,
            "scan complete"
        );
    }
    all
}

/// Fetch a source file (or any text resource) from Metro at `url`. Used
/// to load script bodies and `.map` files referenced by
/// `Debugger.scriptParsed`. Returns the response body as `String`.
///
/// Wrapped in a 5-second timeout. Errors are coalesced into a single
/// string for ease of UI display.
pub async fn fetch_source(url: &str) -> Result<String, String> {
    debug!(target: "atomio::metro", url = %url, "fetch_source");
    let result = tokio::time::timeout(Duration::from_secs(5), http_get(url))
        .await
        .map_err(|_| "request timed out".to_string())?
        .map_err(|e| e.to_string());
    match &result {
        Ok(body) => debug!(
            target: "atomio::metro",
            url = %url,
            bytes = body.len(),
            "fetch_source ok"
        ),
        Err(e) => warn!(target: "atomio::metro", url = %url, error = %e, "fetch_source failed"),
    }
    result
}

/// Minimal HTTP GET that returns parsed JSON for the discovery endpoint.
async fn fetch_json(url: &str) -> Result<Vec<DebugTarget>, FetchError> {
    let body = http_get(url).await?;
    serde_json::from_str(&body).map_err(|_| FetchError::Parse)
}

/// Bare-bones HTTP/1.1 GET over raw TCP. No redirects, no TLS, no
/// keep-alive. Localhost-only by design; we only talk to Metro on
/// `http://localhost:<port>`.
async fn http_get(url: &str) -> Result<String, FetchError> {
    let parsed: url::Url = url.parse().map_err(|_| FetchError::InvalidUrl)?;
    let host = parsed.host_str().ok_or(FetchError::InvalidUrl)?;
    let port = parsed.port().unwrap_or(80);
    let path_query = match parsed.query() {
        Some(q) => format!("{}?{q}", parsed.path()),
        None => parsed.path().to_string(),
    };

    let addr = format!("{host}:{port}");
    let stream = tokio::net::TcpStream::connect(&addr)
        .await
        .map_err(|_| FetchError::Connect)?;

    let request = format!(
        "GET {path_query} HTTP/1.1\r\nHost: {host}:{port}\r\nAccept: */*\r\nConnection: close\r\n\r\n"
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

    Ok(body.to_string())
}

#[derive(Debug)]
enum FetchError {
    InvalidUrl,
    Connect,
    Io,
    Parse,
}

impl std::fmt::Display for FetchError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            FetchError::InvalidUrl => write!(f, "invalid URL"),
            FetchError::Connect => write!(f, "connection failed"),
            FetchError::Io => write!(f, "I/O error"),
            FetchError::Parse => write!(f, "parse error"),
        }
    }
}

impl std::error::Error for FetchError {}

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
