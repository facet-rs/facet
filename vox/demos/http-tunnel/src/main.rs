//! HTTP Tunnel Demo Binary
//!
//! This example demonstrates HTTP traffic flowing through a rapace tunnel:
//! - Browser → Host (TCP:4000) → rapace tunnel → Cell → axum (TCP:9876)
//!
//! Usage:
//!   cargo run -p rapace-http-tunnel
//!
//! Then test with:
//!   curl http://127.0.0.1:4000/hello
//!   curl http://127.0.0.1:4000/health

use std::sync::Arc;

use rapace::{AnyTransport, RpcSession};
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};

use rapace_http_tunnel::{
    GlobalTunnelMetrics, INTERNAL_HTTP_PORT, TcpTunnelImpl, TunnelHost, create_tunnel_dispatcher,
    run_host_server, run_http_server,
};

/// Port the host listens on for browser connections.
const HOST_PORT: u16 = 4000;

fn main() {
    let rt = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
        .unwrap();
    rt.block_on(async_main());
}

async fn async_main() {
    // Initialize tracing
    tracing_subscriber::registry()
        .with(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "info,rapace_http_tunnel=debug".into()),
        )
        .with(tracing_subscriber::fmt::layer())
        .init();

    println!("=== HTTP Tunnel over Rapace Demo ===\n");

    // Create in-memory transport pair for the demo
    let (host_transport, cell_transport) = AnyTransport::mem_pair();

    // Shared metrics
    let host_metrics = Arc::new(GlobalTunnelMetrics::new());
    let cell_metrics = Arc::new(GlobalTunnelMetrics::new());

    // ========== CELL SIDE ==========
    // Cell uses even channel IDs (2, 4, 6, ...)
    let cell_session = Arc::new(RpcSession::with_channel_start(cell_transport, 2));

    // Create the tunnel service
    let tunnel_service = Arc::new(TcpTunnelImpl::with_metrics(
        cell_session.clone(),
        INTERNAL_HTTP_PORT,
        cell_metrics.clone(),
    ));

    // Set dispatcher for TcpTunnel service
    cell_session.set_dispatcher(create_tunnel_dispatcher(
        tunnel_service.clone(),
        cell_session.buffer_pool().clone(),
    ));

    // Spawn the cell's demux loop
    let cell_session_clone = cell_session.clone();
    tokio::spawn(async move {
        if let Err(e) = cell_session_clone.run().await {
            tracing::error!(error = ?e, "cell session error");
        }
    });

    // Start the internal HTTP server
    tokio::spawn(async move {
        if let Err(e) = run_http_server(INTERNAL_HTTP_PORT).await {
            tracing::error!(error = %e, "internal HTTP server error");
        }
    });

    // ========== HOST SIDE ==========
    // Host uses odd channel IDs (1, 3, 5, ...)
    let host_session = Arc::new(RpcSession::with_channel_start(host_transport, 1));

    // Spawn the host's demux loop
    let host_session_clone = host_session.clone();
    tokio::spawn(async move {
        if let Err(e) = host_session_clone.run().await {
            tracing::error!(error = ?e, "host session error");
        }
    });

    // Create the tunnel host
    let tunnel_host = Arc::new(TunnelHost::with_metrics(
        host_session.clone(),
        host_metrics.clone(),
    ));

    // ========== RUN ==========
    println!(
        "Internal HTTP server running on 127.0.0.1:{}",
        INTERNAL_HTTP_PORT
    );
    println!("Host server running on 127.0.0.1:{}", HOST_PORT);
    println!();
    println!("Test with:");
    println!("  curl http://127.0.0.1:{}/hello", HOST_PORT);
    println!("  curl http://127.0.0.1:{}/health", HOST_PORT);
    println!(
        "  curl -X POST -d 'test data' http://127.0.0.1:{}/echo",
        HOST_PORT
    );
    println!();
    println!("Press Ctrl+C to exit\n");

    // Run the host server (blocks)
    if let Err(e) = run_host_server(tunnel_host, HOST_PORT).await {
        tracing::error!(error = %e, "host server error");
    }
}
