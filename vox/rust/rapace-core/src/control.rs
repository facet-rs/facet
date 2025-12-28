//! Control channel payloads.
//!
//! See spec: [Core Protocol: Control Channel](https://rapace.dev/spec/core/#control-channel-channel-0)

use facet::Facet;

/// Reasons for closing a channel.
///
/// Spec: `r[core.close.close-channel-semantics]` - CloseChannel signals sender freed state.
#[derive(Debug, Clone, PartialEq, Eq, Facet)]
#[repr(u8)]
pub enum CloseReason {
    /// Normal completion.
    Normal,
    /// Error occurred.
    Error(String),
}

/// Reasons for cancelling a channel.
///
/// Spec: `r[core.cancel.behavior]` - receivers MUST stop work, discard data, wake waiters.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Facet)]
#[repr(u8)]
pub enum CancelReason {
    /// Client requested cancellation.
    ClientCancel,
    /// Deadline exceeded.
    DeadlineExceeded,
    /// Resource exhausted.
    ResourceExhausted,
}

/// Control channel payloads (channel 0).
///
/// Spec: `r[core.control.reserved]` - channel 0 reserved for control messages.
/// Spec: `r[core.control.verb-selector]` - `method_id` selects the control verb.
/// Spec: `r[core.control.payload-encoding]` - payloads are Postcard-encoded.
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
#[derive(Debug, Clone, Facet)]
#[repr(u8)]
pub enum ControlPayload {
    /// Open a new data channel.
    ///
    /// Spec: `r[core.channel.open]` - channels MUST be opened via OpenChannel.
    OpenChannel {
        channel_id: u32,
        service_name: String,
        method_name: String,
        metadata: Vec<(String, Vec<u8>)>,
    },
    /// Close a channel gracefully.
    ///
    /// Spec: `r[core.close.close-channel-semantics]` - unilateral, no ack required.
    CloseChannel {
        channel_id: u32,
        reason: CloseReason,
    },
    /// Cancel a channel (immediate abort).
    ///
    /// Spec: `r[core.cancel.idempotent]` - multiple cancels are harmless.
    /// Spec: `r[core.cancel.propagation]` - CALL cancel also cancels attached channels.
    CancelChannel {
        channel_id: u32,
        reason: CancelReason,
    },
    /// Grant flow control credits.
    ///
    /// Spec: `r[core.flow.credit-additive]` - credits are additive.
    GrantCredits { channel_id: u32, bytes: u32 },
    /// Liveness probe.
    ///
    /// Spec: `r[core.ping.semantics]` - receiver MUST respond with Pong.
    Ping { payload: [u8; 8] },
    /// Response to Ping.
    ///
    /// Spec: `r[core.ping.semantics]` - MUST echo the same payload.
    Pong { payload: [u8; 8] },
}

/// Control method IDs (used in `method_id` field for channel 0).
///
/// Spec: `r[core.control.verb-selector]` - control verbs table.
/// Spec: `r[core.control.unknown-reserved]` - unknown 0-99 = protocol error.
/// Spec: `r[core.control.unknown-extension]` - unknown 100+ = ignore silently.
pub mod control_method {
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
