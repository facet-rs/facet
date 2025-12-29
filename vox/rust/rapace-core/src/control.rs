//! Control channel payloads.
//!
//! See spec: [Core Protocol: Control Channel](https://rapace.dev/spec/core/#control-channel-channel-0)

use facet::Facet;

// Re-export control verb constants from rapace-protocol
pub use rapace_protocol::control_verb as control_method;

// Re-export spec-compliant types from rapace-protocol
pub use rapace_protocol::{
    AttachTo, CancelChannel, CancelReason, ChannelKind, CloseChannel, CloseReason, Direction,
    GoAway, GoAwayReason, GrantCredits, Hello, Limits, MethodInfo, OpenChannel, Ping, Pong, Role,
};

/// Control channel payloads (channel 0).
///
/// Spec: `[impl core.control.reserved]` - channel 0 reserved for control messages.
/// Spec: `[impl core.control.verb-selector]` - `method_id` selects the control verb.
/// Spec: `[impl core.control.payload-encoding]` - payloads are Postcard-encoded.
///
/// The `method_id` in MsgDescHot indicates the verb:
/// - 0: Hello (handshake)
/// - 1: OpenChannel
/// - 2: CloseChannel
/// - 3: CancelChannel
/// - 4: GrantCredits
/// - 5: Ping
/// - 6: Pong
/// - 7: GoAway
///
/// This enum provides a unified type for control payloads used in the session layer.
/// For wire-format types, see the individual structs re-exported from `rapace_protocol`.
#[derive(Debug, Clone, Facet)]
#[repr(u8)]
pub enum ControlPayload {
    /// Open a new data channel.
    ///
    /// Spec: `[impl core.channel.open]` - channels MUST be opened via OpenChannel.
    OpenChannel {
        channel_id: u32,
        service_name: String,
        method_name: String,
        metadata: Vec<(String, Vec<u8>)>,
    },
    /// Close a channel gracefully.
    ///
    /// Spec: `[impl core.close.close-channel-semantics]` - unilateral, no ack required.
    CloseChannel {
        channel_id: u32,
        reason: CloseReason,
    },
    /// Cancel a channel (immediate abort).
    ///
    /// Spec: `[impl core.cancel.idempotent]` - multiple cancels are harmless.
    /// Spec: `[impl core.cancel.propagation]` - CALL cancel also cancels attached channels.
    CancelChannel {
        channel_id: u32,
        reason: CancelReason,
    },
    /// Grant flow control credits.
    ///
    /// Spec: `[impl core.flow.credit-additive]` - credits are additive.
    GrantCredits { channel_id: u32, bytes: u32 },
    /// Liveness probe.
    ///
    /// Spec: `[impl core.ping.semantics]` - receiver MUST respond with Pong.
    Ping { payload: [u8; 8] },
    /// Response to Ping.
    ///
    /// Spec: `[impl core.ping.semantics]` - MUST echo the same payload.
    Pong { payload: [u8; 8] },
}
