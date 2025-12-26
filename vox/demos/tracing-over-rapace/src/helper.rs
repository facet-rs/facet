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

use rapace::transport::shm::{ShmSession, ShmSessionConfig};
use rapace::{AnyTransport, RpcSession};
use tokio::io::{AsyncRead, AsyncWrite};
use tokio::net::TcpStream;
use tracing_subscriber::layer::SubscriberExt;

use rapace_tracing_over_rapace::RapaceTracingLayer;

#[cfg(unix)]
const STREAM_CONTROL_ENV: &str = "RAPACE_STREAM_CONTROL_FD";

#[cfg(unix)]
async fn accept_inherited_stream() -> Option<TcpStream> {
    use async_send_fd::AsyncRecvFd;
    use std::os::unix::{
        io::{FromRawFd, RawFd},
        net::UnixStream as StdUnixStream,
    };
    use tokio::{
        io::AsyncWriteExt,
        net::{TcpListener, UnixStream},
    };

    let fd_str = match std::env::var(STREAM_CONTROL_ENV) {
        Ok(val) => val,
        Err(_) => return None,
    };

    let fd: RawFd = fd_str
        .parse()
        .expect("RAPACE_STREAM_CONTROL_FD is not a valid fd");
    let control_std = unsafe { StdUnixStream::from_raw_fd(fd) };
    control_std
        .set_nonblocking(true)
        .expect("failed to configure control socket");
    let mut control = UnixStream::from_std(control_std).expect("failed to create control stream");

    let listener_fd = control
        .recv_fd()
        .await
        .expect("failed to receive listener fd");
    let std_listener = unsafe { std::net::TcpListener::from_raw_fd(listener_fd) };
    std_listener
        .set_nonblocking(true)
        .expect("failed to configure listener");
    let listener = TcpListener::from_std(std_listener).expect("failed to create tokio listener");

    control
        .write_all(&[1u8])
        .await
        .expect("failed to ack listener receipt");

    let (stream, peer) = listener
        .accept()
        .await
        .expect("failed to accept inherited stream");
    eprintln!("[tracing-plugin] Accepted inherited stream from {:?}", peer);
    Some(stream)
}

#[cfg(not(unix))]
async fn accept_inherited_stream() -> Option<TcpStream> {
    None
}

#[derive(Debug)]
enum TransportType {
    Stream,
    Shm,
}

#[derive(Debug)]
struct Args {
    transport: TransportType,
    addr: Option<String>,
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
        addr,
    }
}

async fn run_cell_stream<S: AsyncRead + AsyncWrite + Unpin + Send + Sync + 'static>(stream: S) {
    let transport = AnyTransport::stream(stream);
    run_cell(transport).await;
}

async fn run_cell(transport: AnyTransport) {
    // Cell uses even channel IDs (2, 4, 6, ...)
    let session = Arc::new(RpcSession::with_channel_start(transport, 2));

    // Spawn the session runner
    let session_clone = session.clone();
    let _session_handle = tokio::spawn(async move { session_clone.run().await });

    // Create the tracing layer
    let (layer, shared_filter) =
        RapaceTracingLayer::new(session.clone(), tokio::runtime::Handle::current());
    // Set filter to allow all levels (default is "warn")
    shared_filter.set_filter("trace");

    // Use a scoped subscriber for the traces
    let subscriber = tracing_subscriber::registry().with(layer);

    eprintln!("[tracing-cell] Emitting traces...");

    // Emit a fixed pattern of traces that the host can verify
    tracing::subscriber::with_default(subscriber, || {
        // Simple event
        tracing::info!("cell started");

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

    eprintln!("[tracing-cell] Done emitting traces");

    // Close session
    session.close();
}

fn main() {
    let rt = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
        .unwrap();
    rt.block_on(async_main());
}

async fn async_main() {
    let args = parse_args();

    eprintln!(
        "[tracing-cell] Starting with transport={:?} addr={:?}",
        args.transport, args.addr
    );

    match args.transport {
        TransportType::Stream => {
            if let Some(stream) = accept_inherited_stream().await {
                eprintln!("[tracing-cell] Using inherited TCP listener");
                run_cell_stream(stream).await;
                return;
            }

            let addr = args
                .addr
                .as_ref()
                .expect("--addr required when no inherited listener");

            if addr.contains(':') {
                // TCP
                eprintln!("[tracing-cell] Connecting to TCP address: {}", addr);
                let stream = TcpStream::connect(addr)
                    .await
                    .expect("failed to connect to host");
                eprintln!("[tracing-cell] Connected!");
                run_cell_stream(stream).await;
            } else {
                // Unix socket
                #[cfg(unix)]
                {
                    use tokio::net::UnixStream;
                    eprintln!("[tracing-cell] Connecting to Unix socket: {}", addr);
                    let stream = UnixStream::connect(addr)
                        .await
                        .expect("failed to connect to host");
                    eprintln!("[tracing-cell] Connected!");
                    run_cell_stream(stream).await;
                }
                #[cfg(not(unix))]
                {
                    panic!("Unix sockets not supported on this platform");
                }
            }
        }
        TransportType::Shm => {
            let addr = args
                .addr
                .as_ref()
                .expect("--addr required for shm transport");
            // Wait for the host to create the SHM file
            for i in 0..50 {
                if std::path::Path::new(addr).exists() {
                    break;
                }
                if i == 49 {
                    panic!("SHM file not created by host: {}", addr);
                }
                tokio::time::sleep(Duration::from_millis(100)).await;
            }

            eprintln!("[tracing-cell] Opening SHM file: {}", addr);
            let session = ShmSession::open_file(addr, ShmSessionConfig::default())
                .expect("failed to open SHM file");
            let transport = AnyTransport::shm(session);
            eprintln!("[tracing-cell] SHM mapped!");
            run_cell(transport).await;
        }
    }

    eprintln!("[tracing-cell] Exiting");
}
