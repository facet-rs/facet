//! Diagnostics Plugin Helper Binary
//!
//! This binary acts as the "plugin" side for cross-process testing.
//! It connects to the host via the specified transport and provides
//! the Diagnostics service using DiagnosticsServer::serve().
//!
//! # Usage
//!
//! ```bash
//! # For stream transport (TCP)
//! diagnostics-plugin-helper --transport=stream --addr=127.0.0.1:12345
//!
//! # For stream transport (Unix socket)
//! diagnostics-plugin-helper --transport=stream --addr=/tmp/rapace-diag.sock
//!
//! # For SHM transport (file-backed shared memory)
//! diagnostics-plugin-helper --transport=shm --addr=/tmp/rapace-diag.shm
//! ```

use std::sync::Arc;
use std::time::Duration;

use rapace::{
    transport::{
        shm::{ShmSession, ShmSessionConfig, ShmTransport},
        StreamTransport,
    },
    Transport,
};
use tokio::io::{AsyncRead, AsyncWrite, ReadHalf, WriteHalf};
use tokio::net::TcpStream;

use rapace_diagnostics_over_rapace::{DiagnosticsImpl, DiagnosticsServer};

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
    eprintln!("[diagnostics-plugin] Service ready, waiting for requests...");

    // Use DiagnosticsServer::serve() which handles the frame loop
    let server = DiagnosticsServer::new(DiagnosticsImpl);
    let _ = server.serve(transport).await;

    eprintln!("[diagnostics-plugin] Session ended");
}

#[tokio::main]
async fn main() {
    // Initialize tracing subscriber
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::from_default_env()
                .add_directive("rapace_core=debug".parse().unwrap())
                .add_directive("rapace_diagnostics_over_rapace=debug".parse().unwrap()),
        )
        .with_writer(std::io::stderr)
        .init();

    let args = parse_args();

    eprintln!(
        "[diagnostics-plugin] Starting with transport={:?} addr={}",
        args.transport, args.addr
    );

    match args.transport {
        TransportType::Stream => {
            if args.addr.contains(':') {
                // TCP
                eprintln!(
                    "[diagnostics-plugin] Connecting to TCP address: {}",
                    args.addr
                );
                let stream = TcpStream::connect(&args.addr)
                    .await
                    .expect("failed to connect to host");
                eprintln!("[diagnostics-plugin] Connected!");
                run_plugin_stream(stream).await;
            } else {
                // Unix socket
                #[cfg(unix)]
                {
                    use tokio::net::UnixStream;
                    eprintln!(
                        "[diagnostics-plugin] Connecting to Unix socket: {}",
                        args.addr
                    );
                    let stream = UnixStream::connect(&args.addr)
                        .await
                        .expect("failed to connect to host");
                    eprintln!("[diagnostics-plugin] Connected!");
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

            eprintln!("[diagnostics-plugin] Opening SHM file: {}", args.addr);
            let session = ShmSession::open_file(&args.addr, ShmSessionConfig::default())
                .expect("failed to open SHM file");
            let transport = Arc::new(ShmTransport::new(session));
            eprintln!("[diagnostics-plugin] SHM mapped!");
            run_plugin(transport).await;
        }
    }

    eprintln!("[diagnostics-plugin] Exiting");
}
