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

/// Check if the test case requires the subject to make an outgoing call.
///
/// Most conformance tests verify that the subject (as initiator) correctly
/// sends OpenChannel and request frames. Only a few tests (like handshake tests)
/// are passive and just verify the Hello exchange.
fn requires_outgoing_call(case: &str) -> bool {
    // Tests that DON'T require an outgoing call (passive tests)
    let passive_tests = [
        "handshake.",                        // Handshake tests only verify Hello exchange
        "control.",                          // Control tests (ping/pong) work on channel 0
        "frame.desc_",                       // Descriptor tests just verify frame format
        "frame.sentinel_",                   // Sentinel tests verify inline payload handling
        "frame.payload_",                    // Payload tests verify frame structure
        "frame.flags_",                      // Flag tests verify flag handling
        "frame.msg_id_scope",                // msg_id scope test just observes frames
        "channel.control_flag_set",          // Control flag test just observes Hello
        "channel.open_required_before_data", // This one observes behavior
        "flow.credit_semantics",             // Credit semantics just observes CREDITS flag
        "flow.eos_no_credits",               // EOS credits just observes frames
    ];

    for prefix in &passive_tests {
        if case.starts_with(prefix) {
            return false;
        }
    }

    // All other tests require an outgoing call
    true
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

    // Create session as initiator (odd channel IDs)
    let session = Arc::new(RpcSession::new(transport));

    tracing::info!("Starting RPC session");

    // Spawn the session's run loop
    let session_clone = session.clone();
    let run_handle = tokio::spawn(async move {
        match session_clone.run().await {
            Ok(()) => {
                tracing::info!("Session completed normally");
            }
            Err(e) => {
                tracing::error!(?e, "Session error");
            }
        }
    });

    // Wait for handshake to complete
    session.wait_ready().await;
    tracing::debug!("Handshake complete");

    // For tests that require an outgoing call, make a test call
    if requires_outgoing_call(case) {
        tracing::debug!("Making test call for case: {}", case);

        // Use a simple test method ID (any non-zero value works)
        let test_method_id: u32 = 0x12345678;

        // Create a simple test payload
        let test_payload = vec![0x01, 0x02, 0x03, 0x04];

        // Get the next channel ID and make a call
        let channel_id = session.next_channel_id();

        // Make the call - this will:
        // 1. Send OpenChannel control message
        // 2. Send request with DATA|EOS flags
        // 3. Wait for response
        match session.call(channel_id, test_method_id, test_payload).await {
            Ok(response) => {
                tracing::debug!(
                    channel_id,
                    msg_id = response.frame.desc.msg_id,
                    "Test call completed successfully"
                );
            }
            Err(e) => {
                // Many tests don't actually send a response (they just verify
                // the request format), so a timeout or error is expected
                tracing::debug!(?e, "Test call completed (error expected for some tests)");
            }
        }
    }

    // Close the session
    session.close();

    // Wait for the run loop to complete
    let _ = run_handle.await;
}
