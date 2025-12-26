//! HTTP Tunnel Host Binary
//!
//! This binary acts as the "host" side for benchmarking and cross-process testing.
//! It listens for HTTP connections on port 4000 and tunnels them through rapace
//! to a plugin process.
//!
//! # Usage
//!
//! ```bash
//! # For stream transport (Unix socket)
//! http-tunnel-host --transport=stream --addr=/tmp/rapace-tunnel.sock
//!
//! # For stream transport (TCP)
//! http-tunnel-host --transport=stream --addr=127.0.0.1:12345
//!
//! # For SHM transport
//! http-tunnel-host --transport=shm --addr=/tmp/rapace-tunnel.shm
//! ```
//!
//! Then start the plugin with the same transport and address.

use std::sync::Arc;

use rapace::transport::shm::{ShmMetrics, ShmSession, ShmSessionConfig, ShmTransport};
use rapace::{AnyTransport, RpcSession};
use tokio::io::{AsyncRead, AsyncWrite};
use tokio::net::TcpListener;
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};

use rapace_http_tunnel::{GlobalTunnelMetrics, TunnelHost, run_host_server};

/// Port the host listens on for browser connections.
const HOST_PORT: u16 = 4000;

#[derive(Debug, Clone, Copy)]
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

async fn run_host(transport: AnyTransport) {
    let metrics = Arc::new(GlobalTunnelMetrics::new());

    // Host uses odd channel IDs (1, 3, 5, ...)
    let session = Arc::new(RpcSession::with_channel_start(transport, 1));

    // Spawn the session demux loop
    let session_clone = session.clone();
    tokio::spawn(async move {
        if let Err(e) = session_clone.run().await {
            eprintln!("[http-tunnel-host] Session ended with error: {:?}", e);
        }
    });

    // Create the tunnel host
    let tunnel_host = Arc::new(TunnelHost::with_metrics(session.clone(), metrics.clone()));

    eprintln!(
        "[http-tunnel-host] Host server running on 127.0.0.1:{}",
        HOST_PORT
    );
    eprintln!("[http-tunnel-host] Ready to accept connections");

    // Run the host server
    if let Err(e) = run_host_server(tunnel_host, HOST_PORT).await {
        eprintln!("[http-tunnel-host] Host server error: {:?}", e);
    }

    eprintln!("[http-tunnel-host] Metrics: {}", metrics.summary());
}

async fn run_host_stream<S: AsyncRead + AsyncWrite + Unpin + Send + Sync + 'static>(stream: S) {
    run_host(AnyTransport::stream(stream)).await;
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
            tracing_subscriber::EnvFilter::try_from_default_env().unwrap_or_else(|_| "warn".into()),
        )
        .with(tracing_subscriber::fmt::layer().with_writer(std::io::stderr))
        .init();

    let args = parse_args();

    eprintln!(
        "[http-tunnel-host] Starting with transport={:?} addr={}",
        args.transport, args.addr
    );

    match args.transport {
        TransportType::Stream => {
            // Check if it's a TCP address (contains ':') or Unix socket path
            if args.addr.contains(':') {
                // TCP: listen and wait for plugin to connect
                eprintln!(
                    "[http-tunnel-host] Listening for TCP connection on: {}",
                    args.addr
                );
                let listener = TcpListener::bind(&args.addr)
                    .await
                    .expect("failed to bind TCP listener");
                let (stream, peer) = listener
                    .accept()
                    .await
                    .expect("failed to accept connection");
                eprintln!("[http-tunnel-host] Plugin connected from: {}", peer);
                run_host_stream(stream).await;
            } else {
                // Unix socket: listen and wait for plugin to connect
                #[cfg(unix)]
                {
                    use tokio::net::UnixListener;
                    // Remove existing socket if present
                    let _ = std::fs::remove_file(&args.addr);
                    eprintln!(
                        "[http-tunnel-host] Listening for Unix socket connection on: {}",
                        args.addr
                    );
                    let listener =
                        UnixListener::bind(&args.addr).expect("failed to bind Unix listener");
                    let (stream, _) = listener
                        .accept()
                        .await
                        .expect("failed to accept connection");
                    eprintln!("[http-tunnel-host] Plugin connected!");
                    run_host_stream(stream).await;
                }
                #[cfg(not(unix))]
                {
                    panic!("Unix sockets not supported on this platform");
                }
            }
        }
        TransportType::Shm => {
            // Create the SHM file as host
            eprintln!("[http-tunnel-host] Creating SHM file: {}", args.addr);
            // Remove existing if present
            let _ = std::fs::remove_file(&args.addr);

            // Choose SHM config based on flag
            let config = match args.shm_size {
                ShmSize::Default => {
                    eprintln!("[http-tunnel-host] SHM config: DEFAULT (64 slots × 4KB = 256KB)");
                    ShmSessionConfig::default()
                }
                ShmSize::Large => {
                    eprintln!("[http-tunnel-host] SHM config: LARGE (256 slots × 16KB = 4MB)");
                    ShmSessionConfig {
                        ring_capacity: 512,
                        slot_size: 16384, // 16KB per slot
                        slot_count: 256,  // 256 slots = 4MB total
                    }
                }
            };

            let session =
                ShmSession::create_file(&args.addr, config).expect("failed to create SHM file");

            // Create metrics for SHM transport
            let shm_metrics = Arc::new(ShmMetrics::new());
            let transport =
                AnyTransport::new(ShmTransport::new_with_metrics(session, shm_metrics.clone()));
            eprintln!("[http-tunnel-host] SHM created, waiting for plugin...");
            // Give plugin time to map
            tokio::time::sleep(std::time::Duration::from_millis(500)).await;
            run_host(transport).await;
            eprintln!("[http-tunnel-host] SHM metrics: {}", shm_metrics.summary());
        }
    }

    eprintln!("[http-tunnel-host] Exiting");
}
