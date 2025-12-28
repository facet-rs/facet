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
//! # For SHM hub transport
//! template-engine-helper --transport=shm --hub-path=/tmp/hub.shm --peer-id=0 --doorbell-fd=5
//! ```

use std::sync::Arc;

use rapace::RpcSession;
use rapace_template_engine::create_template_engine_dispatcher;

fn main() {
    let rt = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
        .unwrap();

    rt.block_on(demo_helper_common::run_helper(
        "helper",
        |transport| async move {
            // Plugin uses even channel IDs (2, 4, 6, ...)
            let session = Arc::new(RpcSession::with_channel_start(transport, 2));

            // Set up TemplateEngine dispatcher
            session.set_dispatcher(create_template_engine_dispatcher(
                session.clone(),
                session.buffer_pool().clone(),
            ));

            // Run the session until the transport closes
            eprintln!("[helper] Plugin session running...");
            if let Err(e) = session.run().await {
                eprintln!("[helper] Session ended with error: {:?}", e);
            } else {
                eprintln!("[helper] Session ended normally");
            }
        },
    ));
}
