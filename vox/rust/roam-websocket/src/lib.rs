#![deny(unsafe_code)]

//! WebSocket transport layer for roam RPC.
//!
//! This crate provides WebSocket support for roam services using the
//! `MessageTransport` trait from `roam-session`.
//!
//! Unlike byte stream transports (TCP, Unix sockets), WebSocket provides
//! native message framing, so no additional byte-stream framing is needed.
//!
//! r[impl transport.message.one-to-one] - Each WebSocket message = one roam message.
//! r[impl transport.message.binary] - Uses binary WebSocket frames.
//! r[impl transport.message.multiplexing] - channel_id field provides multiplexing.
//!
//! # Platform Support
//!
//! This crate provides WebSocket transport for both native (tokio) and WASM (browser)
//! environments:
//!
//! - **Native**: Uses `tokio-tungstenite` for async WebSocket support
//! - **WASM**: Uses `web_sys::WebSocket` browser API
//!
//! # Example (Native - Accepting connections)
//!
//! ```ignore
//! use roam_websocket::{WsTransport, ws_accept};
//! use roam_stream::{HandshakeConfig, ServiceDispatcher};
//!
//! // Server: accept WebSocket connection
//! let ws_stream = accept_async(tcp_stream).await?;
//! let transport = WsTransport::new(ws_stream);
//! let (handle, driver) = ws_accept(transport, HandshakeConfig::default(), dispatcher).await?;
//! tokio::spawn(driver.run());
//! ```
//!
//! # Example (WASM - Browser client)
//!
//! ```ignore
//! use roam_websocket::WsTransport;
//! use roam_stream::{HandshakeConfig, accept_framed, NoDispatcher};
//!
//! // Connect to a WebSocket server from the browser
//! let transport = WsTransport::connect("ws://localhost:9000").await?;
//! let (handle, driver) = accept_framed(transport, HandshakeConfig::default(), NoDispatcher).await?;
//! wasm_bindgen_futures::spawn_local(driver.run());
//! ```

// Platform-specific implementations
#[cfg(not(target_arch = "wasm32"))]
mod native;
#[cfg(not(target_arch = "wasm32"))]
pub use native::*;

#[cfg(target_arch = "wasm32")]
mod wasm;
#[cfg(target_arch = "wasm32")]
pub use wasm::*;
