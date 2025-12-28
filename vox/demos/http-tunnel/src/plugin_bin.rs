//! HTTP Tunnel Plugin Helper Binary
//!
//! This binary acts as the "plugin" side for cross-process testing.
//! It connects to the host via the specified transport and runs the
//! TcpTunnel service with the internal axum HTTP server.
//!
//! # Usage
//!
//! ```bash
//! # For stream transport (TCP)
//! http-tunnel-plugin --transport=stream --addr=127.0.0.1:12345
//!
//! # For stream transport (Unix socket)
//! http-tunnel-plugin --transport=stream --addr=/tmp/rapace-tunnel.sock
//! ```
//!
//! Note: SHM transport is not supported in this binary because it requires
//! the hub architecture where the host spawns the plugin process to pass
//! the doorbell file descriptor via inheritance.

use std::sync::Arc;

use rapace::{AnyTransport, RpcSession};
use tokio::io::{AsyncRead, AsyncWrite};
use tokio::net::TcpStream;
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};

use rapace_http_tunnel::{
    GlobalTunnelMetrics, INTERNAL_HTTP_PORT, TcpTunnelImpl, create_tunnel_dispatcher,
    run_http_server,
};

#[derive(Debug)]
struct Args {
    addr: String,
}

fn parse_args() -> Args {
    let args: Vec<String> = std::env::args().collect();

    let mut addr = None;

    let mut i = 1;
    while i < args.len() {
        if args[i].starts_with("--transport=") {
            let t = args[i].strip_prefix("--transport=").unwrap();
            if t != "stream" {
                panic!(
                    "Only stream transport is supported. SHM requires hub architecture \
                     where the host spawns the plugin process."
                );
            }
        } else if args[i].starts_with("--addr=") {
            addr = Some(args[i].strip_prefix("--addr=").unwrap().to_string());
        }
        i += 1;
    }

    Args {
        addr: addr.expect("--addr required"),
    }
}

async fn run_plugin(transport: AnyTransport) {
    let metrics = Arc::new(GlobalTunnelMetrics::new());

    // Plugin uses even channel IDs (2, 4, 6, ...)
    let session = Arc::new(RpcSession::with_channel_start(transport, 2));

    // Create the tunnel service
    let tunnel_service = Arc::new(TcpTunnelImpl::with_metrics(
        session.clone(),
        INTERNAL_HTTP_PORT,
        metrics.clone(),
    ));

    // Set dispatcher
    session.set_dispatcher(create_tunnel_dispatcher(
        tunnel_service,
        session.buffer_pool().clone(),
    ));

    // Run the session until the transport closes
    eprintln!("[http-tunnel-plugin] Session running...");
    if let Err(e) = session.run().await {
        eprintln!("[http-tunnel-plugin] Session ended with error: {:?}", e);
    } else {
        eprintln!("[http-tunnel-plugin] Session ended normally");
    }
    eprintln!("[http-tunnel-plugin] Metrics: {}", metrics.summary());
}

async fn run_plugin_stream<S: AsyncRead + AsyncWrite + Unpin + Send + Sync + 'static>(stream: S) {
    run_plugin(AnyTransport::stream(stream)).await;
}

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
        .with(tracing_subscriber::fmt::layer().with_writer(std::io::stderr))
        .init();

    let args = parse_args();

    eprintln!(
        "[http-tunnel-plugin] Starting with stream transport, addr={}",
        args.addr
    );

    // Start the internal HTTP server first
    tokio::spawn(async move {
        if let Err(e) = run_http_server(INTERNAL_HTTP_PORT).await {
            eprintln!("[http-tunnel-plugin] HTTP server error: {}", e);
        }
    });

    // Give the HTTP server a moment to start
    tokio::time::sleep(std::time::Duration::from_millis(100)).await;

    // Check if it's a TCP address (contains ':') or Unix socket path
    if args.addr.contains(':') {
        // TCP
        eprintln!(
            "[http-tunnel-plugin] Connecting to TCP address: {}",
            args.addr
        );
        let stream = TcpStream::connect(&args.addr)
            .await
            .expect("failed to connect to host");
        eprintln!("[http-tunnel-plugin] Connected!");
        run_plugin_stream(stream).await;
    } else {
        // Unix socket
        #[cfg(unix)]
        {
            use tokio::net::UnixStream;
            eprintln!(
                "[http-tunnel-plugin] Connecting to Unix socket: {}",
                args.addr
            );
            let stream = UnixStream::connect(&args.addr)
                .await
                .expect("failed to connect to host");
            eprintln!("[http-tunnel-plugin] Connected!");
            run_plugin_stream(stream).await;
        }
        #[cfg(not(unix))]
        {
            panic!("Unix sockets not supported on this platform");
        }
    }

    eprintln!("[http-tunnel-plugin] Exiting");
}
