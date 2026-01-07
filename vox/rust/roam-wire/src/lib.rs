#![deny(unsafe_code)]

//! Spec-level wire types.
//!
//! Canonical definitions live in `docs/content/spec/_index.md` and `docs/content/shm-spec/_index.md`.

use facet::Facet;

/// Hello message for handshake.
// r[impl message.hello.structure]
#[repr(u8)]
#[derive(Debug, Clone, PartialEq, Eq, Facet)]
pub enum Hello {
    /// Spec v1 Hello.
    V1 {
        max_payload_size: u32,
        initial_stream_credit: u32,
    } = 0,
}

/// Metadata value.
// r[impl unary.metadata.type]
#[repr(u8)]
#[derive(Debug, Clone, PartialEq, Eq, Facet)]
pub enum MetadataValue {
    String(String) = 0,
    Bytes(Vec<u8>) = 1,
    U64(u64) = 2,
}

impl MetadataValue {
    /// Get the byte length of this value.
    pub fn byte_len(&self) -> usize {
        match self {
            MetadataValue::String(s) => s.len(),
            MetadataValue::Bytes(b) => b.len(),
            MetadataValue::U64(_) => 8,
        }
    }
}

/// Metadata validation limits.
///
/// r[impl unary.metadata.limits] - Metadata has size limits.
pub mod metadata_limits {
    /// Maximum number of metadata entries.
    pub const MAX_ENTRIES: usize = 128;
    /// Maximum key size in bytes.
    pub const MAX_KEY_SIZE: usize = 256;
    /// Maximum value size in bytes (16 KB).
    pub const MAX_VALUE_SIZE: usize = 16 * 1024;
    /// Maximum total metadata size in bytes (64 KB).
    pub const MAX_TOTAL_SIZE: usize = 64 * 1024;
}

/// Validate metadata against protocol limits.
///
/// r[impl unary.metadata.limits] - Validate all metadata constraints.
/// r[impl unary.metadata.keys] - Keys at most 256 bytes.
/// r[impl unary.metadata.order] - Order is preserved (Vec maintains order).
/// r[impl unary.metadata.duplicates] - Duplicate keys are allowed.
pub fn validate_metadata(metadata: &[(String, MetadataValue)]) -> Result<(), &'static str> {
    use metadata_limits::*;

    // Check entry count
    if metadata.len() > MAX_ENTRIES {
        return Err("unary.metadata.limits");
    }

    let mut total_size = 0usize;

    for (key, value) in metadata {
        // Check key size
        if key.len() > MAX_KEY_SIZE {
            return Err("unary.metadata.limits");
        }

        // Check value size
        let value_len = value.byte_len();
        if value_len > MAX_VALUE_SIZE {
            return Err("unary.metadata.limits");
        }

        // Accumulate total size
        total_size += key.len() + value_len;
    }

    // Check total size
    if total_size > MAX_TOTAL_SIZE {
        return Err("unary.metadata.limits");
    }

    Ok(())
}

/// Protocol message.
///
/// Variant order is wire-significant (postcard enum discriminants).
#[repr(u8)]
#[derive(Debug, Clone, PartialEq, Eq, Facet)]
pub enum Message {
    // Control
    Hello(Hello) = 0,
    Goodbye {
        reason: String,
    } = 1,

    // RPC
    /// r[impl core.metadata] - Request carries metadata key-value pairs.
    /// r[impl unary.metadata.unknown] - Unknown keys are ignored.
    Request {
        request_id: u64,
        method_id: u64,
        metadata: Vec<(String, MetadataValue)>,
        payload: Vec<u8>,
    } = 2,
    /// r[impl core.metadata] - Response carries metadata key-value pairs.
    /// r[impl unary.metadata.unknown] - Unknown keys are ignored.
    Response {
        request_id: u64,
        metadata: Vec<(String, MetadataValue)>,
        payload: Vec<u8>,
    } = 3,
    /// r[impl unary.cancel.message] - Cancel message requests callee stop processing.
    /// r[impl unary.cancel.no-response-required] - Caller should timeout, not wait indefinitely.
    Cancel {
        request_id: u64,
    } = 4,

    // Streams
    // rs[impl wire.stream] - Stream<T> encoded as u64 stream ID on wire
    Data {
        stream_id: u64,
        payload: Vec<u8>,
    } = 5,
    Close {
        stream_id: u64,
    } = 6,
    Reset {
        stream_id: u64,
    } = 7,
    Credit {
        stream_id: u64,
        bytes: u32,
    } = 8,
}
