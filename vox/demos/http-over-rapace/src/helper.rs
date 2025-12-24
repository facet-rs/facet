//! HTTP Plugin Helper Binary
//!
//! This binary acts as the "plugin" side for cross-process testing.
//! It connects to the host via the specified transport and runs the
//! HttpService (axum router), handling HTTP requests via RPC.
//!
//! # Usage
//!
//! ```bash
//! # For stream transport (TCP)
//! http-plugin-helper --transport=stream --addr=127.0.0.1:12345
//!
//! # For stream transport (Unix socket)
//! http-plugin-helper --transport=stream --addr=/tmp/rapace-http.sock
//!
//! # For SHM transport (file-backed shared memory)
//! http-plugin-helper --transport=shm --addr=/tmp/rapace-http.shm
//! ```
//!
//! The helper:
//! 1. Connects to the host at the specified address
//! 2. Creates an RpcSession with the HttpService dispatcher (axum router)
//! 3. Runs until the connection closes
//!
//! The host side is responsible for:
//! 1. Listening on the address (or creating the SHM file)
//! 2. Accepting the connection
//! 3. Running an HTTP server that proxies requests via RPC

use std::sync::Arc;

use rapace::transport::shm::{ShmSession, ShmSessionConfig, ShmTransport};
use rapace::{RpcSession, Transport};
use tokio::io::{AsyncRead, AsyncWrite};
use tokio::net::TcpStream;

use rapace_http_over_rapace::{AxumHttpService, create_http_service_dispatcher};

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
    eprintln!("[http-plugin] Accepted inherited stream from {:?}", peer);
    Some(stream)
}

#[cfg(not(unix))]
async fn accept_inherited_stream() -> Option<TcpStream> {
    None
}

async fn run_cell_stream<S: AsyncRead + AsyncWrite + Unpin + Send + Sync + 'static>(stream: S) {
    let transport = Transport::stream(stream);
    run_cell(transport).await;
}

async fn run_cell(transport: Transport) {
    // Cell uses even channel IDs (2, 4, 6, ...)
    let session = Arc::new(RpcSession::with_channel_start(transport, 2));

    // Create axum-based HTTP service with demo routes
    let http_service = AxumHttpService::with_demo_routes();

    // Set up HttpService dispatcher
    session.set_dispatcher(create_http_service_dispatcher(
        http_service,
        session.buffer_pool().clone(),
    ));

    // Run the session until the transport closes
    eprintln!("[http-cell] Session running...");
    if let Err(e) = session.run().await {
        eprintln!("[http-cell] Session ended with error: {:?}", e);
    } else {
        eprintln!("[http-cell] Session ended normally");
    }
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
        "[http-cell] Starting with transport={:?} addr={:?}",
        args.transport, args.addr
    );

    match args.transport {
        TransportType::Stream => {
            if let Some(stream) = accept_inherited_stream().await {
                eprintln!("[http-cell] Using inherited TCP listener");
                run_cell_stream(stream).await;
                return;
            }

            // Check if it's a TCP address (contains ':') or Unix socket path
            let addr = args
                .addr
                .as_ref()
                .expect("--addr required when no inherited listener");

            if addr.contains(':') {
                // TCP
                eprintln!("[http-cell] Connecting to TCP address: {}", addr);
                let stream = TcpStream::connect(addr)
                    .await
                    .expect("failed to connect to host");
                eprintln!("[http-cell] Connected!");
                run_cell_stream(stream).await;
            } else {
                // Unix socket
                #[cfg(unix)]
                {
                    use tokio::net::UnixStream;
                    eprintln!("[http-cell] Connecting to Unix socket: {}", addr);
                    let stream = UnixStream::connect(addr)
                        .await
                        .expect("failed to connect to host");
                    eprintln!("[http-cell] Connected!");
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
            // Wait a bit for the host to create the SHM file
            for i in 0..50 {
                if std::path::Path::new(addr).exists() {
                    break;
                }
                if i == 49 {
                    panic!("SHM file not created by host: {}", addr);
                }
                tokio::time::sleep(std::time::Duration::from_millis(100)).await;
            }

            eprintln!("[http-cell] Opening SHM file: {}", addr);
            let session = ShmSession::open_file(addr, ShmSessionConfig::default())
                .expect("failed to open SHM file");
            let transport = Transport::Shm(ShmTransport::new(session));
            eprintln!("[http-cell] SHM mapped!");
            run_cell(transport).await;
        }
    }

    eprintln!("[http-cell] Exiting");
}
