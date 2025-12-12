//! Tracing over Rapace - Demo Binary
//!
//! This example demonstrates tracing forwarding where:
//! - The **plugin** uses tracing normally (tracing::info!, spans, etc.)
//! - The **host** receives all spans/events via RPC and collects them
//!
//! This pattern allows centralized logging in the host while plugins
//! use standard tracing APIs.

use std::sync::Arc;
use std::time::Duration;

use rapace::{InProcTransport, RpcSession, Transport};
use tracing_subscriber::layer::SubscriberExt;

use rapace_tracing_over_rapace::{
    HostTracingSink, RapaceTracingLayer, TraceRecord, create_tracing_sink_dispatcher,
};

#[tokio::main]
async fn main() {
    println!("=== Tracing over Rapace Demo ===\n");

    // Create a transport pair (in-memory for demo)
    let (host_transport, plugin_transport) = InProcTransport::pair();
    let host_transport = Arc::new(host_transport);
    let plugin_transport = Arc::new(plugin_transport);

    // ========== HOST SIDE ==========
    // Create the tracing sink that will collect all traces
    let tracing_sink = HostTracingSink::new();

    // Create RpcSession for the host (uses odd channel IDs: 1, 3, 5, ...)
    let host_session = Arc::new(RpcSession::with_channel_start(host_transport.clone(), 1));

    // Set dispatcher for TracingSink service
    host_session.set_dispatcher(create_tracing_sink_dispatcher(tracing_sink.clone()));

    // Spawn the host's demux loop
    let host_session_clone = host_session.clone();
    let _host_handle = tokio::spawn(async move { host_session_clone.run().await });

    // ========== PLUGIN SIDE ==========
    // Create RpcSession for the plugin (uses even channel IDs: 2, 4, 6, ...)
    let plugin_session = Arc::new(RpcSession::with_channel_start(plugin_transport.clone(), 2));

    // Spawn the plugin's demux loop
    let plugin_session_clone = plugin_session.clone();
    let _plugin_handle = tokio::spawn(async move { plugin_session_clone.run().await });

    // Create the tracing layer that forwards to host
    let (layer, _shared_filter) =
        RapaceTracingLayer::new(plugin_session.clone(), tokio::runtime::Handle::current());

    // Install the layer (in a real app, this would be done at startup)
    // For this demo, we use a scoped subscriber
    let subscriber = tracing_subscriber::registry().with(layer);

    // ========== EMIT SOME TRACES ==========
    println!("--- Emitting traces from plugin side ---\n");

    tracing::subscriber::with_default(subscriber, || {
        // Simple event
        tracing::info!("Hello from the plugin!");

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
    let _ = host_transport.close().await;
    let _ = plugin_transport.close().await;

    println!("\n=== Demo Complete ===");
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    /// Helper to run tracing scenario with RpcSession
    async fn run_scenario<T: Transport + Send + Sync + 'static>(
        host_transport: Arc<T>,
        plugin_transport: Arc<T>,
    ) -> Vec<TraceRecord> {
        // Host side
        let tracing_sink = HostTracingSink::new();
        let host_session = Arc::new(RpcSession::with_channel_start(host_transport.clone(), 1));
        host_session.set_dispatcher(create_tracing_sink_dispatcher(tracing_sink.clone()));
        let host_session_clone = host_session.clone();
        let host_handle = tokio::spawn(async move { host_session_clone.run().await });

        // Plugin side
        let plugin_session = Arc::new(RpcSession::with_channel_start(plugin_transport.clone(), 2));
        let plugin_session_clone = plugin_session.clone();
        let plugin_handle = tokio::spawn(async move { plugin_session_clone.run().await });

        // Let the demux loops start
        tokio::task::yield_now().await;

        // Create layer
        let (layer, _shared_filter) =
            RapaceTracingLayer::new(plugin_session.clone(), tokio::runtime::Handle::current());
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
        let _ = host_transport.close().await;
        let _ = plugin_transport.close().await;
        host_handle.abort();
        plugin_handle.abort();

        tracing_sink.records()
    }

    #[tokio::test]
    async fn test_inproc_transport() {
        let (host_transport, plugin_transport) = InProcTransport::pair();
        let records = run_scenario(Arc::new(host_transport), Arc::new(plugin_transport)).await;

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

    #[tokio::test]
    async fn test_stream_transport_tcp() {
        use rapace::StreamTransport;
        use tokio::io::{ReadHalf, WriteHalf};
        use tokio::net::{TcpListener, TcpStream};

        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();

        let accept_task = tokio::spawn(async move {
            let (stream, _) = listener.accept().await.unwrap();
            let transport: StreamTransport<ReadHalf<TcpStream>, WriteHalf<TcpStream>> =
                StreamTransport::new(stream);
            Arc::new(transport)
        });

        let stream = TcpStream::connect(addr).await.unwrap();
        let host_transport: Arc<StreamTransport<ReadHalf<TcpStream>, WriteHalf<TcpStream>>> =
            Arc::new(StreamTransport::new(stream));

        let plugin_transport = accept_task.await.unwrap();

        let records = run_scenario(host_transport, plugin_transport).await;
        assert!(!records.is_empty());
    }

    #[tokio::test]
    async fn test_span_lifecycle() {
        let (host_transport, plugin_transport) = InProcTransport::pair();
        let host_transport = Arc::new(host_transport);
        let plugin_transport = Arc::new(plugin_transport);

        let tracing_sink = HostTracingSink::new();
        let host_session = Arc::new(RpcSession::with_channel_start(host_transport.clone(), 1));
        host_session.set_dispatcher(create_tracing_sink_dispatcher(tracing_sink.clone()));
        let host_session_clone = host_session.clone();
        let host_handle = tokio::spawn(async move { host_session_clone.run().await });

        let plugin_session = Arc::new(RpcSession::with_channel_start(plugin_transport.clone(), 2));
        let plugin_session_clone = plugin_session.clone();
        let plugin_handle = tokio::spawn(async move { plugin_session_clone.run().await });

        let (layer, _shared_filter) =
            RapaceTracingLayer::new(plugin_session.clone(), tokio::runtime::Handle::current());
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

        let _ = host_transport.close().await;
        let _ = plugin_transport.close().await;
        host_handle.abort();
        plugin_handle.abort();

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

    #[tokio::test]
    async fn test_event_with_fields() {
        let (host_transport, plugin_transport) = InProcTransport::pair();
        let host_transport = Arc::new(host_transport);
        let plugin_transport = Arc::new(plugin_transport);

        let tracing_sink = HostTracingSink::new();
        let host_session = Arc::new(RpcSession::with_channel_start(host_transport.clone(), 1));
        host_session.set_dispatcher(create_tracing_sink_dispatcher(tracing_sink.clone()));
        let host_session_clone = host_session.clone();
        let host_handle = tokio::spawn(async move { host_session_clone.run().await });

        let plugin_session = Arc::new(RpcSession::with_channel_start(plugin_transport.clone(), 2));
        let plugin_session_clone = plugin_session.clone();
        let plugin_handle = tokio::spawn(async move { plugin_session_clone.run().await });

        let (layer, _shared_filter) =
            RapaceTracingLayer::new(plugin_session.clone(), tokio::runtime::Handle::current());
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

        let _ = host_transport.close().await;
        let _ = plugin_transport.close().await;
        host_handle.abort();
        plugin_handle.abort();

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
    #[tokio::test]
    async fn test_shm_transport() {
        use rapace::transport::shm::{ShmSession, ShmSessionConfig, ShmTransport};

        let shm_path = format!("/tmp/rapace-tracing-test-{}.shm", std::process::id());
        let _ = std::fs::remove_file(&shm_path);

        let host_session = ShmSession::create_file(&shm_path, ShmSessionConfig::default())
            .expect("Failed to create SHM");
        let host_transport = Arc::new(ShmTransport::new(host_session));

        let plugin_session = ShmSession::open_file(&shm_path, ShmSessionConfig::default())
            .expect("Failed to open SHM");
        let plugin_transport = Arc::new(ShmTransport::new(plugin_session));

        let records = run_scenario(host_transport, plugin_transport).await;

        let _ = std::fs::remove_file(&shm_path);

        assert!(!records.is_empty());
    }
}
