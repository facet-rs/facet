//! WebSocket transport for roam.
//!
//! Implements [`Link`](roam_types::Link) over a WebSocket connection.
//! Each roam message maps 1:1 to a WebSocket binary frame.
//!
//! - **Native**: uses `tokio-tungstenite`
//! - **WASM**: uses `web_sys::WebSocket`

#[cfg(not(target_arch = "wasm32"))]
mod native;
#[cfg(not(target_arch = "wasm32"))]
pub use native::*;

#[cfg(target_arch = "wasm32")]
mod wasm;
#[cfg(target_arch = "wasm32")]
pub use wasm::*;
