//! Host-side tracing receiver.
//!
//! Implements `HostTracing` service to receive tracing records from cells.

use peeps::Mutex;
use std::sync::Arc;

use crate::record::TracingRecord;
use crate::service::{HostTracing, TracingConfig};

/// Identifies a peer/cell.
pub type PeerId = u64;

/// A tracing record tagged with its source peer.
#[derive(Debug, Clone)]
pub struct TaggedRecord {
    /// The peer ID that emitted this record.
    pub peer_id: PeerId,
    /// Optional human-readable peer name.
    pub peer_name: Option<String>,
    /// The tracing record.
    pub record: TracingRecord,
}

/// Shared state for the host tracing service.
///
/// This is the core state that can be shared across multiple `HostTracingService`
/// instances (one per cell connection).
pub struct HostTracingState {
    /// Channel for sending tagged records to consumers.
    record_tx: peeps::Sender<TaggedRecord>,
    /// Receiver end (taken by consumer).
    record_rx: Mutex<Option<peeps::Receiver<TaggedRecord>>>,
    /// Current tracing configuration (shared across all cells).
    config: Mutex<TracingConfig>,
}

impl HostTracingState {
    /// Create new host tracing state.
    ///
    /// `buffer_size` is the capacity of the record channel.
    /// If the consumer is slow, newest records are dropped.
    pub fn new(buffer_size: usize) -> Arc<Self> {
        let (record_tx, record_rx) = peeps::channel("trace_host_records", buffer_size);
        Arc::new(Self {
            record_tx,
            record_rx: Mutex::new("HostTracingState.record_rx", Some(record_rx)),
            config: Mutex::new("HostTracingState.config", TracingConfig::default()),
        })
    }

    /// Take the record receiver.
    ///
    /// Call this once to get the stream of tagged records from all cells.
    /// Returns `None` if already taken.
    pub fn take_receiver(&self) -> Option<peeps::Receiver<TaggedRecord>> {
        self.record_rx.lock().take()
    }

    /// Set the tracing configuration.
    ///
    /// This affects what `get_tracing_config()` returns to cells.
    /// Existing cells won't see this until you call `CellTracingClient::configure()`
    /// on their handles.
    pub fn set_config(&self, config: TracingConfig) {
        *self.config.lock() = config;
    }

    /// Get the current tracing configuration.
    pub fn config(&self) -> TracingConfig {
        self.config.lock().clone()
    }

    /// Create a service instance for a specific peer.
    ///
    /// Each cell connection gets its own `HostTracingService` that tags
    /// records with the peer's identity.
    pub fn service_for_peer(
        self: &Arc<Self>,
        peer_id: PeerId,
        peer_name: Option<String>,
    ) -> HostTracingService {
        HostTracingService {
            state: self.clone(),
            peer_id,
            peer_name,
        }
    }
}

/// Host-side tracing service for a single cell.
///
/// Implements the `HostTracing` trait. Each cell connection gets its own
/// instance, configured with the peer's identity for tagging records.
///
/// Create via `HostTracingState::service_for_peer()`.
#[derive(Clone)]
pub struct HostTracingService {
    state: Arc<HostTracingState>,
    peer_id: PeerId,
    peer_name: Option<String>,
}

impl HostTracing for HostTracingService {
    async fn get_tracing_config(&self, _cx: &roam::Context) -> TracingConfig {
        self.state.config()
    }

    async fn emit_tracing(&self, _cx: &roam::Context, records: Vec<TracingRecord>) {
        for record in records {
            let tagged = TaggedRecord {
                peer_id: self.peer_id,
                peer_name: self.peer_name.clone(),
                record,
            };
            // Non-blocking send - drop if channel is full (backpressure)
            let _ = self.state.record_tx.try_send(tagged);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::record::Level;

    /// Create a dummy context for tests that call trait methods directly
    fn dummy_cx() -> roam::Context {
        roam::Context::new(
            roam::wire::ConnectionId::new(1),
            roam::wire::RequestId::new(1),
            roam::wire::MethodId::new(1),
            roam::wire::Metadata::default(),
            vec![],
        )
    }

    #[tokio::test]
    async fn test_host_tracing_service() {
        let state = HostTracingState::new(100);
        let mut rx = state.take_receiver().unwrap();

        let service = state.service_for_peer(1, Some("test-cell".to_string()));
        let cx = dummy_cx();

        // Emit some records
        let records = vec![
            TracingRecord::Event {
                parent: None,
                target: "test".to_string(),
                level: Level::Info,
                message: Some("hello".to_string()),
                fields: vec![],
                timestamp_ns: 0,
            },
            TracingRecord::Event {
                parent: None,
                target: "test".to_string(),
                level: Level::Warn,
                message: Some("warning".to_string()),
                fields: vec![],
                timestamp_ns: 1000,
            },
        ];

        service.emit_tracing(&cx, records).await;

        // Receive and verify
        let tagged1 = rx.recv().await.unwrap();
        assert_eq!(tagged1.peer_id, 1);
        assert_eq!(tagged1.peer_name, Some("test-cell".to_string()));
        if let TracingRecord::Event { message, .. } = tagged1.record {
            assert_eq!(message, Some("hello".to_string()));
        } else {
            panic!("expected Event");
        }

        let tagged2 = rx.recv().await.unwrap();
        if let TracingRecord::Event { message, .. } = tagged2.record {
            assert_eq!(message, Some("warning".to_string()));
        } else {
            panic!("expected Event");
        }
    }

    #[tokio::test]
    async fn test_config_query() {
        let state = HostTracingState::new(100);
        let service = state.service_for_peer(1, None);
        let cx = dummy_cx();

        // Default config (from RUST_LOG env var or "info")
        let config = service.get_tracing_config(&cx).await;
        // Default should contain some filter directives
        assert!(!config.filter_directives.is_empty() || config.filter_directives == "info");

        // Update config
        state.set_config(TracingConfig {
            filter_directives: "debug,mymodule=trace".to_string(),
            include_span_events: true,
        });

        // Query again
        let config = service.get_tracing_config(&cx).await;
        assert_eq!(config.filter_directives, "debug,mymodule=trace");
        assert!(config.include_span_events);
    }
}
