//! Session layer that wraps a Transport and enforces RPC semantics.
//!
//! The Session handles:
//! - Per-channel credit tracking (flow control)
//! - Channel state machine (Open → HalfClosedLocal/Remote → Closed)
//! - Cancellation (marking channels as cancelled, dropping late frames)
//! - Deadline checking before dispatch
//! - Control frame processing (PING/PONG, CANCEL, CREDITS)

use std::collections::HashMap;
use std::sync::Arc;

use parking_lot::Mutex;
use rapace_core::{
    control_method, ControlPayload, ErrorCode, Frame, FrameFlags, FrameView, MsgDescHot, RpcError,
    Transport, TransportError, NO_DEADLINE,
};

/// Default initial credits for new channels (64KB).
pub const DEFAULT_INITIAL_CREDITS: u32 = 65536;

/// Channel lifecycle state.
///
/// Follows HTTP/2-style half-close semantics:
/// - Open: Both sides can send
/// - HalfClosedLocal: We sent EOS, peer can still send
/// - HalfClosedRemote: Peer sent EOS, we can still send
/// - Closed: Both sides sent EOS (or cancelled)
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ChannelLifecycle {
    /// Channel is open, both sides can send.
    #[default]
    Open,
    /// We sent EOS, waiting for peer's EOS.
    HalfClosedLocal,
    /// Peer sent EOS, we can still send.
    HalfClosedRemote,
    /// Channel is fully closed (both EOS received, or cancelled).
    Closed,
}

/// Per-channel state tracked by the session.
#[derive(Debug, Clone)]
pub struct ChannelState {
    /// Channel lifecycle state.
    pub lifecycle: ChannelLifecycle,
    /// Available credits for sending on this channel.
    pub send_credits: u32,
    /// Whether this channel has been cancelled.
    pub cancelled: bool,
    /// Number of data frames sent.
    pub frames_sent: u64,
    /// Number of data frames received.
    pub frames_received: u64,
}

impl Default for ChannelState {
    fn default() -> Self {
        Self {
            lifecycle: ChannelLifecycle::Open,
            send_credits: DEFAULT_INITIAL_CREDITS,
            cancelled: false,
            frames_sent: 0,
            frames_received: 0,
        }
    }
}

impl ChannelState {
    /// Check if we can send on this channel.
    pub fn can_send(&self) -> bool {
        !self.cancelled
            && matches!(
                self.lifecycle,
                ChannelLifecycle::Open | ChannelLifecycle::HalfClosedRemote
            )
    }

    /// Check if we can receive on this channel.
    pub fn can_receive(&self) -> bool {
        !self.cancelled
            && matches!(
                self.lifecycle,
                ChannelLifecycle::Open | ChannelLifecycle::HalfClosedLocal
            )
    }

    /// Transition state after we send EOS.
    pub fn mark_local_eos(&mut self) {
        self.lifecycle = match self.lifecycle {
            ChannelLifecycle::Open => ChannelLifecycle::HalfClosedLocal,
            ChannelLifecycle::HalfClosedRemote => ChannelLifecycle::Closed,
            other => other, // Already half-closed or closed
        };
    }

    /// Transition state after receiving EOS from peer.
    pub fn mark_remote_eos(&mut self) {
        self.lifecycle = match self.lifecycle {
            ChannelLifecycle::Open => ChannelLifecycle::HalfClosedRemote,
            ChannelLifecycle::HalfClosedLocal => ChannelLifecycle::Closed,
            other => other, // Already half-closed or closed
        };
    }
}

/// Session wraps a Transport and enforces RPC semantics.
///
/// # Responsibilities
///
/// - **Credits**: Tracks per-channel send credits. Data frames require sufficient
///   credits; control channel (0) is exempt.
/// - **Cancellation**: Tracks cancelled channels and drops frames for them.
/// - **Deadlines**: Checks `deadline_ns` before dispatch; returns `DeadlineExceeded`
///   if expired.
///
/// # Thread Safety
///
/// Session is `Send + Sync` and can be shared via `Arc<Session<T>>`.
pub struct Session<T: Transport> {
    transport: Arc<T>,
    /// Per-channel state. Channel 0 is the control channel.
    channels: Mutex<HashMap<u32, ChannelState>>,
}

impl<T: Transport + Send + Sync> Session<T> {
    /// Create a new session wrapping the given transport.
    pub fn new(transport: Arc<T>) -> Self {
        Self {
            transport,
            channels: Mutex::new(HashMap::new()),
        }
    }

    /// Get a reference to the underlying transport.
    pub fn transport(&self) -> &T {
        &self.transport
    }

    /// Send a frame, enforcing credit limits and channel state for data channels.
    ///
    /// - Control channel (0) is exempt from credit checks.
    /// - Data channels require `payload_len <= available_credits`.
    /// - Tracks EOS to transition channel state.
    /// - Returns `RpcError::Status { code: ResourceExhausted }` if insufficient credits.
    pub async fn send_frame(&self, frame: &Frame) -> Result<(), RpcError> {
        let channel_id = frame.desc.channel_id;
        let payload_len = frame.desc.payload_len;
        let has_eos = frame.desc.flags.contains(FrameFlags::EOS);

        // Control channel is exempt from credit checks and state tracking
        if channel_id != 0 && frame.desc.flags.contains(FrameFlags::DATA) {
            let mut channels = self.channels.lock();
            let state = channels.entry(channel_id).or_default();

            // Check if we can send on this channel
            if !state.can_send() {
                // Silently drop frames for cancelled/closed channels
                return Ok(());
            }

            // Check credits
            if payload_len > state.send_credits {
                return Err(RpcError::Status {
                    code: ErrorCode::ResourceExhausted,
                    message: format!(
                        "insufficient credits: need {}, have {}",
                        payload_len, state.send_credits
                    ),
                });
            }

            // Deduct credits
            state.send_credits -= payload_len;
            state.frames_sent += 1;

            // Track EOS
            if has_eos {
                state.mark_local_eos();
            }
        }

        self.transport
            .send_frame(frame)
            .await
            .map_err(RpcError::Transport)
    }

    /// Receive a frame, processing control frames and filtering cancelled/closed channels.
    ///
    /// - Processes CANCEL control frames to mark channels as cancelled.
    /// - Processes CREDITS control frames to update send credits.
    /// - Tracks EOS to transition channel state.
    /// - Drops data frames for cancelled/closed channels.
    /// - Returns frames that should be dispatched.
    pub async fn recv_frame(&self) -> Result<FrameView<'_>, TransportError> {
        loop {
            let frame = self.transport.recv_frame().await?;

            // Process control frames
            if frame.desc.channel_id == 0 && frame.desc.flags.contains(FrameFlags::CONTROL) {
                self.process_control_frame(&frame);
                // Control frames are passed through to caller (for PING/PONG handling)
                return Ok(frame);
            }

            let channel_id = frame.desc.channel_id;
            let has_eos = frame.desc.flags.contains(FrameFlags::EOS);

            // Check if this channel can receive and update state
            {
                let mut channels = self.channels.lock();
                let state = channels.entry(channel_id).or_default();

                if !state.can_receive() {
                    // Drop frames for cancelled/closed channels, continue receiving
                    continue;
                }

                // Track received frame
                if frame.desc.flags.contains(FrameFlags::DATA) {
                    state.frames_received += 1;
                }

                // Track EOS from peer
                if has_eos {
                    state.mark_remote_eos();
                }
            }

            return Ok(frame);
        }
    }

    /// Check if a frame's deadline has expired.
    ///
    /// Returns `true` if the frame has a deadline and it has passed.
    pub fn is_deadline_exceeded(&self, desc: &MsgDescHot) -> bool {
        if desc.deadline_ns == NO_DEADLINE {
            return false;
        }
        let now = now_ns();
        now > desc.deadline_ns
    }

    /// Grant credits to a channel (called when receiving CREDITS control frame).
    pub fn grant_credits(&self, channel_id: u32, bytes: u32) {
        let mut channels = self.channels.lock();
        let state = channels.entry(channel_id).or_default();
        state.send_credits = state.send_credits.saturating_add(bytes);
    }

    /// Mark a channel as cancelled.
    pub fn cancel_channel(&self, channel_id: u32) {
        let mut channels = self.channels.lock();
        let state = channels.entry(channel_id).or_default();
        state.cancelled = true;
    }

    /// Check if a channel is cancelled.
    pub fn is_cancelled(&self, channel_id: u32) -> bool {
        let channels = self.channels.lock();
        channels
            .get(&channel_id)
            .map(|s| s.cancelled)
            .unwrap_or(false)
    }

    /// Get available send credits for a channel.
    pub fn get_credits(&self, channel_id: u32) -> u32 {
        let channels = self.channels.lock();
        channels
            .get(&channel_id)
            .map(|s| s.send_credits)
            .unwrap_or(DEFAULT_INITIAL_CREDITS)
    }

    /// Get the lifecycle state of a channel.
    pub fn get_lifecycle(&self, channel_id: u32) -> ChannelLifecycle {
        let channels = self.channels.lock();
        channels
            .get(&channel_id)
            .map(|s| s.lifecycle)
            .unwrap_or(ChannelLifecycle::Open)
    }

    /// Get a snapshot of the channel state.
    pub fn get_channel_state(&self, channel_id: u32) -> ChannelState {
        let channels = self.channels.lock();
        channels.get(&channel_id).cloned().unwrap_or_default()
    }

    /// Check if a channel is fully closed.
    pub fn is_closed(&self, channel_id: u32) -> bool {
        self.get_lifecycle(channel_id) == ChannelLifecycle::Closed
    }

    /// Process a control frame, updating session state.
    fn process_control_frame(&self, frame: &FrameView<'_>) {
        match frame.desc.method_id {
            control_method::CANCEL_CHANNEL => {
                // Try to decode CancelChannel payload
                if let Ok(ControlPayload::CancelChannel { channel_id, .. }) =
                    facet_postcard::from_slice::<ControlPayload>(frame.payload)
                {
                    self.cancel_channel(channel_id);
                }
            }
            control_method::GRANT_CREDITS => {
                // Always decode from payload (contains channel_id and bytes)
                if let Ok(ControlPayload::GrantCredits { channel_id, bytes }) =
                    facet_postcard::from_slice::<ControlPayload>(frame.payload)
                {
                    self.grant_credits(channel_id, bytes);
                }
            }
            _ => {
                // Other control frames (PING, PONG, etc.) are passed through
            }
        }
    }

    /// Close the session.
    pub async fn close(&self) -> Result<(), TransportError> {
        self.transport.close().await
    }
}

/// Get current monotonic time in nanoseconds.
fn now_ns() -> u64 {
    use std::time::Instant;
    static START: std::sync::OnceLock<Instant> = std::sync::OnceLock::new();
    let start = START.get_or_init(Instant::now);
    start.elapsed().as_nanos() as u64
}
