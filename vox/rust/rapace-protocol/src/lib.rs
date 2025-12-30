//! Rapace protocol wire types.
//!
//! This crate defines the canonical wire format types for the Rapace protocol.
//! It is used by both the implementation (`rapace-core`) and the conformance
//! test suite (`rapace-conformance`).
//!
//! All types here match the [Rapace specification](https://rapace.dev/spec/).

#![no_std]
#![deny(unsafe_code)]

extern crate alloc;

use alloc::string::String;
use alloc::vec::Vec;
use facet::Facet;

// =============================================================================
// Frame Format (spec: frame-format.md)
// =============================================================================

/// Sentinel value indicating payload is inline.
/// Spec: `[impl frame.sentinel.values]`
pub const INLINE_PAYLOAD_SLOT: u32 = 0xFFFFFFFF;

/// Sentinel value indicating no deadline.
/// Spec: `[impl frame.sentinel.values]`
pub const NO_DEADLINE: u64 = 0xFFFFFFFFFFFFFFFF;

/// Size of inline payload in bytes.
/// Spec: `[impl frame.payload.inline]`
pub const INLINE_PAYLOAD_SIZE: usize = 16;

/// Hot-path message descriptor (64 bytes).
/// Spec: `[impl frame.desc.size]`
#[derive(Clone, Copy, Debug, Default)]
#[repr(C, align(64))]
pub struct MsgDescHot {
    // Identity (16 bytes)
    /// Unique message ID per connection, monotonically increasing.
    /// Spec: `[impl frame.msg-id.scope]`
    pub msg_id: u64,
    /// Logical channel (0 = control channel).
    /// Spec: `[impl core.channel.id.zero-reserved]`
    pub channel_id: u32,
    /// Method identifier (FNV-1a hash) or control verb for channel 0.
    /// Spec: `[impl core.method-id.algorithm]`
    pub method_id: u32,

    // Payload location (16 bytes)
    /// Slot index for SHM, or `INLINE_PAYLOAD_SLOT` for inline.
    /// Spec: `[impl frame.sentinel.values]`
    pub payload_slot: u32,
    /// Generation counter for ABA safety (SHM only).
    /// Spec: `[impl frame.shm.slot-guard]`
    pub payload_generation: u32,
    /// Byte offset within slot (typically 0).
    pub payload_offset: u32,
    /// Payload length in bytes.
    /// Spec: `[impl frame.payload.empty]`
    pub payload_len: u32,

    // Flow control & timing (16 bytes)
    /// Frame flags.
    /// Spec: `[impl core.flags.reserved]`
    pub flags: u32,
    /// Credits being granted to peer (valid if `CREDITS` flag set).
    /// Spec: `[impl core.flow.credit-semantics]`
    pub credit_grant: u32,
    /// Absolute deadline in nanoseconds since epoch.
    /// Spec: `[impl frame.sentinel.values]`
    pub deadline_ns: u64,

    // Inline payload (16 bytes)
    /// When `payload_slot == INLINE_PAYLOAD_SLOT`, payload lives here.
    /// Spec: `[impl frame.payload.inline]`
    pub inline_payload: [u8; INLINE_PAYLOAD_SIZE],
}

const _: () = assert!(core::mem::size_of::<MsgDescHot>() == 64);

impl MsgDescHot {
    /// Create a new descriptor with default values.
    pub const fn new() -> Self {
        Self {
            msg_id: 0,
            channel_id: 0,
            method_id: 0,
            payload_slot: INLINE_PAYLOAD_SLOT,
            payload_generation: 0,
            payload_offset: 0,
            payload_len: 0,
            flags: 0,
            credit_grant: 0,
            deadline_ns: NO_DEADLINE,
            inline_payload: [0; INLINE_PAYLOAD_SIZE],
        }
    }

    /// Encode descriptor to bytes (little-endian).
    /// Spec: `[impl frame.desc.encoding]`
    pub fn to_bytes(&self) -> [u8; 64] {
        let mut buf = [0u8; 64];
        buf[0..8].copy_from_slice(&self.msg_id.to_le_bytes());
        buf[8..12].copy_from_slice(&self.channel_id.to_le_bytes());
        buf[12..16].copy_from_slice(&self.method_id.to_le_bytes());
        buf[16..20].copy_from_slice(&self.payload_slot.to_le_bytes());
        buf[20..24].copy_from_slice(&self.payload_generation.to_le_bytes());
        buf[24..28].copy_from_slice(&self.payload_offset.to_le_bytes());
        buf[28..32].copy_from_slice(&self.payload_len.to_le_bytes());
        buf[32..36].copy_from_slice(&self.flags.to_le_bytes());
        buf[36..40].copy_from_slice(&self.credit_grant.to_le_bytes());
        buf[40..48].copy_from_slice(&self.deadline_ns.to_le_bytes());
        buf[48..64].copy_from_slice(&self.inline_payload);
        buf
    }

    /// Decode descriptor from bytes (little-endian).
    /// Spec: `[impl frame.desc.encoding]`
    pub fn from_bytes(buf: &[u8; 64]) -> Self {
        Self {
            msg_id: u64::from_le_bytes(buf[0..8].try_into().unwrap()),
            channel_id: u32::from_le_bytes(buf[8..12].try_into().unwrap()),
            method_id: u32::from_le_bytes(buf[12..16].try_into().unwrap()),
            payload_slot: u32::from_le_bytes(buf[16..20].try_into().unwrap()),
            payload_generation: u32::from_le_bytes(buf[20..24].try_into().unwrap()),
            payload_offset: u32::from_le_bytes(buf[24..28].try_into().unwrap()),
            payload_len: u32::from_le_bytes(buf[28..32].try_into().unwrap()),
            flags: u32::from_le_bytes(buf[32..36].try_into().unwrap()),
            credit_grant: u32::from_le_bytes(buf[36..40].try_into().unwrap()),
            deadline_ns: u64::from_le_bytes(buf[40..48].try_into().unwrap()),
            inline_payload: buf[48..64].try_into().unwrap(),
        }
    }

    /// Returns true if payload is inline.
    #[inline]
    pub const fn is_inline(&self) -> bool {
        self.payload_slot == INLINE_PAYLOAD_SLOT
    }

    /// Returns true if this is a control frame (channel 0).
    #[inline]
    pub const fn is_control(&self) -> bool {
        self.channel_id == 0
    }

    /// Get inline payload slice (only valid if is_inline()).
    #[inline]
    pub fn inline_payload(&self) -> &[u8] {
        &self.inline_payload[..self.payload_len as usize]
    }
}

// =============================================================================
// Frame Flags (spec: core.md#frameflags)
// =============================================================================

/// Frame flags.
pub mod flags {
    /// Frame carries payload data.
    pub const DATA: u32 = 0b0000_0001;
    /// Control message (channel 0 only).
    /// Spec: `[impl core.control.flag-set]`, `[impl core.control.flag-clear]`
    pub const CONTROL: u32 = 0b0000_0010;
    /// End of stream (half-close).
    /// Spec: `[impl core.eos.after-send]`
    pub const EOS: u32 = 0b0000_0100;
    /// Reserved (do not use).
    pub const _RESERVED_08: u32 = 0b0000_1000;
    /// Error response.
    /// Spec: `[impl core.call.error.flags]`
    pub const ERROR: u32 = 0b0001_0000;
    /// Priority hint (maps to priority 192).
    /// Spec: `[impl core.flags.high-priority]`
    pub const HIGH_PRIORITY: u32 = 0b0010_0000;
    /// Contains credit grant.
    /// Spec: `[impl core.flow.credit-semantics]`
    pub const CREDITS: u32 = 0b0100_0000;
    /// Reserved (do not use).
    pub const _RESERVED_80: u32 = 0b1000_0000;
    /// Fire-and-forget (no response expected).
    pub const NO_REPLY: u32 = 0b0001_0000_0000;
    /// This is a response frame.
    /// Spec: `[impl core.call.response.flags]`
    pub const RESPONSE: u32 = 0b0010_0000_0000;
}

// =============================================================================
// Control Verbs (spec: core.md#control-verbs)
// =============================================================================

/// Control message verbs (method_id values for channel 0).
pub mod control_verb {
    /// Hello (handshake).
    pub const HELLO: u32 = 0;
    /// Open a new channel.
    pub const OPEN_CHANNEL: u32 = 1;
    /// Close a channel gracefully.
    pub const CLOSE_CHANNEL: u32 = 2;
    /// Cancel a channel (abort).
    pub const CANCEL_CHANNEL: u32 = 3;
    /// Grant flow control credits.
    pub const GRANT_CREDITS: u32 = 4;
    /// Liveness probe.
    pub const PING: u32 = 5;
    /// Response to Ping.
    pub const PONG: u32 = 6;
    /// Graceful shutdown.
    pub const GO_AWAY: u32 = 7;
}

// =============================================================================
// Handshake Types (spec: handshake.md)
// =============================================================================

/// Protocol version packed as (major << 16) | minor.
/// For v1.0: 0x00010000
pub const PROTOCOL_VERSION_1_0: u32 = 0x00010000;

/// Connection role.
/// Spec: `[impl handshake.role.validation]`
#[derive(Debug, Clone, Copy, PartialEq, Eq, Facet)]
#[repr(u8)]
pub enum Role {
    /// The peer that initiated the connection.
    Initiator = 1,
    /// The peer that accepted the connection.
    Acceptor = 2,
}

/// Feature bits for capability negotiation.
/// Spec: `[impl handshake.features.required]`
pub mod features {
    /// Supports STREAM/TUNNEL channels attached to calls.
    pub const ATTACHED_STREAMS: u64 = 1 << 0;
    /// Uses CallResult envelope with status + trailers.
    pub const CALL_ENVELOPE: u64 = 1 << 1;
    /// Enforces credit-based flow control.
    pub const CREDIT_FLOW_CONTROL: u64 = 1 << 2;
    /// Supports Rapace-level Ping/Pong.
    pub const RAPACE_PING: u64 = 1 << 3;
    /// WebTransport: map channels to QUIC streams.
    pub const WEBTRANSPORT_MULTI_STREAM: u64 = 1 << 4;
    /// WebTransport: support unreliable datagrams.
    pub const WEBTRANSPORT_DATAGRAMS: u64 = 1 << 5;
}

/// Connection limits.
#[derive(Debug, Clone, PartialEq, Eq, Facet)]
pub struct Limits {
    /// Maximum payload bytes per frame.
    pub max_payload_size: u32,
    /// Maximum concurrent channels (0 = unlimited).
    pub max_channels: u32,
    /// Maximum pending RPC calls (0 = unlimited).
    pub max_pending_calls: u32,
}

impl Default for Limits {
    fn default() -> Self {
        Self {
            max_payload_size: 1024 * 1024, // 1MB
            max_channels: 0,               // unlimited
            max_pending_calls: 0,          // unlimited
        }
    }
}

/// Method registry entry.
/// Spec: `[impl handshake.registry.validation]`
#[derive(Debug, Clone, PartialEq, Eq, Facet)]
pub struct MethodInfo {
    /// Method identifier (FNV-1a hash).
    /// Spec: `[impl core.method-id.algorithm]`
    pub method_id: u32,
    /// Structural signature hash (BLAKE3).
    /// Spec: `[impl handshake.sig-hash.blake3]`
    pub sig_hash: [u8; 32],
    /// Human-readable name for debugging.
    pub name: Option<String>,
}

/// Hello message for handshake.
/// Spec: `[impl handshake.required]`
#[derive(Debug, Clone, PartialEq, Eq, Facet)]
pub struct Hello {
    /// Protocol version.
    /// Spec: `[impl handshake.version.major]`, `[impl handshake.version.minor]`
    pub protocol_version: u32,
    /// Connection role.
    /// Spec: `[impl handshake.role.validation]`
    pub role: Role,
    /// Features the peer MUST support.
    /// Spec: `[impl handshake.features.required]`
    pub required_features: u64,
    /// Features the peer supports.
    pub supported_features: u64,
    /// Advertised limits.
    pub limits: Limits,
    /// Method registry.
    /// Spec: `[impl handshake.registry.validation]`
    pub methods: Vec<MethodInfo>,
    /// Extension parameters.
    /// Spec: `[impl handshake.params.unknown]`
    pub params: Vec<(String, Vec<u8>)>,
}

impl Default for Hello {
    fn default() -> Self {
        Self {
            protocol_version: PROTOCOL_VERSION_1_0,
            role: Role::Initiator,
            required_features: features::ATTACHED_STREAMS | features::CALL_ENVELOPE,
            supported_features: features::ATTACHED_STREAMS
                | features::CALL_ENVELOPE
                | features::CREDIT_FLOW_CONTROL
                | features::RAPACE_PING,
            limits: Limits::default(),
            methods: Vec::new(),
            params: Vec::new(),
        }
    }
}

// =============================================================================
// Channel Types (spec: core.md#channel-kinds)
// =============================================================================

/// Channel kind.
/// Spec: `[impl core.channel.kind]`
#[derive(Debug, Clone, Copy, PartialEq, Eq, Facet)]
#[repr(u8)]
pub enum ChannelKind {
    /// Unary request-response.
    Call = 1,
    /// Typed streaming.
    Stream = 2,
    /// Raw byte tunnel.
    Tunnel = 3,
}

/// Stream/tunnel direction.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Facet)]
#[repr(u8)]
pub enum Direction {
    /// Client sends, server receives.
    ClientToServer = 1,
    /// Server sends, client receives.
    ServerToClient = 2,
    /// Both directions.
    Bidir = 3,
}

/// Attachment info for STREAM/TUNNEL channels.
/// Spec: `[impl core.channel.open.attach-required]`
#[derive(Debug, Clone, PartialEq, Eq, Facet)]
pub struct AttachTo {
    /// The parent CALL channel.
    pub call_channel_id: u32,
    /// Port identifier from method signature.
    /// Spec: `[impl core.stream.port-id-assignment]`
    pub port_id: u32,
    /// Direction.
    pub direction: Direction,
}

/// OpenChannel control message.
/// Spec: `[impl core.channel.open]`
#[derive(Debug, Clone, PartialEq, Eq, Facet)]
pub struct OpenChannel {
    /// The new channel's ID.
    /// Spec: `[impl core.channel.id.allocation]`
    pub channel_id: u32,
    /// Channel kind.
    /// Spec: `[impl core.channel.kind]`
    pub kind: ChannelKind,
    /// Attachment for STREAM/TUNNEL.
    /// Spec: `[impl core.channel.open.attach-required]`
    pub attach: Option<AttachTo>,
    /// Metadata (tracing, auth, etc.).
    pub metadata: Vec<(String, Vec<u8>)>,
    /// Initial flow control credits.
    pub initial_credits: u32,
}

// =============================================================================
// Close/Cancel Types (spec: core.md#half-close-and-termination)
// =============================================================================

/// Reason for closing a channel.
/// Spec: `[impl core.close.close-channel-semantics]`
#[derive(Debug, Clone, PartialEq, Eq, Facet)]
#[repr(u8)]
pub enum CloseReason {
    /// Normal completion.
    Normal,
    /// Error occurred.
    Error(String),
}

/// Reason for canceling a channel.
/// Spec: `[impl core.cancel.behavior]`
#[derive(Debug, Clone, Copy, PartialEq, Eq, Facet)]
#[repr(u8)]
pub enum CancelReason {
    /// Client requested cancellation.
    ClientCancel = 1,
    /// Deadline passed.
    DeadlineExceeded = 2,
    /// Out of resources.
    ResourceExhausted = 3,
    /// Protocol violation.
    ProtocolViolation = 4,
    /// Not authenticated.
    Unauthenticated = 5,
    /// Not authorized.
    PermissionDenied = 6,
}

/// CloseChannel control message.
#[derive(Debug, Clone, PartialEq, Eq, Facet)]
pub struct CloseChannel {
    /// The channel to close.
    pub channel_id: u32,
    /// Reason for closing.
    pub reason: CloseReason,
}

/// CancelChannel control message.
/// Spec: `[impl core.cancel.idempotent]`
#[derive(Debug, Clone, PartialEq, Eq, Facet)]
pub struct CancelChannel {
    /// The channel to cancel.
    pub channel_id: u32,
    /// Reason for cancellation.
    pub reason: CancelReason,
}

// =============================================================================
// Flow Control (spec: core.md#flow-control)
// =============================================================================

/// GrantCredits control message.
/// Spec: `[impl core.flow.credit-additive]`
#[derive(Debug, Clone, PartialEq, Eq, Facet)]
pub struct GrantCredits {
    /// The channel receiving credits.
    pub channel_id: u32,
    /// Number of bytes of credit to grant.
    pub bytes: u32,
}

// =============================================================================
// Ping/Pong (spec: core.md#pingpong)
// =============================================================================

/// Ping control message.
/// Spec: `[impl core.ping.semantics]`
#[derive(Debug, Clone, PartialEq, Eq, Facet)]
pub struct Ping {
    /// Opaque payload that must be echoed in Pong.
    pub payload: [u8; 8],
}

/// Pong control message.
/// Spec: `[impl core.ping.semantics]`
#[derive(Debug, Clone, PartialEq, Eq, Facet)]
pub struct Pong {
    /// Echoed payload from Ping.
    pub payload: [u8; 8],
}

// =============================================================================
// GoAway (spec: core.md#goaway)
// =============================================================================

/// Reason for GoAway.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Facet)]
#[repr(u8)]
pub enum GoAwayReason {
    /// Normal shutdown.
    Shutdown = 1,
    /// Maintenance.
    Maintenance = 2,
    /// Server overloaded.
    Overload = 3,
    /// Protocol error.
    ProtocolError = 4,
}

/// GoAway control message.
/// Spec: `[impl core.goaway.last-channel-id]`, `[impl core.goaway.after-send]`
#[derive(Debug, Clone, PartialEq, Eq, Facet)]
pub struct GoAway {
    /// Reason for shutdown.
    pub reason: GoAwayReason,
    /// Last channel ID the sender will process.
    pub last_channel_id: u32,
    /// Human-readable reason.
    pub message: String,
    /// Extension data.
    pub metadata: Vec<(String, Vec<u8>)>,
}

// =============================================================================
// Error Types (spec: errors.md)
// =============================================================================

/// RPC status.
/// Spec: `[impl error.status.success]`, `[impl error.status.error]`
#[derive(Debug, Clone, PartialEq, Eq, Facet)]
pub struct Status {
    /// Error code (0 = OK).
    pub code: u32,
    /// Human-readable description.
    pub message: String,
    /// Opaque structured details.
    pub details: Vec<u8>,
}

impl Status {
    /// Create a success status.
    pub fn ok() -> Self {
        Self {
            code: 0,
            message: String::new(),
            details: Vec::new(),
        }
    }

    /// Create an error status.
    pub fn error(code: u32, message: impl Into<String>) -> Self {
        Self {
            code,
            message: message.into(),
            details: Vec::new(),
        }
    }

    /// Returns true if this is a success status.
    pub fn is_ok(&self) -> bool {
        self.code == 0
    }
}

/// CallResult envelope.
/// Spec: `[impl core.call.result.envelope]`
#[derive(Debug, Clone, PartialEq, Eq, Facet)]
pub struct CallResult {
    /// Status (success or error).
    pub status: Status,
    /// Trailing metadata.
    pub trailers: Vec<(String, Vec<u8>)>,
    /// Response body (present on success, absent on error).
    pub body: Option<Vec<u8>>,
}

/// Standard error codes (gRPC-compatible).
/// Spec: errors.md
pub mod error_code {
    /// Success.
    pub const OK: u32 = 0;
    /// Request was cancelled.
    pub const CANCELLED: u32 = 1;
    /// Unknown error.
    pub const UNKNOWN: u32 = 2;
    /// Invalid argument.
    pub const INVALID_ARGUMENT: u32 = 3;
    /// Deadline exceeded.
    pub const DEADLINE_EXCEEDED: u32 = 4;
    /// Entity not found.
    pub const NOT_FOUND: u32 = 5;
    /// Entity already exists.
    pub const ALREADY_EXISTS: u32 = 6;
    /// Permission denied.
    pub const PERMISSION_DENIED: u32 = 7;
    /// Resource exhausted.
    pub const RESOURCE_EXHAUSTED: u32 = 8;
    /// Failed precondition.
    pub const FAILED_PRECONDITION: u32 = 9;
    /// Operation aborted.
    pub const ABORTED: u32 = 10;
    /// Out of range.
    pub const OUT_OF_RANGE: u32 = 11;
    /// Not implemented.
    pub const UNIMPLEMENTED: u32 = 12;
    /// Internal error.
    pub const INTERNAL: u32 = 13;
    /// Service unavailable.
    pub const UNAVAILABLE: u32 = 14;
    /// Data loss.
    pub const DATA_LOSS: u32 = 15;
    /// Unauthenticated.
    pub const UNAUTHENTICATED: u32 = 16;
    /// Incompatible schema.
    pub const INCOMPATIBLE_SCHEMA: u32 = 17;

    // Protocol error codes (50-99)
    /// Generic protocol error.
    pub const PROTOCOL_ERROR: u32 = 50;
    /// Invalid frame.
    pub const INVALID_FRAME: u32 = 51;
    /// Invalid channel.
    pub const INVALID_CHANNEL: u32 = 52;
    /// Invalid method.
    pub const INVALID_METHOD: u32 = 53;
    /// Decode error.
    pub const DECODE_ERROR: u32 = 54;
    /// Encode error.
    pub const ENCODE_ERROR: u32 = 55;
}

// =============================================================================
// Method ID Computation (spec: core.md#method-id-computation)
// =============================================================================

/// Compute method ID using FNV-1a hash.
/// Spec: `[impl core.method-id.algorithm]`
pub fn compute_method_id(service_name: &str, method_name: &str) -> u32 {
    const FNV_OFFSET: u64 = 0xcbf29ce484222325;
    const FNV_PRIME: u64 = 0x100000001b3;

    let mut hash: u64 = FNV_OFFSET;

    // Hash service name
    for byte in service_name.bytes() {
        hash ^= byte as u64;
        hash = hash.wrapping_mul(FNV_PRIME);
    }

    // Hash separator
    hash ^= b'.' as u64;
    hash = hash.wrapping_mul(FNV_PRIME);

    // Hash method name
    for byte in method_name.bytes() {
        hash ^= byte as u64;
        hash = hash.wrapping_mul(FNV_PRIME);
    }

    // Fold to 32 bits
    ((hash >> 32) ^ hash) as u32
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_msg_desc_hot_size() {
        assert_eq!(core::mem::size_of::<MsgDescHot>(), 64);
    }

    #[test]
    fn test_msg_desc_hot_roundtrip() {
        let mut desc = MsgDescHot::new();
        desc.msg_id = 12345;
        desc.channel_id = 7;
        desc.method_id = 0xDEADBEEF;
        desc.flags = flags::DATA | flags::EOS;
        desc.inline_payload[0..4].copy_from_slice(b"test");
        desc.payload_len = 4;

        let bytes = desc.to_bytes();
        let decoded = MsgDescHot::from_bytes(&bytes);

        assert_eq!(decoded.msg_id, 12345);
        assert_eq!(decoded.channel_id, 7);
        assert_eq!(decoded.method_id, 0xDEADBEEF);
        assert_eq!(decoded.flags, flags::DATA | flags::EOS);
        assert_eq!(&decoded.inline_payload[0..4], b"test");
    }

    #[test]
    fn test_method_id_computation() {
        // Same input produces consistent output
        let id1 = compute_method_id("Calculator", "add");
        let id2 = compute_method_id("Calculator", "add");
        assert_eq!(id1, id2);

        // Different methods produce different IDs
        let id3 = compute_method_id("Calculator", "subtract");
        assert_ne!(id1, id3);

        // Method ID should not be zero (reserved)
        assert_ne!(id1, 0);
    }
}
