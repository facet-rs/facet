//! Session layer that wraps a Transport and enforces RPC semantics.
//!
//! The Session handles:
//! - Per-channel credit tracking (flow control)
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

/// Per-channel state tracked by the session.
#[derive(Debug, Clone)]
struct ChannelState {
    /// Available credits for sending on this channel.
    send_credits: u32,
    /// Whether this channel has been cancelled.
    cancelled: bool,
}

impl Default for ChannelState {
    fn default() -> Self {
        Self {
            send_credits: DEFAULT_INITIAL_CREDITS,
            cancelled: false,
        }
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

    /// Send a frame, enforcing credit limits for data channels.
    ///
    /// - Control channel (0) is exempt from credit checks.
    /// - Data channels require `payload_len <= available_credits`.
    /// - Returns `RpcError::Status { code: ResourceExhausted }` if insufficient credits.
    pub async fn send_frame(&self, frame: &Frame) -> Result<(), RpcError> {
        let channel_id = frame.desc.channel_id;
        let payload_len = frame.desc.payload_len;

        // Control channel is exempt from credit checks
        if channel_id != 0 && frame.desc.flags.contains(FrameFlags::DATA) {
            let mut channels = self.channels.lock();
            let state = channels.entry(channel_id).or_default();

            // Check if channel is cancelled
            if state.cancelled {
                // Silently drop frames for cancelled channels
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
        }

        self.transport
            .send_frame(frame)
            .await
            .map_err(RpcError::Transport)
    }

    /// Receive a frame, processing control frames and filtering cancelled channels.
    ///
    /// - Processes CANCEL control frames to mark channels as cancelled.
    /// - Processes CREDITS control frames to update send credits.
    /// - Drops data frames for cancelled channels.
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

            // Check if this channel is cancelled
            {
                let channels = self.channels.lock();
                if let Some(state) = channels.get(&frame.desc.channel_id) {
                    if state.cancelled {
                        // Drop frames for cancelled channels, continue receiving
                        continue;
                    }
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

    /// Process a control frame, updating session state.
    fn process_control_frame(&self, frame: &FrameView<'_>) {
        match frame.desc.method_id {
            control_method::CANCEL_CHANNEL => {
                // Try to decode CancelChannel payload
                if let Ok(ControlPayload::CancelChannel { channel_id, .. }) =
                    facet_postcard::from_bytes::<ControlPayload>(frame.payload)
                {
                    self.cancel_channel(channel_id);
                }
            }
            control_method::GRANT_CREDITS => {
                // Always decode from payload (contains channel_id and bytes)
                if let Ok(ControlPayload::GrantCredits { channel_id, bytes }) =
                    facet_postcard::from_bytes::<ControlPayload>(frame.payload)
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

#[cfg(test)]
mod tests {
    use super::*;

    // Session tests would go here, but they need a transport implementation.
    // The conformance tests in testkit exercise Session behavior.
}
