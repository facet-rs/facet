//! Control channel payloads.

use facet::Facet;

/// Reasons for closing a channel.
#[derive(Debug, Clone, PartialEq, Eq, Facet)]
#[repr(u8)]
pub enum CloseReason {
    /// Normal completion.
    Normal,
    /// Error occurred.
    Error(String),
}

/// Reasons for cancelling a channel.
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
/// The `method_id` in MsgDescHot indicates the verb:
/// - 1: OpenChannel
/// - 2: CloseChannel
/// - 3: CancelChannel
/// - 4: GrantCredits
/// - 5: Ping
/// - 6: Pong
#[derive(Debug, Clone, Facet)]
#[repr(u8)]
pub enum ControlPayload {
    /// Open a new data channel.
    OpenChannel {
        channel_id: u32,
        service_name: String,
        method_name: String,
        metadata: Vec<(String, Vec<u8>)>,
    },
    /// Close a channel gracefully.
    CloseChannel {
        channel_id: u32,
        reason: CloseReason,
    },
    /// Cancel a channel.
    CancelChannel {
        channel_id: u32,
        reason: CancelReason,
    },
    /// Grant flow control credits.
    GrantCredits { channel_id: u32, bytes: u32 },
    /// Liveness probe.
    Ping { payload: [u8; 8] },
    /// Response to Ping.
    Pong { payload: [u8; 8] },
}

/// Control method IDs.
pub mod control_method {
    pub const OPEN_CHANNEL: u32 = 1;
    pub const CLOSE_CHANNEL: u32 = 2;
    pub const CANCEL_CHANNEL: u32 = 3;
    pub const GRANT_CREDITS: u32 = 4;
    pub const PING: u32 = 5;
    pub const PONG: u32 = 6;
}
