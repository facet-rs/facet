//! rapace-spec-subject: Real rapace-core implementation for spec testing.
//!
//! This binary uses the actual rapace-core RpcSession and StreamTransport
//! to communicate with the spec-tester via stdin/stdout.
//!
//! # Usage
//!
//! ```bash
//! rapace-spec-subject --case handshake.valid_hello_exchange
//! ```

use std::sync::Arc;

use clap::Parser;
use rapace_core::RpcSession;
use rapace_core::stream::StreamTransport;

#[derive(Parser, Debug)]
#[command(name = "rapace-spec-subject")]
#[command(about = "Real rapace-core implementation for spec testing")]
struct Args {
    /// Test case to run (e.g., "handshake.valid_hello_exchange")
    #[arg(long)]
    case: String,
}

fn main() {
    let args = Args::parse();

    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .expect("failed to create runtime");

    rt.block_on(run_case(&args.case));
}

async fn run_case(case: &str) {
    eprintln!("[spec-subject] Running case: {}", case);

    // Create transport from stdin/stdout
    let transport = StreamTransport::from_stdio();

    // Create session - use channel start 1 (odd IDs) since we're the "initiator"
    // The spec-tester acts as acceptor in tests
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

    eprintln!("[spec-subject] Starting session...");

    // Run the session until the transport closes
    match session.run().await {
        Ok(()) => {
            eprintln!("[spec-subject] Session completed normally");
        }
        Err(e) => {
            eprintln!("[spec-subject] Session error: {:?}", e);
            std::process::exit(1);
        }
    }
}
