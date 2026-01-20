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
        initial_channel_credit: u32,
    } = 0,
}

/// Metadata value.
// r[impl call.metadata.type]
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
/// r[impl call.metadata.limits] - Metadata has size limits.
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
/// r[impl call.metadata.limits] - Validate all metadata constraints.
/// r[impl call.metadata.keys] - Keys at most 256 bytes.
/// r[impl call.metadata.order] - Order is preserved (Vec maintains order).
/// r[impl call.metadata.duplicates] - Duplicate keys are allowed.
pub fn validate_metadata(metadata: &[(String, MetadataValue)]) -> Result<(), &'static str> {
    use metadata_limits::*;

    // Check entry count
    if metadata.len() > MAX_ENTRIES {
        return Err("call.metadata.limits");
    }

    let mut total_size = 0usize;

    for (key, value) in metadata {
        // Check key size
        if key.len() > MAX_KEY_SIZE {
            return Err("call.metadata.limits");
        }

        // Check value size
        let value_len = value.byte_len();
        if value_len > MAX_VALUE_SIZE {
            return Err("call.metadata.limits");
        }

        // Accumulate total size
        total_size += key.len() + value_len;
    }

    // Check total size
    if total_size > MAX_TOTAL_SIZE {
        return Err("call.metadata.limits");
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
    /// r[impl call.metadata.unknown] - Unknown keys are ignored.
    /// r[impl channeling.request.channels] - Channel IDs listed explicitly for proxy support.
    Request {
        request_id: u64,
        method_id: u64,
        metadata: Vec<(String, MetadataValue)>,
        /// Channel IDs used by this call, in argument declaration order.
        /// This is the authoritative source - servers MUST use these IDs,
        /// not any IDs that may be embedded in the payload.
        channels: Vec<u64>,
        payload: Vec<u8>,
    } = 2,
    /// r[impl core.metadata] - Response carries metadata key-value pairs.
    /// r[impl call.metadata.unknown] - Unknown keys are ignored.
    /// r[impl channeling.response.channels] - Channel IDs for streams returned by the method.
    Response {
        request_id: u64,
        metadata: Vec<(String, MetadataValue)>,
        /// Channel IDs for streams in the response, in return type declaration order.
        /// Client uses these to bind receivers for incoming Data messages.
        channels: Vec<u64>,
        payload: Vec<u8>,
    } = 3,
    /// r[impl call.cancel.message] - Cancel message requests callee stop processing.
    /// r[impl call.cancel.no-response-required] - Caller should timeout, not wait indefinitely.
    Cancel {
        request_id: u64,
    } = 4,

    // Channels
    // rs[impl wire.stream] - Tx<T>/Rx<T> encoded as u64 channel ID on wire
    Data {
        channel_id: u64,
        payload: Vec<u8>,
    } = 5,
    Close {
        channel_id: u64,
    } = 6,
    Reset {
        channel_id: u64,
    } = 7,
    Credit {
        channel_id: u64,
        bytes: u32,
    } = 8,
}
