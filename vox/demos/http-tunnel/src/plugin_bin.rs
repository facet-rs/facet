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
//! # For SHM transport
//! http-tunnel-plugin --transport=shm --addr=/tmp/rapace-tunnel.shm
//! ```

use std::sync::Arc;

use rapace::transport::shm::{ShmMetrics, ShmSession, ShmSessionConfig, ShmTransport};
use rapace::{RpcSession, Transport};
use tokio::io::{AsyncRead, AsyncWrite};
use tokio::net::TcpStream;
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};

use rapace_http_tunnel::{
    GlobalTunnelMetrics, INTERNAL_HTTP_PORT, TcpTunnelImpl, create_tunnel_dispatcher,
    run_http_server,
};

#[derive(Debug)]
enum TransportType {
    Stream,
    Shm,
}

#[derive(Debug, Clone, Copy)]
enum ShmSize {
    Default, // 64 slots × 4KB = 256KB
    Large,   // 256 slots × 16KB = 4MB
}

#[derive(Debug)]
struct Args {
    transport: TransportType,
    addr: String,
    shm_size: ShmSize,
}

fn parse_args() -> Args {
    let args: Vec<String> = std::env::args().collect();

    let mut transport = None;
    let mut addr = None;
    let mut shm_size = ShmSize::Default;

    let mut i = 1;
    while i < args.len() {
        if args[i].starts_with("--transport=") {
            let t = args[i].strip_prefix("--transport=").unwrap();
            transport = Some(match t {
                "stream" => TransportType::Stream,
                "shm" => TransportType::Shm,
                _ => panic!("unknown transport: {}", t),
            });
        } else if args[i].starts_with("--addr=") {
            addr = Some(args[i].strip_prefix("--addr=").unwrap().to_string());
        } else if args[i] == "--shm-large" {
            shm_size = ShmSize::Large;
        }
        i += 1;
    }

    Args {
        transport: transport.expect("--transport required"),
        addr: addr.expect("--addr required"),
        shm_size,
    }
}

async fn run_plugin(transport: Transport) {
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
    run_plugin(Transport::stream(stream)).await;
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
        "[http-tunnel-plugin] Starting with transport={:?} addr={}",
        args.transport, args.addr
    );

    // Start the internal HTTP server first
    tokio::spawn(async move {
        if let Err(e) = run_http_server(INTERNAL_HTTP_PORT).await {
            eprintln!("[http-tunnel-plugin] HTTP server error: {}", e);
        }
    });

    // Give the HTTP server a moment to start
    tokio::time::sleep(std::time::Duration::from_millis(100)).await;

    match args.transport {
        TransportType::Stream => {
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
        }
        TransportType::Shm => {
            // Wait for the host to create the SHM file
            for i in 0..50 {
                if std::path::Path::new(&args.addr).exists() {
                    break;
                }
                if i == 49 {
                    panic!("SHM file not created by host: {}", args.addr);
                }
                tokio::time::sleep(std::time::Duration::from_millis(100)).await;
            }

            // Choose SHM config based on flag (must match host's config)
            let config = match args.shm_size {
                ShmSize::Default => {
                    eprintln!("[http-tunnel-plugin] SHM config: DEFAULT (64 slots × 4KB = 256KB)");
                    ShmSessionConfig::default()
                }
                ShmSize::Large => {
                    eprintln!("[http-tunnel-plugin] SHM config: LARGE (256 slots × 16KB = 4MB)");
                    ShmSessionConfig {
                        ring_capacity: 512,
                        slot_size: 16384, // 16KB per slot
                        slot_count: 256,  // 256 slots = 4MB total
                    }
                }
            };

            eprintln!("[http-tunnel-plugin] Opening SHM file: {}", args.addr);
            let session =
                ShmSession::open_file(&args.addr, config).expect("failed to open SHM file");

            // Create metrics for SHM transport
            let shm_metrics = Arc::new(ShmMetrics::new());
            let transport =
                Transport::Shm(ShmTransport::new_with_metrics(session, shm_metrics.clone()));
            eprintln!("[http-tunnel-plugin] SHM mapped!");
            run_plugin(transport).await;
            eprintln!(
                "[http-tunnel-plugin] SHM metrics: {}",
                shm_metrics.summary()
            );
        }
    }

    eprintln!("[http-tunnel-plugin] Exiting");
}
