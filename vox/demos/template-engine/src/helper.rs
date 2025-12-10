//! Template Engine Helper Binary
//!
//! This binary acts as the "plugin" side for cross-process testing.
//! It connects to the host via the specified transport and runs the
//! TemplateEngine service, calling back to the host for values.
//!
//! # Usage
//!
//! ```bash
//! # For stream transport (TCP)
//! template-engine-helper --transport=stream --addr=127.0.0.1:12345
//!
//! # For stream transport (Unix socket)
//! template-engine-helper --transport=stream --addr=/tmp/rapace.sock
//!
//! # For SHM transport (file-backed shared memory)
//! template-engine-helper --transport=shm --addr=/tmp/rapace.shm
//! ```
//!
//! The helper:
//! 1. Connects to the host at the specified address
//! 2. Creates an RpcSession with the TemplateEngine dispatcher
//! 3. Runs until the connection closes
//!
//! The host side is responsible for:
//! 1. Listening on the address (or creating the SHM file)
//! 2. Accepting the connection
//! 3. Creating an RpcSession with the ValueHost dispatcher
//! 4. Sending render requests

use std::sync::Arc;

use rapace::transport::shm::{ShmSession, ShmSessionConfig, ShmTransport};
use rapace::{RpcSession, StreamTransport, Transport};
use tokio::io::{AsyncRead, AsyncWrite, ReadHalf, WriteHalf};
use tokio::net::TcpStream;

use rapace_template_engine::create_template_engine_dispatcher;

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

    // Plugin uses even channel IDs (2, 4, 6, ...)
    let session = Arc::new(RpcSession::with_channel_start(transport.clone(), 2));

    // Set up TemplateEngine dispatcher
    session.set_dispatcher(create_template_engine_dispatcher(session.clone()));

    // Run the session until the transport closes
    eprintln!("[helper] Plugin session running...");
    if let Err(e) = session.run().await {
        eprintln!("[helper] Session ended with error: {:?}", e);
    } else {
        eprintln!("[helper] Session ended normally");
    }
}

async fn run_plugin_shm<T: Transport + Send + Sync + 'static>(transport: Arc<T>) {
    // Plugin uses even channel IDs (2, 4, 6, ...)
    let session = Arc::new(RpcSession::with_channel_start(transport.clone(), 2));

    // Set up TemplateEngine dispatcher
    session.set_dispatcher(create_template_engine_dispatcher(session.clone()));

    // Run the session until the transport closes
    eprintln!("[helper] Plugin session running...");
    if let Err(e) = session.run().await {
        eprintln!("[helper] Session ended with error: {:?}", e);
    } else {
        eprintln!("[helper] Session ended normally");
    }
}

#[tokio::main]
async fn main() {
    let args = parse_args();

    eprintln!(
        "[helper] Starting with transport={:?} addr={}",
        args.transport, args.addr
    );

    match args.transport {
        TransportType::Stream => {
            // Check if it's a TCP address (contains ':') or Unix socket path
            if args.addr.contains(':') {
                // TCP
                eprintln!("[helper] Connecting to TCP address: {}", args.addr);
                let stream = TcpStream::connect(&args.addr)
                    .await
                    .expect("failed to connect to host");
                eprintln!("[helper] Connected!");
                run_plugin_stream(stream).await;
            } else {
                // Unix socket
                #[cfg(unix)]
                {
                    use tokio::net::UnixStream;
                    eprintln!("[helper] Connecting to Unix socket: {}", args.addr);
                    let stream = UnixStream::connect(&args.addr)
                        .await
                        .expect("failed to connect to host");
                    eprintln!("[helper] Connected!");
                    run_plugin_stream(stream).await;
                }
                #[cfg(not(unix))]
                {
                    panic!("Unix sockets not supported on this platform");
                }
            }
        }
        TransportType::Shm => {
            // Wait a bit for the host to create the SHM file
            for i in 0..50 {
                if std::path::Path::new(&args.addr).exists() {
                    break;
                }
                if i == 49 {
                    panic!("SHM file not created by host: {}", args.addr);
                }
                tokio::time::sleep(std::time::Duration::from_millis(100)).await;
            }

            eprintln!("[helper] Opening SHM file: {}", args.addr);
            let session = ShmSession::open_file(&args.addr, ShmSessionConfig::default())
                .expect("failed to open SHM file");
            let transport = Arc::new(ShmTransport::new(session));
            eprintln!("[helper] SHM mapped!");
            run_plugin_shm(transport).await;
        }
    }

    eprintln!("[helper] Exiting");
}
