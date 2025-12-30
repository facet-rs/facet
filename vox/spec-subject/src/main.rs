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

use clap::Parser;

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
    // TODO: Implement real rapace-core session
    //
    // 1. Create StreamTransport from stdin/stdout
    // 2. Create RpcSession with transport
    // 3. Register services from spec-proto
    // 4. Based on case name, either:
    //    - Just run session (Hello + respond)
    //    - Make specific calls
    //    - Other case-specific behavior
    //
    // For now, most tests should just need:
    //   session.run().await
    //
    // Which SHOULD do Hello handshake + respond to requests.
    // (Currently it doesn't - that's what we're fixing!)

    eprintln!("[spec-subject] Running case: {}", case);
    eprintln!("[spec-subject] TODO: Implement real rapace-core session");

    // Default behavior: do Hello and respond to whatever comes
    // Most tests: just run session
    // let transport = StreamTransport::from_stdio();
    // let session = RpcSession::new(transport);
    // session.run().await;
    let _ = case; // Will be used for case-specific behavior later
    eprintln!("[spec-subject] Default case - would run session");
}
