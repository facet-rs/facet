//! Session layer that wraps a Transport and enforces RPC semantics.
//!
//! The Session handles:
//! - Per-channel credit tracking (flow control)
//! - Channel state machine (Open → HalfClosedLocal/Remote → Closed)
//! - Cancellation (marking channels as cancelled, dropping late frames)
//! - Deadline checking before dispatch
//! - Control frame processing (PING/PONG, CANCEL, CREDITS)

use std::collections::{HashMap, VecDeque};
use std::sync::Arc;

use parking_lot::Mutex;
use rapace_core::{
    ControlPayload, ErrorCode, Frame, FrameFlags, FrameView, MsgDescHot, NO_DEADLINE, RpcError,
    Transport, TransportError, control_method,
};

/// Default initial credits for new channels (64KB).
pub const DEFAULT_INITIAL_CREDITS: u32 = 65536;

const DEFAULT_MAX_TOMBSTONES: usize = 8192;
const DEFAULT_MAX_TRACKED_CHANNELS: usize = 4096;

fn max_tombstones() -> usize {
    std::env::var("RAPACE_MAX_TOMBSTONES")
        .ok()
        .and_then(|v| v.parse::<usize>().ok())
        .filter(|v| *v > 0)
        .unwrap_or(DEFAULT_MAX_TOMBSTONES)
}

fn max_tracked_channels() -> usize {
    std::env::var("RAPACE_SESSION_MAX_TRACKED_CHANNELS")
        .ok()
        .and_then(|v| v.parse::<usize>().ok())
        .filter(|v| *v > 0)
        .unwrap_or(DEFAULT_MAX_TRACKED_CHANNELS)
}

/// Bounded set of recently-closed channels.
///
/// Used to drop late frames for channels that have been closed/cancelled, while
/// preventing unbounded memory growth. Eviction is FIFO.
#[derive(Debug)]
struct Tombstones {
    max: usize,
    order: VecDeque<u32>,
    map: HashMap<u32, TombstoneInfo>,
}

impl Tombstones {
    fn new(max: usize) -> Self {
        Self {
            max,
            order: VecDeque::new(),
            map: HashMap::new(),
        }
    }

    fn contains(&self, channel_id: u32) -> bool {
        self.map.contains_key(&channel_id)
    }

    fn get(&self, channel_id: u32) -> Option<&TombstoneInfo> {
        self.map.get(&channel_id)
    }

    /// Insert or update a tombstone entry, evicting oldest entries if needed.
    fn insert(&mut self, channel_id: u32, info: TombstoneInfo) {
        let existed = self.map.insert(channel_id, info).is_some();
        if !existed {
            self.order.push_back(channel_id);
        }

        while self.order.len() > self.max {
            if let Some(evicted) = self.order.pop_front() {
                self.map.remove(&evicted);
            }
        }
    }
}

/// Snapshot of a channel's terminal state, retained after state is pruned.
#[derive(Debug, Clone, Copy)]
struct TombstoneInfo {
    lifecycle: ChannelLifecycle,
    cancelled: bool,
}

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
    /// Bounded set of channels that are fully closed/cancelled (drop late frames, prevent re-open).
    tombstones: Mutex<Tombstones>,
}

impl<T: Transport + Send + Sync> Session<T> {
    /// Create a new session wrapping the given transport.
    pub fn new(transport: Arc<T>) -> Self {
        Self {
            transport,
            channels: Mutex::new(HashMap::new()),
            tombstones: Mutex::new(Tombstones::new(max_tombstones())),
        }
    }

    /// Get a reference to the underlying transport.
    pub fn transport(&self) -> &T {
        &self.transport
    }

    fn is_tombstoned(&self, channel_id: u32) -> bool {
        self.tombstones.lock().contains(channel_id)
    }

    fn tombstone(&self, channel_id: u32) {
        self.tombstones.lock().insert(
            channel_id,
            TombstoneInfo {
                lifecycle: ChannelLifecycle::Closed,
                cancelled: false,
            },
        );
    }

    fn tombstone_cancelled(&self, channel_id: u32) {
        self.tombstones.lock().insert(
            channel_id,
            TombstoneInfo {
                lifecycle: ChannelLifecycle::Closed,
                cancelled: true,
            },
        );
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
            if self.is_tombstoned(channel_id) {
                let mut channels = self.channels.lock();
                channels.remove(&channel_id);
                tracing::trace!(channel_id, "dropping send on tombstoned channel");
                return Ok(());
            }

            let mut channels = self.channels.lock();

            if !channels.contains_key(&channel_id) && channels.len() >= max_tracked_channels() {
                tracing::warn!(
                    channel_id,
                    tracked_channels = channels.len(),
                    max_tracked_channels = max_tracked_channels(),
                    "too many tracked channels; refusing send"
                );
                return Err(RpcError::Status {
                    code: ErrorCode::ResourceExhausted,
                    message: "too many tracked channels".into(),
                });
            }
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

            let should_remove = state.lifecycle == ChannelLifecycle::Closed || state.cancelled;
            let cancelled = state.cancelled;
            if should_remove {
                channels.remove(&channel_id);
            }

            drop(channels);

            if should_remove {
                if cancelled {
                    self.tombstone_cancelled(channel_id);
                } else {
                    self.tombstone(channel_id);
                }
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

            if channel_id != 0 && self.is_tombstoned(channel_id) {
                let mut channels = self.channels.lock();
                channels.remove(&channel_id);
                tracing::trace!(channel_id, "dropping recv on tombstoned channel");
                continue;
            }

            // Check if this channel can receive and update state
            let mut drop_frame = false;
            let mut should_tombstone = false;
            let mut should_tombstone_cancelled = false;
            {
                let mut channels = self.channels.lock();
                if !channels.contains_key(&channel_id) && channels.len() >= max_tracked_channels() {
                    tracing::warn!(
                        channel_id,
                        tracked_channels = channels.len(),
                        max_tracked_channels = max_tracked_channels(),
                        "too many tracked channels; dropping incoming frame"
                    );
                    drop_frame = true;
                    should_tombstone = true;
                } else {
                    let state = channels.entry(channel_id).or_default();
                    if !state.can_receive() {
                        drop_frame = true;
                    } else {
                        // Track received frame
                        if frame.desc.flags.contains(FrameFlags::DATA) {
                            state.frames_received += 1;
                        }

                        // Track EOS from peer
                        if has_eos {
                            state.mark_remote_eos();
                        }

                        if state.lifecycle == ChannelLifecycle::Closed || state.cancelled {
                            should_tombstone_cancelled = state.cancelled;
                            channels.remove(&channel_id);
                            should_tombstone = true;
                        }
                    }
                }
            }

            if should_tombstone {
                if should_tombstone_cancelled {
                    self.tombstone_cancelled(channel_id);
                } else {
                    self.tombstone(channel_id);
                }
            }

            if drop_frame {
                continue;
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
        if channel_id == 0 {
            return;
        }

        {
            let mut channels = self.channels.lock();
            let _ = channels.remove(&channel_id);
        }
        self.tombstone_cancelled(channel_id);
    }

    /// Check if a channel is cancelled.
    pub fn is_cancelled(&self, channel_id: u32) -> bool {
        let channels = self.channels.lock();
        if let Some(state) = channels.get(&channel_id) {
            return state.cancelled;
        }
        drop(channels);
        self.tombstones
            .lock()
            .get(channel_id)
            .map(|t| t.cancelled)
            .unwrap_or(false)
    }

    /// Get available send credits for a channel.
    pub fn get_credits(&self, channel_id: u32) -> u32 {
        let channels = self.channels.lock();
        if let Some(state) = channels.get(&channel_id) {
            return state.send_credits;
        }
        drop(channels);
        if channel_id != 0 && self.is_tombstoned(channel_id) {
            return 0;
        }
        DEFAULT_INITIAL_CREDITS
    }

    /// Get the lifecycle state of a channel.
    pub fn get_lifecycle(&self, channel_id: u32) -> ChannelLifecycle {
        let channels = self.channels.lock();
        if let Some(state) = channels.get(&channel_id) {
            return state.lifecycle;
        }
        drop(channels);
        self.tombstones
            .lock()
            .get(channel_id)
            .map(|t| t.lifecycle)
            .unwrap_or(ChannelLifecycle::Open)
    }

    /// Get a snapshot of the channel state.
    pub fn get_channel_state(&self, channel_id: u32) -> ChannelState {
        let channels = self.channels.lock();
        if let Some(state) = channels.get(&channel_id) {
            return state.clone();
        }
        drop(channels);
        if let Some(t) = self.tombstones.lock().get(channel_id).copied() {
            return ChannelState {
                lifecycle: t.lifecycle,
                send_credits: 0,
                cancelled: t.cancelled,
                frames_sent: 0,
                frames_received: 0,
            };
        }
        ChannelState::default()
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
    use web_time::Instant;
    static START: std::sync::OnceLock<Instant> = std::sync::OnceLock::new();
    let start = START.get_or_init(Instant::now);
    start.elapsed().as_nanos() as u64
}

#[cfg(test)]
mod tests {
    use super::*;
    use rapace_core::control_method;
    use rapace_transport_mem::InProcTransport;

    fn data_eos_frame(channel_id: u32) -> Frame {
        let mut desc = MsgDescHot::new();
        desc.channel_id = channel_id;
        desc.flags = FrameFlags::DATA | FrameFlags::EOS;
        Frame::with_inline_payload(desc, &[]).expect("empty payload should fit inline")
    }

    fn ping_frame() -> Frame {
        let mut desc = MsgDescHot::new();
        desc.channel_id = 0;
        desc.method_id = control_method::PING;
        desc.flags = FrameFlags::CONTROL;
        Frame::with_inline_payload(desc, &[0u8; 8]).expect("ping payload should fit inline")
    }

    fn cancel_frame(channel_id: u32) -> Frame {
        let payload = ControlPayload::CancelChannel {
            channel_id,
            reason: rapace_core::CancelReason::ClientCancel,
        };
        let bytes = facet_postcard::to_vec(&payload).expect("cancel payload should serialize");
        let mut desc = MsgDescHot::new();
        desc.channel_id = 0;
        desc.method_id = control_method::CANCEL_CHANNEL;
        desc.flags = FrameFlags::CONTROL;
        Frame::with_inline_payload(desc, &bytes).expect("cancel payload should fit inline")
    }

    #[tokio::test]
    async fn test_closed_channel_is_pruned_and_tombstoned() {
        let (a, b) = InProcTransport::pair();
        let session = Session::new(Arc::new(a));

        // Local EOS: Open -> HalfClosedLocal (state created)
        session.send_frame(&data_eos_frame(2)).await.unwrap();
        assert_eq!(session.channels.lock().len(), 1);

        // Remote EOS: HalfClosedLocal -> Closed (state removed + tombstoned)
        b.send_frame(&data_eos_frame(2)).await.unwrap();
        let _ = session.recv_frame().await.unwrap();
        assert_eq!(session.channels.lock().len(), 0);
        assert!(session.tombstones.lock().contains(2));
    }

    #[tokio::test]
    async fn test_late_frames_on_closed_channel_are_dropped() {
        let (a, b) = InProcTransport::pair();
        let session = Session::new(Arc::new(a));

        // Close channel 2
        session.send_frame(&data_eos_frame(2)).await.unwrap();
        b.send_frame(&data_eos_frame(2)).await.unwrap();
        let _ = session.recv_frame().await.unwrap();
        assert!(session.tombstones.lock().contains(2));

        // Send a late DATA frame on closed channel 2, then a PING.
        let mut late_desc = MsgDescHot::new();
        late_desc.channel_id = 2;
        late_desc.flags = FrameFlags::DATA;
        let late = Frame::with_inline_payload(late_desc, &[1, 2, 3]).unwrap();
        b.send_frame(&late).await.unwrap();
        b.send_frame(&ping_frame()).await.unwrap();

        // recv_frame should skip the late frame and return the ping.
        let got = session.recv_frame().await.unwrap();
        assert_eq!(got.desc.channel_id, 0);
        assert_eq!(got.desc.method_id, control_method::PING);
    }

    #[tokio::test]
    async fn test_cancelled_channel_is_tombstoned_and_drops_late_frames() {
        let (a, b) = InProcTransport::pair();
        let session = Session::new(Arc::new(a));

        b.send_frame(&cancel_frame(2)).await.unwrap();
        let got = session.recv_frame().await.unwrap();
        assert_eq!(got.desc.channel_id, 0);
        assert_eq!(got.desc.method_id, control_method::CANCEL_CHANNEL);

        assert!(session.is_cancelled(2));

        // Send a late DATA frame on cancelled channel 2, then a PING.
        let mut late_desc = MsgDescHot::new();
        late_desc.channel_id = 2;
        late_desc.flags = FrameFlags::DATA;
        let late = Frame::with_inline_payload(late_desc, &[1, 2, 3]).unwrap();
        b.send_frame(&late).await.unwrap();
        b.send_frame(&ping_frame()).await.unwrap();

        // recv_frame should skip the late frame and return the ping.
        let got = session.recv_frame().await.unwrap();
        assert_eq!(got.desc.channel_id, 0);
        assert_eq!(got.desc.method_id, control_method::PING);
    }
}
