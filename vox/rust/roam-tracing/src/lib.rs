//! Cross-cell tracing for roam RPC framework.
//!
//! This crate provides tracing infrastructure for cells (sandboxed processes)
//! to emit tracing events/spans that the host can collect over roam streams.
//!
//! # Architecture
//!
//! ```text
//! ┌─────────────────────────────────────────────────────────────┐
//! │  HOST                                                       │
//! │  ┌─────────────┐    ┌─────────────┐    ┌─────────────┐     │
//! │  │ TracingHost │◄───│  Rx<Record> │◄───│  Rx<Record> │     │
//! │  │             │    │  (cell A)   │    │  (cell B)   │     │
//! │  └─────────────┘    └─────────────┘    └─────────────┘     │
//! │         │                  ▲                  ▲             │
//! │         ▼                  │                  │             │
//! │  ┌──────────────────────────────────────────────────────┐  │
//! │  │      TaggedRecord stream → subscriber/TUI/logs       │  │
//! │  └──────────────────────────────────────────────────────┘  │
//! └─────────────────────────────────────────────────────────────┘
//!                              │roam RPC (stream/SHM)
//! ┌─────────────────────────────────────────────────────────────┐
//! │  CELL                                                       │
//! │  ┌─────────────────┐    ┌──────────────┐    ┌───────────┐  │
//! │  │ CellTracingLayer│───►│ LossyBuffer  │───►│ Tx<Record>│  │
//! │  │   (Layer<S>)    │    │  (bounded)   │    │           │  │
//! │  └─────────────────┘    └──────────────┘    └───────────┘  │
//! │         ▲                                                   │
//! │  ┌──────┴──────┐                                           │
//! │  │ tracing::{  │                                           │
//! │  │  info!(),   │                                           │
//! │  │  #[instrument]                                          │
//! │  │ }           │                                           │
//! │  └─────────────┘                                           │
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
//! // Register the service with your dispatcher
//! let tracing_dispatcher = CellTracingDispatcher::new(service);
//! // ... combine with your other dispatchers
//! ```
//!
//! # Host-side Usage
//!
//! ```ignore
//! use roam_tracing::TracingHost;
//!
//! // Create the host collector
//! let mut tracing_host = TracingHost::new(4096);
//!
//! // Take the receiver (do this once)
//! let mut records = tracing_host.take_receiver().unwrap();
//!
//! // When spawning a cell:
//! tracing_host.register_peer(peer_id, Some("my-cell".into()), &handle).await?;
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
mod host;
mod record;
mod service;

// Re-export record types
pub use record::{FieldValue, Level, SpanId, TracingRecord};

// Re-export service types
pub use service::{
    CellTracing, CellTracingClient, CellTracingDispatcher, ConfigResult, TracingConfig,
    cell_tracing_service_detail,
};

// Re-export cell-side types
pub use cell::{CellTracingLayer, CellTracingService};

// Re-export host-side types
pub use host::{ConfigureError, PeerId, RegisterError, TaggedRecord, TracingHost};

/// Initialize cell-side tracing.
///
/// Call this early in the cell's main function to set up tracing forwarding.
///
/// Returns a tuple of:
/// - `CellTracingLayer`: Install this as a layer in your tracing subscriber
/// - `CellTracingService`: Register this with your cell's dispatcher
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
/// let (layer, service) = init_cell_tracing(1024);
///
/// tracing_subscriber::registry()
///     .with(layer)
///     .init();
///
/// // Create dispatcher for the tracing service
/// let tracing_dispatcher = CellTracingDispatcher::new(service);
/// ```
pub fn init_cell_tracing(buffer_size: usize) -> (CellTracingLayer, CellTracingService) {
    let layer = CellTracingLayer::new(buffer_size);
    let service = layer.service_handle();
    (layer, service)
}

#[cfg(test)]
mod tests {
    use super::*;
    use tracing_subscriber::prelude::*;

    #[test]
    fn test_layer_captures_events() {
        // Create the layer and service
        let (layer, _service) = init_cell_tracing(100);

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
        {
            let mut config = layer.config.write().unwrap();
            config.include_span_events = true;
            config.min_level = Level::Trace;
        }

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
        {
            let mut config = layer.config.write().unwrap();
            config.min_level = Level::Warn; // Only warn and error
        }

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
