//! Cross-cell tracing for roam RPC framework.
//!
//! This crate provides tracing infrastructure for cells (sandboxed processes)
//! to emit tracing events/spans that the host can collect via RPC.
//!
//! # Architecture
//!
//! ```text
//! ┌─────────────────────────────────────────────────────────────┐
//! │  HOST                                                       │
//! │  ┌──────────────────┐    ┌──────────────────────────────┐  │
//! │  │ HostTracingState │◄───│  mpsc::Receiver<TaggedRecord>│  │
//! │  │                  │    │  (consumer: TUI/logs/etc.)   │  │
//! │  └──────────────────┘    └──────────────────────────────┘  │
//! │         │                                                   │
//! │         ▼ service_for_peer(id, name)                       │
//! │  ┌──────────────────┐                                      │
//! │  │HostTracingService│  ◄── implements HostTracing trait    │
//! │  │  (one per cell)  │      get_tracing_config()            │
//! │  └──────────────────┘      emit_tracing(records)           │
//! └─────────────────────────────────────────────────────────────┘
//!                              ▲
//!                              │ RPC calls
//!                              │
//! ┌─────────────────────────────────────────────────────────────┐
//! │  CELL                                                       │
//! │  ┌─────────────────┐    ┌──────────────┐    ┌───────────┐  │
//! │  │ CellTracingLayer│───►│ LossyBuffer  │───►│drain task │  │
//! │  │   (Layer<S>)    │    │  (bounded)   │    │(RPC calls)│  │
//! │  └─────────────────┘    └──────────────┘    └───────────┘  │
//! │         ▲                                         │         │
//! │  ┌──────┴──────┐                                  │         │
//! │  │ tracing::{  │                                  ▼         │
//! │  │  info!(),   │                     HostTracingClient      │
//! │  │  #[instrument]                    .emit_tracing(batch)   │
//! │  │ }           │                                            │
//! │  └─────────────┘                                            │
//! └─────────────────────────────────────────────────────────────┘
//! ```
//!
//! # Cell-side Usage
//!
//! ```ignore
//! use roam_tracing::{init_cell_tracing, CellTracingDispatcher};
//! use tracing_subscriber::prelude::*;
//!
//! // Initialize cell-side tracing
//! let (layer, service) = init_cell_tracing(1024);
//!
//! // Set up tracing subscriber with the layer
//! tracing_subscriber::registry()
//!     .with(layer)
//!     .init();
//!
//! // Create dispatcher for the CellTracing service (host can push config)
//! let tracing_dispatcher = CellTracingDispatcher::new(service.clone());
//!
//! // ... establish_guest() ...
//!
//! // Spawn the drain task (after getting handle)
//! service.spawn_drain(handle.clone());
//! ```
//!
//! # Host-side Usage
//!
//! ```ignore
//! use roam_tracing::{HostTracingState, HostTracingDispatcher};
//!
//! // Create shared state for all cells
//! let tracing_state = HostTracingState::new(4096);
//!
//! // Take the receiver (do this once)
//! let mut records = tracing_state.take_receiver().unwrap();
//!
//! // For each cell, create a service and dispatcher:
//! let tracing_service = tracing_state.service_for_peer(peer_id, Some("cell-name".into()));
//! let tracing_dispatcher = HostTracingDispatcher::new(tracing_service);
//!
//! // Compose with your other host services using RoutedDispatcher
//! let combined = RoutedDispatcher::new(tracing_dispatcher, host_service_dispatcher);
//!
//! // Consume records in a separate task
//! tokio::spawn(async move {
//!     while let Some(tagged) = records.recv().await {
//!         println!("[{}] {:?}", tagged.peer_name.unwrap_or_default(), tagged.record);
//!     }
//! });
//! ```

#![deny(unsafe_code)]

mod buffer;
mod cell;
mod dispatch;
mod host;
mod record;
mod service;

// Re-export record types
pub use record::{FieldValue, Level, SpanId, TracingRecord};

// Re-export service types
pub use service::{
    // CellTracing - cell implements, host calls (for config updates)
    CellTracing,
    CellTracingClient,
    CellTracingDispatcher,
    cell_tracing_service_detail,
    // HostTracing - host implements, cell calls (emit records, query config)
    HostTracing,
    HostTracingClient,
    HostTracingDispatcher,
    host_tracing_service_detail,
    // Config types
    ConfigResult,
    TracingConfig,
};

// Re-export cell-side types
pub use cell::{CellTracingGuard, CellTracingLayer, CellTracingService};

// Re-export host-side types
pub use host::{HostTracingService, HostTracingState, PeerId, TaggedRecord};

// Re-export dispatch functionality
pub use dispatch::dispatch_record;

/// Initialize cell-side tracing.
///
/// Call this early in the cell's main function to set up tracing forwarding.
///
/// Returns a tuple of:
/// - `CellTracingLayer`: Install this as a layer in your tracing subscriber
/// - `CellTracingGuard`: **You must call `.start(handle).await`** after establishing
///   the connection. Panics on drop if you forget!
///
/// # Arguments
///
/// * `buffer_size` - Maximum number of records to buffer before dropping oldest
///
/// # Example
///
/// ```ignore
/// use roam_tracing::{init_cell_tracing, CellTracingDispatcher};
/// use tracing_subscriber::prelude::*;
///
/// let (layer, tracing_guard) = init_cell_tracing(1024);
///
/// tracing_subscriber::registry()
///     .with(layer)
///     .init();
///
/// // Create dispatcher using the service from the guard
/// let tracing_dispatcher = CellTracingDispatcher::new(tracing_guard.service());
///
/// // ... after establish_guest() returns handle:
/// // This MUST be called or the guard will panic on drop!
/// tracing_guard.start(handle.clone()).await;
///
/// // Now tracing uses the host's RUST_LOG config
/// tracing::info!("cell started");
/// ```
pub fn init_cell_tracing(buffer_size: usize) -> (CellTracingLayer, CellTracingGuard) {
    let layer = CellTracingLayer::new(buffer_size);
    let service = layer.service_handle();
    (layer, CellTracingGuard::new(service))
}

#[cfg(test)]
mod tests {
    use super::*;
    use tracing_subscriber::prelude::*;

    #[test]
    fn test_layer_captures_events() {
        // Use the real API, defuse guard for unit test (no host to connect to)
        let (layer, guard) = init_cell_tracing(100);
        let _service = guard.defuse();

        // Get access to the buffer for testing
        let buffer = layer.buffer.clone();

        // Set up subscriber with our layer
        let subscriber = tracing_subscriber::registry().with(layer);

        // Use the subscriber for this test
        tracing::subscriber::with_default(subscriber, || {
            tracing::info!("test message");
            tracing::warn!(key = "value", "warning with field");
        });

        // Check that events were captured
        // We should have at least 2 events
        let mut count = 0;
        while let Some(record) = buffer.try_pop() {
            if let TracingRecord::Event { message, level, .. } = record {
                count += 1;
                if count == 1 {
                    assert_eq!(message, Some("test message".to_string()));
                    assert_eq!(level, Level::Info);
                } else if count == 2 {
                    assert_eq!(message, Some("warning with field".to_string()));
                    assert_eq!(level, Level::Warn);
                }
            }
        }
        assert_eq!(count, 2, "expected 2 events");
    }

    #[test]
    fn test_layer_captures_spans() {
        // Create the layer with span events enabled
        let layer = CellTracingLayer::new(100);
        layer.set_config(&TracingConfig {
            filter_directives: "trace".to_string(),
            include_span_events: true,
        });

        let buffer = layer.buffer.clone();

        let subscriber = tracing_subscriber::registry().with(layer);

        tracing::subscriber::with_default(subscriber, || {
            let _span = tracing::info_span!("test_span", foo = 42).entered();
            tracing::info!("inside span");
        });

        // Check that we got span enter, event, and span close
        let mut span_enter = false;
        let mut event_inside = false;
        let mut span_exit = false;
        let mut span_close = false;

        while let Some(record) = buffer.try_pop() {
            match record {
                TracingRecord::SpanEnter { name, .. } => {
                    if name == "test_span" {
                        span_enter = true;
                    }
                }
                TracingRecord::Event { message, .. } => {
                    if message == Some("inside span".to_string()) {
                        event_inside = true;
                    }
                }
                TracingRecord::SpanExit { .. } => {
                    span_exit = true;
                }
                TracingRecord::SpanClose { .. } => {
                    span_close = true;
                }
            }
        }

        assert!(span_enter, "expected SpanEnter");
        assert!(event_inside, "expected Event inside span");
        assert!(span_exit, "expected SpanExit");
        assert!(span_close, "expected SpanClose");
    }

    #[test]
    fn test_level_filtering() {
        let layer = CellTracingLayer::new(100);
        layer.set_config(&TracingConfig {
            filter_directives: "warn".to_string(), // Only warn and error
            include_span_events: false,
        });

        let buffer = layer.buffer.clone();

        let subscriber = tracing_subscriber::registry().with(layer);

        tracing::subscriber::with_default(subscriber, || {
            tracing::debug!("debug message"); // Should be filtered
            tracing::info!("info message"); // Should be filtered
            tracing::warn!("warn message"); // Should pass
            tracing::error!("error message"); // Should pass
        });

        let mut count = 0;
        while let Some(record) = buffer.try_pop() {
            if let TracingRecord::Event { level, .. } = record {
                count += 1;
                assert!(
                    level >= Level::Warn,
                    "expected only warn or error, got {level:?}"
                );
            }
        }
        assert_eq!(count, 2, "expected 2 events (warn and error)");
    }
}
