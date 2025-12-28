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
//! # For SHM hub transport
//! diagnostics-plugin-helper --transport=shm --hub-path=/tmp/hub.shm --peer-id=0 --doorbell-fd=5
//! ```

use rapace_diagnostics_over_rapace::{DiagnosticsImpl, DiagnosticsServer};

fn main() {
    // Initialize tracing subscriber
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::from_default_env()
                .add_directive("rapace_core=debug".parse().unwrap())
                .add_directive("rapace_diagnostics_over_rapace=debug".parse().unwrap()),
        )
        .with_writer(std::io::stderr)
        .init();

    let rt = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
        .unwrap();

    rt.block_on(demo_helper_common::run_helper(
        "diagnostics-plugin",
        |transport| async move {
            eprintln!("[diagnostics-plugin] Service ready, waiting for requests...");

            // Use DiagnosticsServer::serve() which handles the frame loop
            let server = DiagnosticsServer::new(DiagnosticsImpl);
            let _ = server.serve(transport).await;

            eprintln!("[diagnostics-plugin] Session ended");
        },
    ));
}
