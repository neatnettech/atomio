//! CDP (Chrome DevTools Protocol) client for atomio.
//!
//! This crate owns the debugger connection lifecycle:
//!
//! 1. **Metro discovery** -- scan localhost for a running Metro bundler and
//!    enumerate its debuggable targets via the `/json/list` endpoint.
//! 2. **CDP transport** -- WebSocket connection to a Hermes debug endpoint,
//!    sending JSON-RPC requests and receiving responses + events.
//! 3. **Message types** -- strongly-typed wrappers around the CDP JSON-RPC
//!    protocol subset that Hermes exposes.
//!
//! The crate has **no gpui dependency**. The UI layer in `atomio` spawns a
//! tokio runtime, connects via this crate, and bridges events to the gpui
//! thread through channels.

pub mod cdp;
pub mod metro;
pub mod transport;
