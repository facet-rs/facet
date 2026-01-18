//! Tracing service definitions.
//!
//! Defines RPC services for cross-cell tracing:
//! - `HostTracing`: Host implements, cell calls (push records, query config)
//! - `CellTracing`: Cell implements, host calls (push config updates)

use facet::Facet;
use roam::service;

use crate::record::{Level, TracingRecord};

/// Configuration for tracing.
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

// ============================================================================
// HostTracing - Host implements, Cell calls
// ============================================================================

/// Service implemented by the HOST to receive tracing from cells.
///
/// Cells call this service to:
/// 1. Query the tracing configuration on startup
/// 2. Push batches of tracing records
///
/// # Protocol Flow
///
/// 1. Cell starts up and establishes connection to host
/// 2. Cell calls `get_tracing_config()` to get initial filter settings
/// 3. Cell's tracing layer captures events into a buffer
/// 4. Cell periodically calls `emit_tracing(batch)` to push records to host
/// 5. Host receives records and forwards to TUI/logs/subscribers
#[service]
pub trait HostTracing {
    /// Get the current tracing configuration.
    ///
    /// Called by cell on startup to get initial filter/level settings.
    async fn get_tracing_config(&self) -> TracingConfig;

    /// Push a batch of tracing records to the host.
    ///
    /// Called periodically by the cell to forward captured events/spans.
    /// Fire-and-forget: cell doesn't wait for processing, just delivery.
    async fn emit_tracing(&self, records: Vec<TracingRecord>);
}

// ============================================================================
// CellTracing - Cell implements, Host calls
// ============================================================================

/// Service implemented by the CELL to receive configuration updates.
///
/// The host calls this service to push configuration changes after startup.
/// For the initial config, cells query `HostTracing::get_tracing_config()`.
#[service]
pub trait CellTracing {
    /// Update the tracing configuration.
    ///
    /// Called by host to change filters, levels, etc. after startup.
    async fn configure(&self, config: TracingConfig) -> ConfigResult;
}
