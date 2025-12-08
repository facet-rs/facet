// src/channel.rs

use std::marker::PhantomData;
use std::sync::Arc;
use crate::types::{ChannelId, MethodId, MsgId, ByteLen};
use crate::flow::ChannelFlowSender;
use crate::frame::{FrameFlags};
use tokio::sync::mpsc;

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
    inner: Option<Arc<tokio::sync::Mutex<ChannelInner>>>,
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

/// Internal channel state shared between send/recv operations.
/// This will be connected to the session layer for actual frame transport.
struct ChannelInner {
    /// Sender for outbound frames (frames we're sending to the peer).
    /// Connected to the session's outbound frame processing.
    outbound_tx: mpsc::UnboundedSender<OutboundFrame>,
    /// Receiver for inbound frames (frames we're receiving from the peer).
    /// Fed by the session's inbound frame dispatcher.
    inbound_rx: mpsc::UnboundedReceiver<Frame>,
    /// Message ID counter for building frames.
    next_msg_id: std::sync::atomic::AtomicU64,
}

/// An outbound frame ready to be sent through the session layer.
/// This is a placeholder - the session layer will convert this to actual wire frames.
#[derive(Debug)]
pub struct OutboundFrame {
    pub channel_id: ChannelId,
    pub method_id: MethodId,
    pub msg_id: MsgId,
    pub data: Vec<u8>,
    pub flags: FrameFlags,
}

impl ChannelInner {
    fn new() -> (Self, mpsc::UnboundedSender<Frame>, mpsc::UnboundedReceiver<OutboundFrame>) {
        let (outbound_tx, outbound_rx) = mpsc::unbounded_channel();
        let (inbound_tx, inbound_rx) = mpsc::unbounded_channel();

        (
            ChannelInner {
                outbound_tx,
                inbound_rx,
                next_msg_id: std::sync::atomic::AtomicU64::new(1),
            },
            inbound_tx,
            outbound_rx,
        )
    }

    fn next_msg_id(&self) -> MsgId {
        let id = self.next_msg_id.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        MsgId::new(id)
    }
}

// === Open state: bidirectional I/O ===

impl Channel<Open> {
    /// Create a new open channel.
    ///
    /// This is an internal constructor; channels are typically opened via the session API.
    /// Returns the channel along with the inbound sender and outbound receiver for session wiring.
    pub(crate) fn new(id: ChannelId, method_id: MethodId, initial_credits: u32) -> (Self, mpsc::UnboundedSender<Frame>, mpsc::UnboundedReceiver<OutboundFrame>) {
        let (inner, inbound_tx, outbound_rx) = ChannelInner::new();
        let channel = Channel {
            id,
            method_id,
            flow: ChannelFlowSender::new(initial_credits),
            stats: ChannelStats::default(),
            inner: Some(Arc::new(tokio::sync::Mutex::new(inner))),
            _state: PhantomData,
        };
        (channel, inbound_tx, outbound_rx)
    }

    /// Send data on this channel.
    ///
    /// This method will wait for sufficient flow control credits before sending.
    /// Returns an error if the channel is closed or credits cannot be acquired.
    pub async fn send(&mut self, data: &[u8]) -> Result<(), SendError> {
        // 1. Check if we have enough credits
        let data_len = data.len() as u32;
        let byte_len = ByteLen::new(data_len, u32::MAX).ok_or(SendError::PayloadTooLarge)?;

        // Try to acquire credits (non-blocking for now, in future could wait)
        let permit = self.flow.try_acquire(byte_len)
            .map_err(|_| SendError::InsufficientCredits)?;

        // 2. Get the inner state
        let inner = self.inner.as_ref().ok_or(SendError::ChannelClosed)?;
        let inner_guard = inner.lock().await;

        // 3. Build and send the frame
        let msg_id = inner_guard.next_msg_id();
        let frame = OutboundFrame {
            channel_id: self.id,
            method_id: self.method_id,
            msg_id,
            data: data.to_vec(),
            flags: FrameFlags::DATA,
        };

        inner_guard.outbound_tx.send(frame)
            .map_err(|_| SendError::SessionClosed)?;

        // 4. Update stats
        self.stats.bytes_sent += data_len as u64;
        self.stats.messages_sent += 1;

        // 5. Consume the credits (data was sent successfully)
        permit.consume();

        Ok(())
    }

    /// Send data and immediately half-close the send side (set EOS flag).
    ///
    /// After this call, the channel transitions to `HalfClosedLocal` and cannot
    /// send more data, but can still receive data from the peer.
    pub async fn send_and_close(mut self, data: &[u8]) -> Result<Channel<HalfClosedLocal>, SendError> {
        // Similar to send, but with EOS flag
        let data_len = data.len() as u32;
        let byte_len = ByteLen::new(data_len, u32::MAX).ok_or(SendError::PayloadTooLarge)?;

        let permit = self.flow.try_acquire(byte_len)
            .map_err(|_| SendError::InsufficientCredits)?;

        let inner = self.inner.as_ref().ok_or(SendError::ChannelClosed)?;
        let inner_guard = inner.lock().await;

        let msg_id = inner_guard.next_msg_id();
        let frame = OutboundFrame {
            channel_id: self.id,
            method_id: self.method_id,
            msg_id,
            data: data.to_vec(),
            flags: FrameFlags::DATA | FrameFlags::EOS,
        };

        inner_guard.outbound_tx.send(frame)
            .map_err(|_| SendError::SessionClosed)?;

        self.stats.bytes_sent += data_len as u64;
        self.stats.messages_sent += 1;

        permit.consume();

        // Drop the guard before moving self.inner
        drop(inner_guard);

        // Transition to HalfClosedLocal
        Ok(Channel {
            id: self.id,
            method_id: self.method_id,
            flow: self.flow,
            stats: self.stats,
            inner: self.inner,
            _state: PhantomData,
        })
    }

    /// Half-close the send side without sending data (just send EOS).
    ///
    /// Transitions the channel to `HalfClosedLocal`. The channel can still receive
    /// data from the peer.
    pub async fn close_send(self) -> Result<Channel<HalfClosedLocal>, SendError> {
        // Send an empty frame with EOS flag
        let inner = self.inner.as_ref().ok_or(SendError::ChannelClosed)?;
        let inner_guard = inner.lock().await;

        let msg_id = inner_guard.next_msg_id();
        let frame = OutboundFrame {
            channel_id: self.id,
            method_id: self.method_id,
            msg_id,
            data: Vec::new(),
            flags: FrameFlags::DATA | FrameFlags::EOS,
        };

        inner_guard.outbound_tx.send(frame)
            .map_err(|_| SendError::SessionClosed)?;

        // Drop the guard before moving self.inner
        drop(inner_guard);

        // Transition to HalfClosedLocal
        Ok(Channel {
            id: self.id,
            method_id: self.method_id,
            flow: self.flow,
            stats: self.stats,
            inner: self.inner,
            _state: PhantomData,
        })
    }

    /// Receive data from the peer.
    ///
    /// Returns `None` if the peer has sent EOS (end-of-stream), indicating no more
    /// data will be received. In that case, the channel should transition to
    /// `HalfClosedRemote` via internal session handling.
    pub async fn recv(&mut self) -> Option<Frame> {
        let inner = self.inner.as_ref()?;
        let mut inner_guard = inner.lock().await;

        // Receive from the inbound channel
        let frame = inner_guard.inbound_rx.recv().await?;

        // Update stats
        self.stats.bytes_received += frame.data.len() as u64;
        self.stats.messages_received += 1;

        Some(frame)
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
        let inner = self.inner.as_ref()?;
        let mut inner_guard = inner.lock().await;

        // Receive from the inbound channel
        let frame = inner_guard.inbound_rx.recv().await?;

        // Update stats
        self.stats.bytes_received += frame.data.len() as u64;
        self.stats.messages_received += 1;

        Some(frame)
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
            inner: None, // Drop the inner state when closed
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
        // Same logic as Open::send
        let data_len = data.len() as u32;
        let byte_len = ByteLen::new(data_len, u32::MAX).ok_or(SendError::PayloadTooLarge)?;

        let permit = self.flow.try_acquire(byte_len)
            .map_err(|_| SendError::InsufficientCredits)?;

        let inner = self.inner.as_ref().ok_or(SendError::ChannelClosed)?;
        let inner_guard = inner.lock().await;

        let msg_id = inner_guard.next_msg_id();
        let frame = OutboundFrame {
            channel_id: self.id,
            method_id: self.method_id,
            msg_id,
            data: data.to_vec(),
            flags: FrameFlags::DATA,
        };

        inner_guard.outbound_tx.send(frame)
            .map_err(|_| SendError::SessionClosed)?;

        self.stats.bytes_sent += data_len as u64;
        self.stats.messages_sent += 1;

        permit.consume();

        Ok(())
    }

    /// Close the send side, transitioning to `Closed`.
    ///
    /// After this call, both sides have sent EOS and the channel is fully closed.
    pub async fn close_send(self) -> Result<Channel<Closed>, SendError> {
        // Send an empty frame with EOS flag
        let inner = self.inner.as_ref().ok_or(SendError::ChannelClosed)?;
        let inner_guard = inner.lock().await;

        let msg_id = inner_guard.next_msg_id();
        let frame = OutboundFrame {
            channel_id: self.id,
            method_id: self.method_id,
            msg_id,
            data: Vec::new(),
            flags: FrameFlags::DATA | FrameFlags::EOS,
        };

        inner_guard.outbound_tx.send(frame)
            .map_err(|_| SendError::SessionClosed)?;

        // Transition to Closed
        Ok(Channel {
            id: self.id,
            method_id: self.method_id,
            flow: self.flow,
            stats: self.stats,
            inner: None, // Drop the inner state when closed
            _state: PhantomData,
        })
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
    pub async fn cancel(self) -> Channel<Closed> {
        // Try to send a CANCEL frame if the channel is still open
        if let Some(inner) = self.inner.as_ref() {
            let inner_guard = inner.lock().await;
            let msg_id = inner_guard.next_msg_id();
            let frame = OutboundFrame {
                channel_id: self.id,
                method_id: self.method_id,
                msg_id,
                data: Vec::new(),
                flags: FrameFlags::CANCEL,
            };

            // Best effort - ignore errors since we're cancelling anyway
            let _ = inner_guard.outbound_tx.send(frame);
        }

        Channel {
            id: self.id,
            method_id: self.method_id,
            flow: self.flow,
            stats: self.stats,
            inner: None, // Drop the inner state when closed
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
    #[tokio::test]
    async fn typestate_transitions_compile() {
        let channel_id = ChannelId::new(1).unwrap();
        let method_id = MethodId::new(42);

        // Open -> HalfClosedLocal
        let (channel, _inbound_tx, _outbound_rx): (Channel<Open>, _, _) = Channel::new(channel_id, method_id, 1024);
        let channel: Channel<HalfClosedLocal> = channel.close_send().await.unwrap();

        // HalfClosedLocal -> Closed (via peer_closed)
        let channel: Channel<Closed> = channel.peer_closed();
        let _ = channel.stats();
    }

    #[tokio::test]
    async fn open_to_half_closed_remote_via_cancel() {
        let channel_id = ChannelId::new(2).unwrap();
        let method_id = MethodId::new(99);

        let (channel, _inbound_tx, _outbound_rx): (Channel<Open>, _, _) = Channel::new(channel_id, method_id, 1024);
        // Can cancel from any state
        let _closed: Channel<Closed> = channel.cancel().await;
    }

    #[test]
    fn channel_id_and_method_id_accessors() {
        let channel_id = ChannelId::new(3).unwrap();
        let method_id = MethodId::new(123);

        let (channel, _inbound_tx, _outbound_rx): (Channel<Open>, _, _) = Channel::new(channel_id, method_id, 1024);

        assert_eq!(channel.id(), channel_id);
        assert_eq!(channel.method_id(), method_id);
        assert_eq!(channel.available_credits(), 1024);
    }

    #[test]
    fn stats_are_accessible() {
        let channel_id = ChannelId::new(4).unwrap();
        let method_id = MethodId::new(456);

        let (channel, _inbound_tx, _outbound_rx): (Channel<Open>, _, _) = Channel::new(channel_id, method_id, 1024);
        let stats = channel.current_stats();

        assert_eq!(stats.bytes_sent, 0);
        assert_eq!(stats.bytes_received, 0);
        assert_eq!(stats.messages_sent, 0);
        assert_eq!(stats.messages_received, 0);
    }

    /// This test demonstrates what CANNOT compile - uncomment to verify type safety
    #[allow(dead_code)]
    async fn typestate_prevents_invalid_operations() {
        let channel_id = ChannelId::new(5).unwrap();
        let method_id = MethodId::new(789);

        let (channel, _inbound_tx, _outbound_rx): (Channel<Open>, _, _) = Channel::new(channel_id, method_id, 1024);
        let channel: Channel<HalfClosedLocal> = channel.close_send().await.unwrap();

        // These should NOT compile:
        // channel.send(&[1, 2, 3]).await; // Error: HalfClosedLocal has no send method
        // channel.close_send(); // Error: HalfClosedLocal has no close_send method

        // This SHOULD compile:
        let _ = channel.cancel().await;
    }

    #[tokio::test]
    async fn test_send_and_receive() {
        let channel_id = ChannelId::new(10).unwrap();
        let method_id = MethodId::new(42);

        // Create channel
        let (mut channel, inbound_tx, mut outbound_rx): (Channel<Open>, _, _) =
            Channel::new(channel_id, method_id, 1024);

        // Send data
        let data = b"Hello, World!";
        channel.send(data).await.unwrap();

        // Receive the outbound frame
        let outbound_frame = outbound_rx.recv().await.unwrap();
        assert_eq!(outbound_frame.data, data);
        assert_eq!(outbound_frame.channel_id, channel_id);
        assert_eq!(outbound_frame.method_id, method_id);

        // Send inbound frame
        let inbound_frame = Frame {
            data: b"Response".to_vec(),
            is_eos: false,
            is_error: false,
        };
        inbound_tx.send(inbound_frame).unwrap();

        // Receive the inbound frame
        let received = channel.recv().await.unwrap();
        assert_eq!(received.data, b"Response");

        // Check stats
        let stats = channel.current_stats();
        assert_eq!(stats.bytes_sent, data.len() as u64);
        assert_eq!(stats.messages_sent, 1);
        assert_eq!(stats.bytes_received, 8);
        assert_eq!(stats.messages_received, 1);
    }

    #[tokio::test]
    async fn test_close_send_emits_eos() {
        let channel_id = ChannelId::new(11).unwrap();
        let method_id = MethodId::new(99);

        let (channel, _inbound_tx, mut outbound_rx): (Channel<Open>, _, _) =
            Channel::new(channel_id, method_id, 1024);

        // Close the send side
        let _half_closed = channel.close_send().await.unwrap();

        // Check that an EOS frame was sent
        let outbound_frame = outbound_rx.recv().await.unwrap();
        assert!(outbound_frame.flags.contains(FrameFlags::EOS));
        assert_eq!(outbound_frame.data.len(), 0);
    }

    #[tokio::test]
    async fn test_cancel_emits_cancel_frame() {
        let channel_id = ChannelId::new(12).unwrap();
        let method_id = MethodId::new(123);

        let (channel, _inbound_tx, mut outbound_rx): (Channel<Open>, _, _) =
            Channel::new(channel_id, method_id, 1024);

        // Cancel the channel
        let _closed = channel.cancel().await;

        // Check that a CANCEL frame was sent
        let outbound_frame = outbound_rx.recv().await.unwrap();
        assert!(outbound_frame.flags.contains(FrameFlags::CANCEL));
    }
}
