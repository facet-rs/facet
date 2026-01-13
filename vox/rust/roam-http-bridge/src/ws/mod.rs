//! WebSocket bridge for roam RPC services with streaming support.
//!
//! This module implements the WebSocket protocol for the HTTP bridge,
//! enabling streaming methods (with `Tx<T>`/`Rx<T>` channels) over HTTP.
//!
//! # Protocol
//!
//! r[bridge.ws.subprotocol] - Uses `roam-bridge.v1` subprotocol.
//! r[bridge.ws.text-frames] - All messages are JSON text frames.
//! r[bridge.ws.message-format] - Each message has a `type` field.

mod handler;
mod messages;
mod session;

pub use handler::handle_websocket;
pub use messages::WS_SUBPROTOCOL;
