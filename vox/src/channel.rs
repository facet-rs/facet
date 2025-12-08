// src/channel.rs

use std::marker::PhantomData;
use crate::types::{ChannelId, MethodId};
use crate::flow::ChannelFlowSender;

/// Typestate marker for an open channel (both directions active).
///
/// An `Open` channel can:
/// - Send data via `send()`
/// - Receive data via `recv()`
/// - Half-close the send side, transitioning to `HalfClosedLocal`
/// - Cancel, transitioning to `Closed`
pub struct Open;

/// Typestate marker for a channel that is half-closed on the local side.
///
/// The local peer has sent EOS (end-of-stream) and can no longer send data,
/// but the remote peer may still send data. A `HalfClosedLocal` channel can:
/// - Receive data via `recv()`
/// - Transition to `Closed` when the peer sends EOS
/// - Cancel, transitioning to `Closed`
pub struct HalfClosedLocal;

/// Typestate marker for a channel that is half-closed on the remote side.
///
/// The remote peer has sent EOS and will not send more data, but the local
/// peer can still send. A `HalfClosedRemote` channel can:
/// - Send data via `send()`
/// - Close the send side, transitioning to `Closed`
/// - Cancel, transitioning to `Closed`
pub struct HalfClosedRemote;

/// Typestate marker for a closed channel.
///
/// Both sides have sent EOS or the channel was cancelled. No further I/O
/// operations are possible. A `Closed` channel can be queried for statistics.
pub struct Closed;

/// Channel handle with compile-time state tracking via the typestate pattern.
///
/// The `State` type parameter encodes the channel's lifecycle state:
/// - `Channel<Open>`: Both sides can send/receive
/// - `Channel<HalfClosedLocal>`: Local sent EOS, can only receive
/// - `Channel<HalfClosedRemote>`: Remote sent EOS, can only send
/// - `Channel<Closed>`: Both sides closed, no operations allowed
///
/// State transitions consume the channel and return a new channel in the target state,
/// preventing invalid operations at compile time.
pub struct Channel<State> {
    id: ChannelId,
    method_id: MethodId,
    flow: ChannelFlowSender,
    stats: ChannelStats,
    _state: PhantomData<State>,
}

/// Errors that can occur when sending data on a channel.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SendError {
    /// Channel is closed (local or remote).
    ChannelClosed,
    /// Insufficient credits for send.
    InsufficientCredits,
    /// Payload too large for transport.
    PayloadTooLarge,
    /// Session is closed.
    SessionClosed,
}

impl std::fmt::Display for SendError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            SendError::ChannelClosed => write!(f, "channel is closed"),
            SendError::InsufficientCredits => write!(f, "insufficient credits for send"),
            SendError::PayloadTooLarge => write!(f, "payload too large"),
            SendError::SessionClosed => write!(f, "session is closed"),
        }
    }
}

impl std::error::Error for SendError {}

/// A received frame containing data from the peer.
#[derive(Debug)]
pub struct Frame {
    /// The payload data.
    pub data: Vec<u8>,
    /// True if this frame has the EOS (end-of-stream) flag set.
    pub is_eos: bool,
    /// True if this frame has the ERROR flag set.
    pub is_error: bool,
}

/// Statistics for a channel.
#[derive(Debug, Clone, Default)]
pub struct ChannelStats {
    /// Total bytes sent on this channel.
    pub bytes_sent: u64,
    /// Total bytes received on this channel.
    pub bytes_received: u64,
    /// Total messages sent on this channel.
    pub messages_sent: u64,
    /// Total messages received on this channel.
    pub messages_received: u64,
    /// Number of times send was stalled due to flow control.
    pub flow_control_stalls: u64,
}

// === Open state: bidirectional I/O ===

impl Channel<Open> {
    /// Create a new open channel.
    ///
    /// This is an internal constructor; channels are typically opened via the session API.
    pub(crate) fn new(id: ChannelId, method_id: MethodId, initial_credits: u32) -> Self {
        Channel {
            id,
            method_id,
            flow: ChannelFlowSender::new(initial_credits),
            stats: ChannelStats::default(),
            _state: PhantomData,
        }
    }

    /// Send data on this channel.
    ///
    /// This method will wait for sufficient flow control credits before sending.
    /// Returns an error if the channel is closed or credits cannot be acquired.
    pub async fn send(&mut self, data: &[u8]) -> Result<(), SendError> {
        // TODO: implement actual send logic
        // 1. Acquire credits from flow controller
        // 2. Allocate payload slot or use inline
        // 3. Build frame descriptor
        // 4. Enqueue to ring
        // 5. Update stats
        // 6. Ring doorbell
        let _ = data;
        todo!("send will be wired up when session.rs is complete")
    }

    /// Send data and immediately half-close the send side (set EOS flag).
    ///
    /// After this call, the channel transitions to `HalfClosedLocal` and cannot
    /// send more data, but can still receive data from the peer.
    pub fn send_and_close(self, data: &[u8]) -> Channel<HalfClosedLocal> {
        // TODO: implement send + EOS
        let _ = data;
        Channel {
            id: self.id,
            method_id: self.method_id,
            flow: self.flow,
            stats: self.stats,
            _state: PhantomData,
        }
    }

    /// Half-close the send side without sending data (just send EOS).
    ///
    /// Transitions the channel to `HalfClosedLocal`. The channel can still receive
    /// data from the peer.
    pub fn close_send(self) -> Channel<HalfClosedLocal> {
        // TODO: send EOS frame
        Channel {
            id: self.id,
            method_id: self.method_id,
            flow: self.flow,
            stats: self.stats,
            _state: PhantomData,
        }
    }

    /// Receive data from the peer.
    ///
    /// Returns `None` if the peer has sent EOS (end-of-stream), indicating no more
    /// data will be received. In that case, the channel should transition to
    /// `HalfClosedRemote` via internal session handling.
    pub async fn recv(&mut self) -> Option<Frame> {
        // TODO: implement actual receive logic
        // This will be wired up through the session's channel dispatch
        todo!("recv will be wired up when session.rs is complete")
    }
}

// === HalfClosedLocal: can only receive ===

impl Channel<HalfClosedLocal> {
    /// Receive data from the peer.
    ///
    /// The peer may still be sending data even though we've sent EOS.
    /// Returns `None` when the peer sends EOS, at which point the channel
    /// should transition to `Closed`.
    pub async fn recv(&mut self) -> Option<Frame> {
        // TODO: implement actual receive logic
        todo!("recv will be wired up when session.rs is complete")
    }

    /// Called internally when the peer sends EOS, transitioning to `Closed`.
    ///
    /// This is not a public API; it's called by the session's channel dispatcher
    /// when an EOS frame is received.
    pub(crate) fn peer_closed(self) -> Channel<Closed> {
        Channel {
            id: self.id,
            method_id: self.method_id,
            flow: self.flow,
            stats: self.stats,
            _state: PhantomData,
        }
    }
}

// === HalfClosedRemote: can only send ===

impl Channel<HalfClosedRemote> {
    /// Send data on this channel.
    ///
    /// The peer has sent EOS and will not send more data, but we can still
    /// send until we close our send side.
    pub async fn send(&mut self, data: &[u8]) -> Result<(), SendError> {
        // TODO: implement actual send logic
        let _ = data;
        todo!("send will be wired up when session.rs is complete")
    }

    /// Close the send side, transitioning to `Closed`.
    ///
    /// After this call, both sides have sent EOS and the channel is fully closed.
    pub fn close_send(self) -> Channel<Closed> {
        // TODO: send EOS frame
        Channel {
            id: self.id,
            method_id: self.method_id,
            flow: self.flow,
            stats: self.stats,
            _state: PhantomData,
        }
    }
}

// === Closed: no operations ===

impl Channel<Closed> {
    /// Get final statistics for this channel.
    ///
    /// Returns a snapshot of the channel's metrics at the time of closure.
    pub fn stats(&self) -> &ChannelStats {
        &self.stats
    }
}

// === Operations valid in any state ===

impl<S> Channel<S> {
    /// Cancel the channel immediately (advisory).
    ///
    /// Sends a CANCEL frame to the peer and transitions to `Closed`.
    /// This is an advisory operation - the peer should stop processing,
    /// but there may be in-flight frames that arrive after cancellation.
    ///
    /// Cancellation is idempotent and can be called from any state.
    pub fn cancel(self) -> Channel<Closed> {
        // TODO: send CANCEL frame
        Channel {
            id: self.id,
            method_id: self.method_id,
            flow: self.flow,
            stats: self.stats,
            _state: PhantomData,
        }
    }

    /// Get the channel ID.
    pub fn id(&self) -> ChannelId {
        self.id
    }

    /// Get the method ID associated with this channel.
    pub fn method_id(&self) -> MethodId {
        self.method_id
    }

    /// Get the current statistics for this channel.
    pub fn current_stats(&self) -> &ChannelStats {
        &self.stats
    }

    /// Get the number of credits currently available for sending.
    ///
    /// This is useful for determining if a send will block on flow control.
    pub fn available_credits(&self) -> u32 {
        self.flow.available()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Test that type transitions compile correctly.
    ///
    /// This test primarily exists to verify the typestate API - if this compiles,
    /// the type system is enforcing our state machine correctly.
    #[test]
    fn typestate_transitions_compile() {
        let channel_id = ChannelId::new(1).unwrap();
        let method_id = MethodId::new(42);

        // Open -> HalfClosedLocal
        let channel: Channel<Open> = Channel::new(channel_id, method_id, 1024);
        let channel: Channel<HalfClosedLocal> = channel.close_send();

        // HalfClosedLocal -> Closed (via peer_closed)
        let channel: Channel<Closed> = channel.peer_closed();
        let _ = channel.stats();
    }

    #[test]
    fn open_to_half_closed_remote_via_cancel() {
        let channel_id = ChannelId::new(2).unwrap();
        let method_id = MethodId::new(99);

        let channel: Channel<Open> = Channel::new(channel_id, method_id, 1024);
        // Can cancel from any state
        let _closed: Channel<Closed> = channel.cancel();
    }

    #[test]
    fn channel_id_and_method_id_accessors() {
        let channel_id = ChannelId::new(3).unwrap();
        let method_id = MethodId::new(123);

        let channel: Channel<Open> = Channel::new(channel_id, method_id, 1024);

        assert_eq!(channel.id(), channel_id);
        assert_eq!(channel.method_id(), method_id);
        assert_eq!(channel.available_credits(), 1024);
    }

    #[test]
    fn stats_are_accessible() {
        let channel_id = ChannelId::new(4).unwrap();
        let method_id = MethodId::new(456);

        let channel: Channel<Open> = Channel::new(channel_id, method_id, 1024);
        let stats = channel.current_stats();

        assert_eq!(stats.bytes_sent, 0);
        assert_eq!(stats.bytes_received, 0);
        assert_eq!(stats.messages_sent, 0);
        assert_eq!(stats.messages_received, 0);
    }

    /// This test demonstrates what CANNOT compile - uncomment to verify type safety
    #[allow(dead_code)]
    fn typestate_prevents_invalid_operations() {
        let channel_id = ChannelId::new(5).unwrap();
        let method_id = MethodId::new(789);

        let channel: Channel<HalfClosedLocal> = Channel::new(channel_id, method_id, 1024).close_send();

        // These should NOT compile:
        // channel.send(&[1, 2, 3]).await; // Error: HalfClosedLocal has no send method
        // channel.close_send(); // Error: HalfClosedLocal has no close_send method

        // This SHOULD compile:
        let _ = channel.cancel();
    }
}
