//! CellTracing service definition.
//!
//! Defines the RPC service for cell-to-host tracing.

use facet::Facet;
use roam::service;
use roam::session::Tx;

use crate::record::{Level, TracingRecord};

/// Configuration sent from host to cell.
#[derive(Debug, Clone, PartialEq, Facet)]
pub struct TracingConfig {
    /// Minimum level to emit (records below this are dropped).
    pub min_level: Level,
    /// Target filters (empty = accept all).
    /// Format: "target=level" or just "target" (accepts all levels).
    pub filters: Vec<String>,
    /// Whether to include span enter/exit events (verbose).
    pub include_span_events: bool,
}

impl Default for TracingConfig {
    fn default() -> Self {
        Self {
            min_level: Level::Info,
            filters: Vec::new(),
            include_span_events: false,
        }
    }
}

/// Result of configuration update.
#[repr(u8)]
#[derive(Debug, Clone, PartialEq, Facet)]
pub enum ConfigResult {
    /// Configuration applied successfully.
    Ok = 0,
    /// Invalid filter syntax.
    InvalidFilter(String) = 1,
}

/// Service implemented by the CELL to receive config and provide tracing stream.
///
/// The host calls this service on the cell to:
/// 1. Push configuration updates
/// 2. Establish the tracing stream (cell sends records to host)
///
/// # Protocol Flow
///
/// 1. Host spawns cell and establishes connection
/// 2. Host calls `subscribe(tx)` with a `Tx<TracingRecord>`
/// 3. Cell stores the Tx and spawns a background task to forward records
/// 4. Host receives records on the corresponding `Rx<TracingRecord>`
/// 5. Optionally, host calls `configure(...)` to adjust filters/levels
#[service]
pub trait CellTracing {
    /// Update the tracing configuration.
    ///
    /// Called by host to change filters, levels, etc.
    async fn configure(&self, config: TracingConfig) -> ConfigResult;

    /// Establish the tracing stream.
    ///
    /// The cell sends `TracingRecord` values to the host via the `Tx` channel.
    /// This is a long-lived stream that persists for the cell's lifetime.
    ///
    /// The host keeps the corresponding `Rx` end and receives records.
    async fn subscribe(&self, sink: Tx<TracingRecord>);
}
