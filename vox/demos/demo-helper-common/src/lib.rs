//! Common helper infrastructure for demo cells/plugins.
//!
//! This crate provides shared transport setup code for demo helpers,
//! eliminating duplication across diagnostics, template-engine, http-over-rapace,
//! and tracing-over-rapace demos.
//!
//! # Usage
//!
//! ```ignore
//! use demo_helper_common::run_helper;
//!
//! #[tokio::main]
//! async fn main() {
//!     run_helper("my-demo", |transport| async move {
//!         // Your service-specific setup
//!         let session = Arc::new(RpcSession::with_channel_start(transport, 2));
//!         session.set_dispatcher(my_dispatcher);
//!         session.run().await
//!     }).await;
//! }
//! ```
//!
//! # Supported Transports
//!
//! ## Stream Transport
//!
//! ```bash
//! # TCP
//! my-helper --transport=stream --addr=127.0.0.1:12345
//!
//! # Unix socket
//! my-helper --transport=stream --addr=/tmp/my.sock
//!
//! # Inherited FD (via RAPACE_STREAM_CONTROL_FD env var)
//! my-helper --transport=stream
//! ```
//!
//! ## Hub SHM Transport
//!
//! ```bash
//! my-helper --transport=shm --hub-path=/tmp/hub.shm --peer-id=0 --doorbell-fd=5
//! ```

use std::future::Future;
use std::sync::Arc;
use std::time::Duration;

use rapace::AnyTransport;

#[cfg(unix)]
use rapace::transport::shm::{Doorbell, HubPeer};
#[cfg(unix)]
use std::os::unix::io::RawFd;

use tokio::net::TcpStream;

/// Transport type parsed from CLI args.
#[derive(Debug, Clone)]
pub enum TransportType {
    Stream,
    Shm,
}

/// Parsed CLI arguments for helper processes.
#[derive(Debug)]
pub struct HelperArgs {
    pub transport: TransportType,
    /// For stream: TCP address or Unix socket path
    pub addr: Option<String>,
    /// For SHM hub: path to hub file
    pub hub_path: Option<String>,
    /// For SHM hub: peer ID assigned by host
    pub peer_id: Option<u16>,
    /// For SHM hub: doorbell file descriptor
    #[cfg(unix)]
    pub doorbell_fd: Option<RawFd>,
}

impl HelperArgs {
    /// Parse CLI arguments.
    pub fn parse() -> Self {
        let args: Vec<String> = std::env::args().collect();

        let mut transport = None;
        let mut addr = None;
        let mut hub_path = None;
        let mut peer_id = None;
        #[cfg(unix)]
        let mut doorbell_fd = None;

        let mut i = 1;
        while i < args.len() {
            if let Some(t) = args[i].strip_prefix("--transport=") {
                transport = Some(match t {
                    "stream" => TransportType::Stream,
                    "shm" => TransportType::Shm,
                    _ => panic!("unknown transport: {}", t),
                });
            } else if let Some(a) = args[i].strip_prefix("--addr=") {
                addr = Some(a.to_string());
            } else if let Some(p) = args[i].strip_prefix("--hub-path=") {
                hub_path = Some(p.to_string());
            } else if let Some(p) = args[i].strip_prefix("--peer-id=") {
                peer_id = p.parse().ok();
            } else if let Some(fd) = args[i].strip_prefix("--doorbell-fd=") {
                #[cfg(unix)]
                {
                    doorbell_fd = fd.parse().ok();
                }
                #[cfg(not(unix))]
                {
                    let _ = fd;
                }
            }
            i += 1;
        }

        Self {
            transport: transport.expect("--transport required"),
            addr,
            hub_path,
            peer_id,
            #[cfg(unix)]
            doorbell_fd,
        }
    }
}

/// Environment variable for inherited stream control FD.
#[cfg(unix)]
const STREAM_CONTROL_ENV: &str = "RAPACE_STREAM_CONTROL_FD";

/// Accept a stream from an inherited TCP listener (via FD passing).
#[cfg(unix)]
pub async fn accept_inherited_stream() -> Option<TcpStream> {
    use async_send_fd::AsyncRecvFd;
    use std::os::unix::io::FromRawFd;
    use std::os::unix::net::UnixStream as StdUnixStream;
    use tokio::io::AsyncWriteExt;
    use tokio::net::{TcpListener, UnixStream};

    let fd_str = std::env::var(STREAM_CONTROL_ENV).ok()?;
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

    let (stream, _peer) = listener
        .accept()
        .await
        .expect("failed to accept inherited stream");
    Some(stream)
}

#[cfg(not(unix))]
pub async fn accept_inherited_stream() -> Option<TcpStream> {
    None
}

/// Connect to a stream transport (TCP or Unix socket).
pub async fn connect_stream(addr: &str) -> AnyTransport {
    if addr.contains(':') {
        // TCP
        let stream = TcpStream::connect(addr)
            .await
            .expect("failed to connect to TCP address");
        AnyTransport::stream(stream)
    } else {
        // Unix socket
        #[cfg(unix)]
        {
            use tokio::net::UnixStream;
            let stream = UnixStream::connect(addr)
                .await
                .expect("failed to connect to Unix socket");
            AnyTransport::stream(stream)
        }
        #[cfg(not(unix))]
        {
            panic!("Unix sockets not supported on this platform");
        }
    }
}

/// Connect to a hub SHM transport.
#[cfg(unix)]
pub async fn connect_hub(hub_path: &str, peer_id: u16, doorbell_fd: RawFd) -> AnyTransport {
    // Wait for hub file to exist
    for i in 0..50 {
        if std::path::Path::new(hub_path).exists() {
            break;
        }
        if i == 49 {
            panic!("Hub file not created by host: {}", hub_path);
        }
        tokio::time::sleep(Duration::from_millis(100)).await;
    }

    let peer = HubPeer::open(hub_path, peer_id).expect("failed to open hub peer");
    peer.register();

    let doorbell = Doorbell::from_raw_fd(doorbell_fd).expect("failed to create doorbell");

    AnyTransport::shm_hub_peer(Arc::new(peer), doorbell, "helper")
}

/// Run a helper with the given service setup function.
///
/// This handles all transport setup boilerplate. The `service_fn` receives
/// the connected transport and should set up the RPC session and run it.
///
/// # Arguments
///
/// * `name` - Helper name for logging (e.g., "diagnostics-plugin")
/// * `service_fn` - Async function that receives transport and runs the service
pub async fn run_helper<F, Fut>(name: &str, service_fn: F)
where
    F: FnOnce(AnyTransport) -> Fut,
    Fut: Future<Output = ()>,
{
    let args = HelperArgs::parse();

    eprintln!("[{}] Starting with transport={:?}", name, args.transport);

    let transport = match args.transport {
        TransportType::Stream => {
            // Try inherited stream first
            if let Some(stream) = accept_inherited_stream().await {
                eprintln!("[{}] Using inherited TCP listener", name);
                AnyTransport::stream(stream)
            } else {
                let addr = args.addr.expect("--addr required for stream transport");
                eprintln!("[{}] Connecting to: {}", name, addr);
                let transport = connect_stream(&addr).await;
                eprintln!("[{}] Connected!", name);
                transport
            }
        }
        TransportType::Shm => {
            #[cfg(unix)]
            {
                let hub_path = args
                    .hub_path
                    .expect("--hub-path required for shm transport");
                let peer_id = args.peer_id.expect("--peer-id required for shm transport");
                let doorbell_fd = args
                    .doorbell_fd
                    .expect("--doorbell-fd required for shm transport");
                eprintln!(
                    "[{}] Connecting to hub: {} (peer_id={}, doorbell_fd={})",
                    name, hub_path, peer_id, doorbell_fd
                );
                let transport = connect_hub(&hub_path, peer_id, doorbell_fd).await;
                eprintln!("[{}] Hub connected!", name);
                transport
            }
            #[cfg(not(unix))]
            {
                panic!("SHM transport not supported on this platform");
            }
        }
    };

    service_fn(transport).await;

    eprintln!("[{}] Exiting", name);
}
