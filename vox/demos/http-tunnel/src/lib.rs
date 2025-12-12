//! HTTP Tunnel over Rapace
//!
//! This example demonstrates using rapace as a generic full-duplex byte tunnel
//! between two processes, carrying real HTTP traffic through it:
//!
//! - **Host**: Accepts TCP connections from browsers and forwards raw bytes over rapace.
//! - **Plugin**: Implements a TCP tunnel service and forwards bytes to a local axum HTTP server.
//!
//! No HTTP awareness in rapace - it's just frames + flow control + cancellation.
//!
//! # Architecture
//!
//! ```text
//! ┌─────────────────────────────────────────────────────────────────────────┐
//! │                              HOST PROCESS                               │
//! │                                                                         │
//! │   Browser ──► TCP:4000 ──► TcpTunnelClient::open() ─────────────────────┤
//! │               (accept)      (get channel_id, bidirectional stream)      │
//! └────────────────────────────────────────────────────────────────────────┬┘
//!                                                                          │
//!                              rapace transport (TCP/Unix/SHM)             │
//!                              (raw BytesChunk frames on channel)          │
//!                                                                          │
//! ┌────────────────────────────────────────────────────────────────────────┴┐
//! │                             PLUGIN PROCESS                              │
//! │                                                                         │
//! │   TcpTunnelServer ──► connect to local axum ──► axum::Router            │
//! │   (on open, connect)    TCP:INTERNAL_PORT       (HTTP handlers)         │
//! └─────────────────────────────────────────────────────────────────────────┘
//! ```

pub mod host;
pub mod metrics;
pub mod plugin;
pub mod protocol;

// Re-export key types explicitly to avoid ambiguous glob conflicts
pub use host::{TunnelHost, run_host_server};
pub use metrics::{GlobalTunnelMetrics, TunnelMetrics};
pub use plugin::{
    INTERNAL_HTTP_PORT, TcpTunnelImpl, create_demo_router, create_tunnel_dispatcher,
    run_http_server,
};
pub use protocol::{TcpTunnel, TcpTunnelClient, TcpTunnelServer, TunnelHandle};
