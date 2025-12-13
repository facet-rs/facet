//! Error types used throughout Picante.

use crate::key::{DynKey, QueryKindId};
use std::fmt;
use std::sync::Arc;

/// Result type used by Picante APIs.
pub type PicanteResult<T> = std::result::Result<T, Arc<PicanteError>>;

/// A Picante runtime / persistence error.
#[derive(Debug)]
pub enum PicanteError {
    /// A query tried to (directly or indirectly) depend on itself within the same async task.
    Cycle {
        /// The query that was requested.
        requested: DynKey,
        /// The task-local query stack at the point the cycle was detected.
        stack: Vec<DynKey>,
    },

    /// Failed to encode a value using `facet-postcard`.
    Encode {
        /// What we were trying to encode (for diagnostics).
        what: &'static str,
        /// Human-readable error message.
        message: String,
    },

    /// Failed to decode a value using `facet-postcard`.
    Decode {
        /// What we were trying to decode (for diagnostics).
        what: &'static str,
        /// Human-readable error message.
        message: String,
    },

    /// Cache I/O or format errors.
    Cache {
        /// Human-readable error message.
        message: String,
    },

    /// An interned id was requested but is not present in the interning table.
    MissingInternedValue {
        /// Kind id of the interned ingredient.
        kind: QueryKindId,
        /// Missing id.
        id: u32,
    },

    /// An input value was requested but is not present (missing or removed).
    MissingInputValue {
        /// Kind id of the input ingredient.
        kind: QueryKindId,
        /// Stable hash of the encoded key bytes (for diagnostics).
        key_hash: u64,
    },

    /// A query panicked during execution (caught to avoid poisoning the runtime).
    Panic {
        /// Human-readable panic message (best effort).
        message: String,
    },
}

impl fmt::Display for PicanteError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            PicanteError::Cycle { requested, stack } => write!(
                f,
                "cycle detected requesting {requested:?}; stack depth {}",
                stack.len()
            ),
            PicanteError::Encode { what, message } => write!(f, "encode {what} failed: {message}"),
            PicanteError::Decode { what, message } => write!(f, "decode {what} failed: {message}"),
            PicanteError::Cache { message } => write!(f, "cache error: {message}"),
            PicanteError::MissingInternedValue { kind, id } => {
                write!(f, "missing interned value (kind {}, id {id})", kind.0)
            }
            PicanteError::MissingInputValue { kind, key_hash } => write!(
                f,
                "missing input value (kind {}, key {:016x})",
                kind.0, key_hash
            ),
            PicanteError::Panic { message } => write!(f, "query panicked: {message}"),
        }
    }
}

impl std::error::Error for PicanteError {}
