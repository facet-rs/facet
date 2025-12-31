//! rapace-spec-subject: Real rapace-core implementation for spec testing.
//!
//! This binary uses the actual rapace-core RpcSession and StreamTransport
//! to communicate with the spec-peer via TCP.
//!
//! # Usage
//!
//! ```bash
//! PEER_ADDR=127.0.0.1:9000 rapace-spec-subject --case handshake.valid_hello_exchange
//! ```

use std::sync::Arc;

use clap::Parser;
use rapace_core::RpcSession;
use rapace_core::stream::StreamTransport;
use tokio::net::TcpStream;
use tracing_subscriber::EnvFilter;

#[derive(Parser, Debug)]
#[command(name = "rapace-spec-subject")]
#[command(about = "Real rapace-core implementation for spec testing")]
struct Args {
    /// Test case to run (e.g., "handshake.valid_hello_exchange")
    #[arg(long)]
    case: String,
}

fn main() {
    // Initialize tracing - output goes to stderr, no timestamps (harness adds them)
    tracing_subscriber::fmt()
        .with_env_filter(
            EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info")),
        )
        .with_writer(std::io::stderr)
        .without_time()
        .init();

    let args = Args::parse();

    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .expect("failed to create runtime");

    rt.block_on(run_case(&args.case));
}

async fn run_case(case: &str) {
    tracing::info!(case, "Running conformance test case");

    // Get peer address from environment
    let peer_addr = std::env::var("PEER_ADDR").expect("PEER_ADDR environment variable not set");

    tracing::debug!(peer_addr, "Connecting to harness");

    // Connect to the harness
    let stream = TcpStream::connect(&peer_addr)
        .await
        .expect("failed to connect to harness");

    tracing::debug!("Connected, creating transport");

    // Create transport from TCP stream
    let transport = StreamTransport::new(stream);

    // Create session - use channel start 1 (odd IDs) since we're the "initiator"
    // The spec-peer acts as acceptor in tests
    let session = Arc::new(RpcSession::new(transport));

    // For now, just run the session - it will:
    // 1. Perform Hello handshake
    // 2. Handle incoming frames (Ping -> Pong, etc.)
    // 3. Dispatch requests if we register a dispatcher
    //
    // Most conformance tests just need the session to be running
    // and responding to protocol-level messages.
    //
    // The case parameter will be used for case-specific behavior later
    // (e.g., registering specific service handlers for certain tests).
    let _ = case;

    tracing::info!("Starting RPC session");

    // Run the session until the transport closes
    match session.run().await {
        Ok(()) => {
            tracing::info!("Session completed normally");
        }
        Err(e) => {
            tracing::error!(?e, "Session error");
            std::process::exit(1);
        }
    }
}
