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

pub mod protocol;
pub mod plugin;
pub mod host;
pub mod metrics;

// Re-export key types explicitly to avoid ambiguous glob conflicts
pub use protocol::{TcpTunnel, TcpTunnelClient, TcpTunnelRpcClient, TcpTunnelServer, TunnelHandle};
pub use plugin::{
    create_tunnel_dispatcher, create_demo_router, run_http_server,
    TcpTunnelImpl, INTERNAL_HTTP_PORT,
};
pub use host::{run_host_server, TunnelHost};
pub use metrics::{GlobalTunnelMetrics, TunnelMetrics};
