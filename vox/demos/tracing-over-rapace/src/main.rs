//! Tracing over Rapace - Demo Binary
//!
//! This example demonstrates tracing forwarding where:
//! - The **cell** uses tracing normally (tracing::info!, spans, etc.)
//! - The **host** receives all spans/events via RPC and collects them
//!
//! This pattern allows centralized logging in the host while cells
//! use standard tracing APIs.

use std::sync::Arc;
use std::time::Duration;

use rapace::{AnyTransport, RpcSession};
use tracing_subscriber::layer::SubscriberExt;

use rapace_tracing_over_rapace::{
    HostTracingSink, RapaceTracingLayer, TraceRecord, create_tracing_sink_dispatcher,
};

fn main() {
    let rt = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
        .unwrap();
    rt.block_on(async_main());
}

async fn async_main() {
    println!("=== Tracing over Rapace Demo ===\n");

    // Create a transport pair (in-memory for demo)
    let (host_transport, cell_transport) = AnyTransport::mem_pair();

    // ========== HOST SIDE ==========
    // Create the tracing sink that will collect all traces
    let tracing_sink = HostTracingSink::new();

    // Create RpcSession for the host (uses odd channel IDs: 1, 3, 5, ...)
    let host_session = Arc::new(RpcSession::with_channel_start(host_transport, 1));

    // Set dispatcher for TracingSink service
    host_session.set_dispatcher(create_tracing_sink_dispatcher(
        tracing_sink.clone(),
        host_session.buffer_pool().clone(),
    ));

    // Spawn the host's demux loop
    let host_session_clone = host_session.clone();
    let _host_handle = tokio::spawn(async move { host_session_clone.run().await });

    // ========== CELL SIDE ==========
    // Create RpcSession for the cell (uses even channel IDs: 2, 4, 6, ...)
    let cell_session = Arc::new(RpcSession::with_channel_start(cell_transport, 2));

    // Spawn the cell's demux loop
    let cell_session_clone = cell_session.clone();
    let _cell_handle = tokio::spawn(async move { cell_session_clone.run().await });

    // Create the tracing layer that forwards to host
    let (layer, _shared_filter) =
        RapaceTracingLayer::new(cell_session.clone(), tokio::runtime::Handle::current());

    // Install the layer (in a real app, this would be done at startup)
    // For this demo, we use a scoped subscriber
    let subscriber = tracing_subscriber::registry().with(layer);

    // ========== EMIT SOME TRACES ==========
    println!("--- Emitting traces from cell side ---\n");

    tracing::subscriber::with_default(subscriber, || {
        // Simple event
        tracing::info!("Hello from the cell!");

        // Event with fields
        tracing::warn!(user = "alice", action = "login", "User action occurred");

        // Span with events inside
        let span = tracing::info_span!("processing", request_id = 42);
        let _guard = span.enter();

        tracing::debug!("Starting processing");
        tracing::info!("Processing complete");

        // Nested span
        {
            let inner_span = tracing::debug_span!("database_query", table = "users");
            let _inner_guard = inner_span.enter();
            tracing::trace!("Executing query");
        }
    });

    // Give async tasks time to complete
    tokio::time::sleep(Duration::from_millis(100)).await;

    // ========== SHOW COLLECTED TRACES ==========
    println!("\n--- Traces collected by host ---\n");

    for record in tracing_sink.records() {
        match record {
            TraceRecord::NewSpan { id, meta } => {
                println!(
                    "NEW_SPAN[{}]: {} (target={}, level={})",
                    id, meta.name, meta.target, meta.level
                );
                if !meta.fields.is_empty() {
                    for field in &meta.fields {
                        println!("  {} = {}", field.name, field.value);
                    }
                }
            }
            TraceRecord::Enter { span_id } => {
                println!("ENTER[{}]", span_id);
            }
            TraceRecord::Exit { span_id } => {
                println!("EXIT[{}]", span_id);
            }
            TraceRecord::DropSpan { span_id } => {
                println!("DROP_SPAN[{}]", span_id);
            }
            TraceRecord::Event(event) => {
                println!(
                    "EVENT: {} (target={}, level={})",
                    event.message, event.target, event.level
                );
                if let Some(parent) = event.parent_span_id {
                    println!("  parent_span: {}", parent);
                }
                for field in &event.fields {
                    if field.name != "message" {
                        println!("  {} = {}", field.name, field.value);
                    }
                }
            }
            TraceRecord::Record { span_id, fields } => {
                println!("RECORD[{}]:", span_id);
                for field in &fields {
                    println!("  {} = {}", field.name, field.value);
                }
            }
        }
    }

    // Clean up
    host_session.close();
    cell_session.close();

    println!("\n=== Demo Complete ===");
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    /// Helper to run tracing scenario with RpcSession
    async fn run_scenario(
        host_transport: AnyTransport,
        cell_transport: AnyTransport,
    ) -> Vec<TraceRecord> {
        // Host side
        let tracing_sink = HostTracingSink::new();
        let host_session = Arc::new(RpcSession::with_channel_start(host_transport, 1));
        host_session.set_dispatcher(create_tracing_sink_dispatcher(
            tracing_sink.clone(),
            host_session.buffer_pool().clone(),
        ));
        let host_session_clone = host_session.clone();
        let host_handle = tokio::spawn(async move { host_session_clone.run().await });

        // Cell side
        let cell_session = Arc::new(RpcSession::with_channel_start(cell_transport, 2));
        let cell_session_clone = cell_session.clone();
        let cell_handle = tokio::spawn(async move { cell_session_clone.run().await });

        // Let the demux loops start
        tokio::task::yield_now().await;

        // Create layer
        let (layer, shared_filter) =
            RapaceTracingLayer::new(cell_session.clone(), tokio::runtime::Handle::current());
        // Set filter to allow all levels (default is "warn")
        shared_filter.set_filter("trace");
        let subscriber = tracing_subscriber::registry().with(layer);

        // Emit traces
        tracing::subscriber::with_default(subscriber, || {
            let span = tracing::info_span!("test_span", user = "alice");
            let _guard = span.enter();
            tracing::info!("test event");
        });

        // Wait for async tasks
        tokio::time::sleep(Duration::from_millis(50)).await;

        // Cleanup
        host_session.close();
        cell_session.close();
        host_handle.abort();
        cell_handle.abort();

        tracing_sink.records()
    }

    #[tokio_test_lite::test]
    async fn test_mem_transport() {
        let (host_transport, cell_transport) = AnyTransport::mem_pair();
        let records = run_scenario(host_transport, cell_transport).await;

        // Should have: new_span, enter, event, exit, drop_span
        assert!(!records.is_empty(), "Should have some records");

        // Check we got a span
        let has_span = records
            .iter()
            .any(|r| matches!(r, TraceRecord::NewSpan { meta, .. } if meta.name == "test_span"));
        assert!(has_span, "Should have test_span");

        // Check we got an event
        let has_event = records
            .iter()
            .any(|r| matches!(r, TraceRecord::Event(e) if e.message.contains("test event")));
        assert!(has_event, "Should have test event");
    }

    #[tokio_test_lite::test]
    async fn test_stream_transport_tcp() {
        use rapace::StreamTransport;
        use tokio::net::{TcpListener, TcpStream};

        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();

        let accept_task = tokio::spawn(async move {
            let (stream, _) = listener.accept().await.unwrap();
            AnyTransport::new(StreamTransport::new(stream))
        });

        let stream = TcpStream::connect(addr).await.unwrap();
        let host_transport = AnyTransport::new(StreamTransport::new(stream));

        let cell_transport = accept_task.await.unwrap();

        let records = run_scenario(host_transport, cell_transport).await;
        assert!(!records.is_empty());
    }

    #[tokio_test_lite::test]
    async fn test_span_lifecycle() {
        let (host_transport, cell_transport) = AnyTransport::mem_pair();

        let tracing_sink = HostTracingSink::new();
        let host_session = Arc::new(RpcSession::with_channel_start(host_transport, 1));
        host_session.set_dispatcher(create_tracing_sink_dispatcher(
            tracing_sink.clone(),
            host_session.buffer_pool().clone(),
        ));
        let host_session_clone = host_session.clone();
        let host_handle = tokio::spawn(async move { host_session_clone.run().await });

        let cell_session = Arc::new(RpcSession::with_channel_start(cell_transport, 2));
        let cell_session_clone = cell_session.clone();
        let cell_handle = tokio::spawn(async move { cell_session_clone.run().await });

        let (layer, shared_filter) =
            RapaceTracingLayer::new(cell_session.clone(), tokio::runtime::Handle::current());
        // Set filter to allow all levels (default is "warn")
        shared_filter.set_filter("trace");
        let subscriber = tracing_subscriber::registry().with(layer);

        tracing::subscriber::with_default(subscriber, || {
            let span = tracing::info_span!("lifecycle_test");
            {
                let _guard = span.enter();
                // span is entered here
            }
            // span is exited here
            drop(span);
            // span is dropped here
        });

        tokio::time::sleep(Duration::from_millis(50)).await;

        host_session.close();
        cell_session.close();
        host_handle.abort();
        cell_handle.abort();

        let records = tracing_sink.records();

        // Should see the full lifecycle
        let has_new_span = records
            .iter()
            .any(|r| matches!(r, TraceRecord::NewSpan { .. }));
        let has_enter = records
            .iter()
            .any(|r| matches!(r, TraceRecord::Enter { .. }));
        let has_exit = records
            .iter()
            .any(|r| matches!(r, TraceRecord::Exit { .. }));
        let has_drop = records
            .iter()
            .any(|r| matches!(r, TraceRecord::DropSpan { .. }));

        assert!(has_new_span, "Should have new_span");
        assert!(has_enter, "Should have enter");
        assert!(has_exit, "Should have exit");
        assert!(has_drop, "Should have drop_span");
    }

    #[tokio_test_lite::test]
    async fn test_event_with_fields() {
        let (host_transport, cell_transport) = AnyTransport::mem_pair();

        let tracing_sink = HostTracingSink::new();
        let host_session = Arc::new(RpcSession::with_channel_start(host_transport, 1));
        host_session.set_dispatcher(create_tracing_sink_dispatcher(
            tracing_sink.clone(),
            host_session.buffer_pool().clone(),
        ));
        let host_session_clone = host_session.clone();
        let host_handle = tokio::spawn(async move { host_session_clone.run().await });

        let cell_session = Arc::new(RpcSession::with_channel_start(cell_transport, 2));
        let cell_session_clone = cell_session.clone();
        let cell_handle = tokio::spawn(async move { cell_session_clone.run().await });

        let (layer, shared_filter) =
            RapaceTracingLayer::new(cell_session.clone(), tokio::runtime::Handle::current());
        // Set filter to allow all levels (default is "warn")
        shared_filter.set_filter("trace");
        let subscriber = tracing_subscriber::registry().with(layer);

        tracing::subscriber::with_default(subscriber, || {
            tracing::info!(
                user = "bob",
                count = 42,
                enabled = true,
                "Event with fields"
            );
        });

        tokio::time::sleep(Duration::from_millis(50)).await;

        host_session.close();
        cell_session.close();
        host_handle.abort();
        cell_handle.abort();

        let records = tracing_sink.records();

        // Find the event
        let event = records.iter().find_map(|r| {
            if let TraceRecord::Event(e) = r {
                Some(e)
            } else {
                None
            }
        });

        assert!(event.is_some(), "Should have an event");
        let event = event.unwrap();

        // Check fields
        let has_user = event
            .fields
            .iter()
            .any(|f| f.name == "user" && f.value == "bob");
        let has_count = event
            .fields
            .iter()
            .any(|f| f.name == "count" && f.value == "42");
        let has_enabled = event
            .fields
            .iter()
            .any(|f| f.name == "enabled" && f.value == "true");

        assert!(has_user, "Should have user field");
        assert!(has_count, "Should have count field");
        assert!(has_enabled, "Should have enabled field");
    }

    #[cfg(unix)]
    #[tokio_test_lite::test]
    async fn test_shm_transport() {
        use rapace::transport::shm::ShmTransport;

        let (host_shm, cell_shm) = ShmTransport::hub_pair().expect("Failed to create hub pair");
        let host_transport = AnyTransport::new(host_shm);
        let cell_transport = AnyTransport::new(cell_shm);

        let records = run_scenario(host_transport, cell_transport).await;

        assert!(!records.is_empty());
    }
}
