//! Tracing record schema for cross-cell tracing.
//!
//! Defines the compact record format sent from cells to host.

use facet::Facet;

/// Unique identifier for a span within a cell.
///
/// These IDs are cell-local; the host tags records with `peer_id`
/// to distinguish spans from different cells.
pub type SpanId = u64;

/// Tracing level (mirrors tracing::Level).
#[repr(u8)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Facet)]
pub enum Level {
    /// Very verbose tracing information.
    Trace = 0,
    /// Debugging information.
    Debug = 1,
    /// General information.
    Info = 2,
    /// Warnings.
    Warn = 3,
    /// Errors.
    Error = 4,
}

impl Level {
    /// Convert from `tracing::Level`.
    pub fn from_tracing(level: &tracing::Level) -> Self {
        match *level {
            tracing::Level::TRACE => Level::Trace,
            tracing::Level::DEBUG => Level::Debug,
            tracing::Level::INFO => Level::Info,
            tracing::Level::WARN => Level::Warn,
            tracing::Level::ERROR => Level::Error,
        }
    }
}

/// A field value in a tracing event or span.
#[repr(u8)]
#[derive(Debug, Clone, PartialEq, Facet)]
pub enum FieldValue {
    /// Signed 64-bit integer.
    I64(i64) = 0,
    /// Unsigned 64-bit integer.
    U64(u64) = 1,
    /// Boolean value.
    Bool(bool) = 2,
    /// String (debug or display formatted).
    Str(String) = 3,
}

/// A tracing record sent from cell to host.
#[repr(u8)]
#[derive(Debug, Clone, PartialEq, Facet)]
pub enum TracingRecord {
    /// A new span was entered.
    SpanEnter {
        /// Cell-local span ID.
        id: SpanId,
        /// Parent span ID, if any.
        parent: Option<SpanId>,
        /// Module path (e.g., "my_cell::handler").
        target: String,
        /// Span name.
        name: String,
        /// Span level.
        level: Level,
        /// Span fields.
        fields: Vec<(String, FieldValue)>,
        /// Monotonic timestamp in nanoseconds (cell-local clock).
        timestamp_ns: u64,
    } = 0,

    /// A span was exited (not closed, just not current).
    SpanExit {
        /// Cell-local span ID.
        id: SpanId,
        /// Monotonic timestamp in nanoseconds.
        timestamp_ns: u64,
    } = 1,

    /// A span was closed/dropped.
    SpanClose {
        /// Cell-local span ID.
        id: SpanId,
        /// Monotonic timestamp in nanoseconds.
        timestamp_ns: u64,
    } = 2,

    /// An event occurred.
    Event {
        /// Parent span ID, if any.
        parent: Option<SpanId>,
        /// Module path (e.g., "my_cell::handler").
        target: String,
        /// Event level.
        level: Level,
        /// Event message (the "message" field).
        message: Option<String>,
        /// Event fields (excluding message).
        fields: Vec<(String, FieldValue)>,
        /// Monotonic timestamp in nanoseconds.
        timestamp_ns: u64,
    } = 3,
}
