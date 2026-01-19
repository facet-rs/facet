//! Tracing service definitions.
//!
//! Defines RPC services for cross-cell tracing:
//! - `HostTracing`: Host implements, cell calls (push records, query config)
//! - `CellTracing`: Cell implements, host calls (push config updates)

use facet::Facet;
use roam::service;

use crate::record::TracingRecord;

/// Configuration for tracing.
///
/// The `filter_directives` field uses the same syntax as `RUST_LOG`:
/// - `trace`, `debug`, `info`, `warn`, `error` - global level
/// - `target=level` - per-target level
/// - `target[span]=level` - per-span level
/// - Multiple directives separated by commas
///
/// Examples:
/// - `"info"` - info level globally
/// - `"debug,hyper=info"` - debug globally, but info for hyper
/// - `"trace,tokio=off"` - trace everything except tokio
#[derive(Debug, Clone, PartialEq, Facet)]
pub struct TracingConfig {
    /// Filter directives in RUST_LOG format.
    ///
    /// Empty string means "info" (the default).
    pub filter_directives: String,
    /// Whether to include span enter/exit events (verbose).
    pub include_span_events: bool,
}

impl Default for TracingConfig {
    fn default() -> Self {
        Self::from_env()
    }
}

impl TracingConfig {
    /// Create config from `RUST_LOG` environment variable.
    ///
    /// If `RUST_LOG` is not set, defaults to "info".
    pub fn from_env() -> Self {
        Self {
            filter_directives: std::env::var("RUST_LOG").unwrap_or_else(|_| "info".to_string()),
            include_span_events: false,
        }
    }

    /// Create config with specific filter directives.
    pub fn with_filter(filter_directives: impl Into<String>) -> Self {
        Self {
            filter_directives: filter_directives.into(),
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
