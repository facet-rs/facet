//! Tracing Plugin Helper Binary
//!
//! This binary acts as the "plugin" side for cross-process testing.
//! It connects to the host via the specified transport, sets up the
//! RapaceTracingLayer, emits some traces, then exits.
//!
//! # Usage
//!
//! ```bash
//! # For stream transport (TCP)
//! tracing-plugin-helper --transport=stream --addr=127.0.0.1:12345
//!
//! # For stream transport (Unix socket)
//! tracing-plugin-helper --transport=stream --addr=/tmp/rapace-tracing.sock
//!
//! # For SHM transport (file-backed shared memory)
//! tracing-plugin-helper --transport=shm --addr=/tmp/rapace-tracing.shm
//! ```

use std::sync::Arc;
use std::time::Duration;

use rapace::transport::shm::{ShmSession, ShmSessionConfig, ShmTransport};
use rapace::{RpcSession, StreamTransport, Transport};
use tokio::io::{AsyncRead, AsyncWrite, ReadHalf, WriteHalf};
use tokio::net::TcpStream;
use tracing_subscriber::layer::SubscriberExt;

use rapace_tracing_over_rapace::RapaceTracingLayer;

#[derive(Debug)]
enum TransportType {
    Stream,
    Shm,
}

#[derive(Debug)]
struct Args {
    transport: TransportType,
    addr: String,
}

fn parse_args() -> Args {
    let args: Vec<String> = std::env::args().collect();

    let mut transport = None;
    let mut addr = None;

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
        }
        i += 1;
    }

    Args {
        transport: transport.expect("--transport required"),
        addr: addr.expect("--addr required"),
    }
}

async fn run_plugin_stream<S: AsyncRead + AsyncWrite + Send + Sync + 'static>(stream: S) {
    let transport: StreamTransport<ReadHalf<S>, WriteHalf<S>> = StreamTransport::new(stream);
    let transport = Arc::new(transport);
    run_plugin(transport).await;
}

async fn run_plugin<T: Transport + Send + Sync + 'static>(transport: Arc<T>) {
    // Plugin uses even channel IDs (2, 4, 6, ...)
    let session = Arc::new(RpcSession::with_channel_start(transport.clone(), 2));

    // Spawn the session runner
    let session_clone = session.clone();
    let _session_handle = tokio::spawn(async move { session_clone.run().await });

    // Create the tracing layer
    let (layer, _shared_filter) =
        RapaceTracingLayer::new(session.clone(), tokio::runtime::Handle::current());

    // Use a scoped subscriber for the traces
    let subscriber = tracing_subscriber::registry().with(layer);

    eprintln!("[tracing-plugin] Emitting traces...");

    // Emit a fixed pattern of traces that the host can verify
    tracing::subscriber::with_default(subscriber, || {
        // Simple event
        tracing::info!("plugin started");

        // Span with nested content
        let outer = tracing::info_span!("outer_span", request_id = 123);
        {
            let _outer_guard = outer.enter();
            tracing::debug!("inside outer span");

            let inner = tracing::debug_span!("inner_span", key = "value");
            {
                let _inner_guard = inner.enter();
                tracing::trace!("inside inner span");
            }
        }

        // Event with multiple fields
        tracing::warn!(
            user = "test_user",
            action = "test_action",
            count = 42,
            "final event"
        );
    });

    // Give time for async RPC calls to complete
    // SHM needs more time due to polling nature and we spawn many async tasks
    // that need to be scheduled and complete
    tokio::time::sleep(Duration::from_millis(1000)).await;

    eprintln!("[tracing-plugin] Done emitting traces");

    // Close transport
    let _ = transport.close().await;
}

#[tokio::main]
async fn main() {
    let args = parse_args();

    eprintln!(
        "[tracing-plugin] Starting with transport={:?} addr={}",
        args.transport, args.addr
    );

    match args.transport {
        TransportType::Stream => {
            if args.addr.contains(':') {
                // TCP
                eprintln!("[tracing-plugin] Connecting to TCP address: {}", args.addr);
                let stream = TcpStream::connect(&args.addr)
                    .await
                    .expect("failed to connect to host");
                eprintln!("[tracing-plugin] Connected!");
                run_plugin_stream(stream).await;
            } else {
                // Unix socket
                #[cfg(unix)]
                {
                    use tokio::net::UnixStream;
                    eprintln!("[tracing-plugin] Connecting to Unix socket: {}", args.addr);
                    let stream = UnixStream::connect(&args.addr)
                        .await
                        .expect("failed to connect to host");
                    eprintln!("[tracing-plugin] Connected!");
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
                tokio::time::sleep(Duration::from_millis(100)).await;
            }

            eprintln!("[tracing-plugin] Opening SHM file: {}", args.addr);
            let session = ShmSession::open_file(&args.addr, ShmSessionConfig::default())
                .expect("failed to open SHM file");
            let transport = Arc::new(ShmTransport::new(session));
            eprintln!("[tracing-plugin] SHM mapped!");
            run_plugin(transport).await;
        }
    }

    eprintln!("[tracing-plugin] Exiting");
}
