//! Host-side tracing receiver.
//!
//! Implements `HostTracing` service to receive tracing records from cells.

use std::sync::{Arc, RwLock};

use tokio::sync::mpsc;

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
    record_tx: mpsc::Sender<TaggedRecord>,
    /// Receiver end (taken by consumer).
    record_rx: std::sync::Mutex<Option<mpsc::Receiver<TaggedRecord>>>,
    /// Current tracing configuration (shared across all cells).
    config: RwLock<TracingConfig>,
}

impl HostTracingState {
    /// Create new host tracing state.
    ///
    /// `buffer_size` is the capacity of the record channel.
    /// If the consumer is slow, newest records are dropped.
    pub fn new(buffer_size: usize) -> Arc<Self> {
        let (record_tx, record_rx) = mpsc::channel(buffer_size);
        Arc::new(Self {
            record_tx,
            record_rx: std::sync::Mutex::new(Some(record_rx)),
            config: RwLock::new(TracingConfig::default()),
        })
    }

    /// Take the record receiver.
    ///
    /// Call this once to get the stream of tagged records from all cells.
    /// Returns `None` if already taken.
    pub fn take_receiver(&self) -> Option<mpsc::Receiver<TaggedRecord>> {
        self.record_rx.lock().unwrap().take()
    }

    /// Set the tracing configuration.
    ///
    /// This affects what `get_tracing_config()` returns to cells.
    /// Existing cells won't see this until you call `CellTracingClient::configure()`
    /// on their handles.
    pub fn set_config(&self, config: TracingConfig) {
        *self.config.write().unwrap() = config;
    }

    /// Get the current tracing configuration.
    pub fn config(&self) -> TracingConfig {
        self.config.read().unwrap().clone()
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
    async fn get_tracing_config(&self) -> TracingConfig {
        self.state.config()
    }

    async fn emit_tracing(&self, records: Vec<TracingRecord>) {
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

    #[tokio::test]
    async fn test_host_tracing_service() {
        let state = HostTracingState::new(100);
        let mut rx = state.take_receiver().unwrap();

        let service = state.service_for_peer(1, Some("test-cell".to_string()));

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

        service.emit_tracing(records).await;

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

        // Default config
        let config = service.get_tracing_config().await;
        assert_eq!(config.min_level, Level::Info);

        // Update config
        state.set_config(TracingConfig {
            min_level: Level::Debug,
            filters: vec!["mymodule".to_string()],
            include_span_events: true,
        });

        // Query again
        let config = service.get_tracing_config().await;
        assert_eq!(config.min_level, Level::Debug);
        assert_eq!(config.filters, vec!["mymodule".to_string()]);
        assert!(config.include_span_events);
    }
}
