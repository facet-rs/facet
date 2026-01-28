#![deny(unsafe_code)]

//! Spec-level wire types.
//!
//! Canonical definitions live in `docs/content/spec/_index.md` and `docs/content/shm-spec/_index.md`.

use facet::Facet;

/// Connection ID identifying a virtual connection on a link.
///
/// Connection 0 is the root connection, established implicitly when the link is created.
/// Additional connections are opened via Connect/Accept messages.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default, Facet)]
#[repr(transparent)]
pub struct ConnectionId(pub u64);

impl ConnectionId {
    /// The root connection (always exists on a link).
    pub const ROOT: Self = Self(0);

    /// Create a new connection ID.
    pub const fn new(id: u64) -> Self {
        Self(id)
    }

    /// Get the raw u64 value.
    pub const fn raw(self) -> u64 {
        self.0
    }

    /// Check if this is the root connection.
    pub const fn is_root(self) -> bool {
        self.0 == 0
    }
}

impl From<u64> for ConnectionId {
    fn from(id: u64) -> Self {
        Self(id)
    }
}

impl From<ConnectionId> for u64 {
    fn from(id: ConnectionId) -> Self {
        id.0
    }
}

impl std::fmt::Display for ConnectionId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "conn:{}", self.0)
    }
}

/// Request ID identifying an in-flight RPC request.
///
/// Request IDs are unique within a connection and monotonically increasing.
/// r[impl call.request-id.uniqueness]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default, Facet)]
#[repr(transparent)]
pub struct RequestId(pub u64);

impl RequestId {
    /// Create a new request ID.
    pub const fn new(id: u64) -> Self {
        Self(id)
    }

    /// Get the raw u64 value.
    pub const fn raw(self) -> u64 {
        self.0
    }
}

impl From<u64> for RequestId {
    fn from(id: u64) -> Self {
        Self(id)
    }
}

impl From<RequestId> for u64 {
    fn from(id: RequestId) -> Self {
        id.0
    }
}

impl std::fmt::Display for RequestId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "req:{}", self.0)
    }
}

/// Method ID identifying an RPC method.
///
/// Method IDs are computed as a hash of the service and method names.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default, Facet)]
#[repr(transparent)]
pub struct MethodId(pub u64);

impl MethodId {
    /// Create a new method ID.
    pub const fn new(id: u64) -> Self {
        Self(id)
    }

    /// Get the raw u64 value.
    pub const fn raw(self) -> u64 {
        self.0
    }
}

impl From<u64> for MethodId {
    fn from(id: u64) -> Self {
        Self(id)
    }
}

impl From<MethodId> for u64 {
    fn from(id: MethodId) -> Self {
        id.0
    }
}

impl std::fmt::Display for MethodId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "method:{}", self.0)
    }
}

/// Hello message for handshake.
// r[impl message.hello.structure]
#[repr(u8)]
#[derive(Debug, Clone, PartialEq, Eq, Facet)]
pub enum Hello {
    /// Spec v3 Hello - metadata includes flags.
    V3 {
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

/// Metadata entry flags.
///
/// r[impl call.metadata.flags] - Flags control metadata handling behavior.
pub mod metadata_flags {
    /// No special handling.
    pub const NONE: u64 = 0;

    /// Value MUST NOT be logged, traced, or included in error messages.
    pub const SENSITIVE: u64 = 1 << 0;

    /// Value MUST NOT be forwarded to downstream calls.
    pub const NO_PROPAGATE: u64 = 1 << 1;
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
pub fn validate_metadata(metadata: &[(String, MetadataValue, u64)]) -> Result<(), &'static str> {
    use metadata_limits::*;

    // Check entry count
    if metadata.len() > MAX_ENTRIES {
        return Err("call.metadata.limits");
    }

    let mut total_size = 0usize;

    for (key, value, _flags) in metadata {
        // Check key size
        if key.len() > MAX_KEY_SIZE {
            return Err("call.metadata.limits");
        }

        // Check value size
        let value_len = value.byte_len();
        if value_len > MAX_VALUE_SIZE {
            return Err("call.metadata.limits");
        }

        // Accumulate total size (flags are varint-encoded, typically 1 byte)
        total_size += key.len() + value_len;
    }

    // Check total size
    if total_size > MAX_TOTAL_SIZE {
        return Err("call.metadata.limits");
    }

    Ok(())
}

/// Metadata entry: (key, value, flags).
///
/// r[impl call.metadata.type] - Metadata is a list of entries.
/// r[impl call.metadata.flags] - Each entry includes flags for handling behavior.
pub type Metadata = Vec<(String, MetadataValue, u64)>;

/// Protocol message.
///
/// Variant order is wire-significant (postcard enum discriminants).
///
/// # Virtual Connections (v2.0.0)
///
/// A link carries multiple virtual connections, each with its own request ID
/// space, channel ID space, and dispatcher. Connection 0 is implicit on link
/// establishment. Additional connections are opened via Connect/Accept/Reject.
///
/// All messages except Hello, Connect, Accept, and Reject include a `conn_id`
/// field identifying which virtual connection they belong to.
#[repr(u8)]
#[derive(Debug, Clone, PartialEq, Eq, Facet)]
pub enum Message {
    // ========================================================================
    // Link control (no conn_id - applies to entire link)
    // ========================================================================
    /// r[impl message.hello.timing] - Sent immediately after link establishment.
    Hello(Hello) = 0,

    // ========================================================================
    // Virtual connection control
    // ========================================================================
    /// r[impl message.connect.initiate] - Request a new virtual connection.
    Connect { request_id: u64, metadata: Metadata } = 1,

    /// r[impl message.accept.response] - Accept a virtual connection request.
    Accept {
        request_id: u64,
        conn_id: ConnectionId,
        metadata: Metadata,
    } = 2,

    /// r[impl message.reject.response] - Reject a virtual connection request.
    Reject {
        request_id: u64,
        reason: String,
        metadata: Metadata,
    } = 3,

    // ========================================================================
    // Connection control (conn_id scoped)
    // ========================================================================
    /// r[impl message.goodbye.send] - Close a virtual connection.
    /// r[impl message.goodbye.connection-zero] - Goodbye on conn 0 closes entire link.
    Goodbye {
        conn_id: ConnectionId,
        reason: String,
    } = 4,

    // ========================================================================
    // RPC (conn_id scoped)
    // ========================================================================
    /// r[impl core.metadata] - Request carries metadata key-value pairs.
    /// r[impl call.metadata.unknown] - Unknown keys are ignored.
    /// r[impl channeling.request.channels] - Channel IDs listed explicitly for proxy support.
    Request {
        conn_id: ConnectionId,
        request_id: u64,
        method_id: u64,
        metadata: Metadata,
        /// Channel IDs used by this call, in argument declaration order.
        /// This is the authoritative source - servers MUST use these IDs,
        /// not any IDs that may be embedded in the payload.
        channels: Vec<u64>,
        payload: Vec<u8>,
    } = 5,

    /// r[impl core.metadata] - Response carries metadata key-value pairs.
    /// r[impl call.metadata.unknown] - Unknown keys are ignored.
    Response {
        conn_id: ConnectionId,
        request_id: u64,
        metadata: Metadata,
        /// Channel IDs for streams in the response, in return type declaration order.
        /// Client uses these to bind receivers for incoming Data messages.
        channels: Vec<u64>,
        payload: Vec<u8>,
    } = 6,

    /// r[impl call.cancel.message] - Cancel message requests callee stop processing.
    /// r[impl call.cancel.no-response-required] - Caller should timeout, not wait indefinitely.
    Cancel {
        conn_id: ConnectionId,
        request_id: u64,
    } = 7,

    // ========================================================================
    // Channels (conn_id scoped)
    // ========================================================================
    // r[impl wire.stream] - Tx<T>/Rx<T> encoded as u64 channel ID on wire
    Data {
        conn_id: ConnectionId,
        channel_id: u64,
        payload: Vec<u8>,
    } = 8,

    Close {
        conn_id: ConnectionId,
        channel_id: u64,
    } = 9,

    Reset {
        conn_id: ConnectionId,
        channel_id: u64,
    } = 10,

    Credit {
        conn_id: ConnectionId,
        channel_id: u64,
        bytes: u32,
    } = 11,
}
