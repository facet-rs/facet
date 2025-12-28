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
//! # For SHM hub transport
//! http-plugin-helper --transport=shm --hub-path=/tmp/hub.shm --peer-id=0 --doorbell-fd=5
//! ```

use std::sync::Arc;

use rapace::RpcSession;
use rapace_http_over_rapace::{AxumHttpService, create_http_service_dispatcher};

fn main() {
    let rt = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
        .unwrap();

    rt.block_on(demo_helper_common::run_helper(
        "http-cell",
        |transport| async move {
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
        },
    ));
}
