//! Tracing Plugin Helper Binary
//!
//! This binary acts as the "plugin" side for cross-process testing.
//! It connects to the host via the specified transport, sets up the
//! RapaceTracingLayer, emits some traces, then exits.
//!
//! # Usage
//!
//! ```bash
//! # For stream transport (TCP)
//! tracing-plugin-helper --transport=stream --addr=127.0.0.1:12345
//!
//! # For stream transport (Unix socket)
//! tracing-plugin-helper --transport=stream --addr=/tmp/rapace-tracing.sock
//!
//! # For SHM hub transport
//! tracing-plugin-helper --transport=shm --hub-path=/tmp/hub.shm --peer-id=0 --doorbell-fd=5
//! ```

use std::sync::Arc;
use std::time::Duration;

use rapace::RpcSession;
use tracing_subscriber::layer::SubscriberExt;

use rapace_tracing_over_rapace::RapaceTracingLayer;

fn main() {
    let rt = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
        .unwrap();

    rt.block_on(demo_helper_common::run_helper(
        "tracing-cell",
        |transport| async move {
            // Cell uses even channel IDs (2, 4, 6, ...)
            let session = Arc::new(RpcSession::with_channel_start(transport, 2));

            // Spawn the session runner
            let session_clone = session.clone();
            let _session_handle = tokio::spawn(async move { session_clone.run().await });

            // Create the tracing layer
            let (layer, shared_filter) =
                RapaceTracingLayer::new(session.clone(), tokio::runtime::Handle::current());
            // Set filter to allow all levels (default is "warn")
            shared_filter.set_filter("trace");

            // Use a scoped subscriber for the traces
            let subscriber = tracing_subscriber::registry().with(layer);

            eprintln!("[tracing-cell] Emitting traces...");

            // Emit a fixed pattern of traces that the host can verify
            tracing::subscriber::with_default(subscriber, || {
                // Simple event
                tracing::info!("cell started");

                // Span with nested content
                let outer = tracing::info_span!("outer_span", request_id = 123);
                {
                    let _outer_guard = outer.enter();
                    tracing::debug!("inside outer span");

                    let inner = tracing::debug_span!("inner_span", key = "value");
                    {
                        let _inner_guard = inner.enter();
                        tracing::trace!("inside inner span");
                    }
                }

                // Event with multiple fields
                tracing::warn!(
                    user = "test_user",
                    action = "test_action",
                    count = 42,
                    "final event"
                );
            });

            // Give time for async RPC calls to complete
            // SHM needs more time due to polling nature and we spawn many async tasks
            // that need to be scheduled and complete
            tokio::time::sleep(Duration::from_millis(1000)).await;

            eprintln!("[tracing-cell] Done emitting traces");

            // Close session
            session.close();
        },
    ));
}
