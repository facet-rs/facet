//! In-memory model of Session channel state for property-based testing.
//!
//! This module provides a pure Rust model of the Session's per-channel state
//! that can be fuzzed without touching real transports.

use std::collections::HashMap;

/// Default initial credits for new channels (64KB).
pub const DEFAULT_INITIAL_CREDITS: u32 = 65536;

/// Channel lifecycle state (matches rapace-testkit::session).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ChannelLifecycle {
    Open,
    HalfClosedLocal,
    HalfClosedRemote,
    Closed,
}

impl Default for ChannelLifecycle {
    fn default() -> Self {
        Self::Open
    }
}

/// Model of ChannelState.
#[derive(Debug, Clone)]
pub struct ChannelStateModel {
    pub lifecycle: ChannelLifecycle,
    pub send_credits: u32,
    pub cancelled: bool,
    pub frames_sent: u64,
    pub frames_received: u64,
}

impl Default for ChannelStateModel {
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

impl ChannelStateModel {
    pub fn can_send(&self) -> bool {
        !self.cancelled
            && matches!(
                self.lifecycle,
                ChannelLifecycle::Open | ChannelLifecycle::HalfClosedRemote
            )
    }

    pub fn can_receive(&self) -> bool {
        !self.cancelled
            && matches!(
                self.lifecycle,
                ChannelLifecycle::Open | ChannelLifecycle::HalfClosedLocal
            )
    }

    pub fn mark_local_eos(&mut self) {
        self.lifecycle = match self.lifecycle {
            ChannelLifecycle::Open => ChannelLifecycle::HalfClosedLocal,
            ChannelLifecycle::HalfClosedRemote => ChannelLifecycle::Closed,
            other => other,
        };
    }

    pub fn mark_remote_eos(&mut self) {
        self.lifecycle = match self.lifecycle {
            ChannelLifecycle::Open => ChannelLifecycle::HalfClosedRemote,
            ChannelLifecycle::HalfClosedLocal => ChannelLifecycle::Closed,
            other => other,
        };
    }
}

/// Model of Session's channel map.
pub struct SessionModel {
    channels: HashMap<u32, ChannelStateModel>,
}

impl SessionModel {
    pub fn new() -> Self {
        Self {
            channels: HashMap::new(),
        }
    }

    /// Get or create a channel state.
    pub fn get_or_create(&mut self, channel_id: u32) -> &mut ChannelStateModel {
        self.channels.entry(channel_id).or_default()
    }

    /// Get channel state if it exists.
    pub fn get(&self, channel_id: u32) -> Option<&ChannelStateModel> {
        self.channels.get(&channel_id)
    }

    /// Try to send a data frame (returns error if insufficient credits).
    ///
    /// Returns true if frame was sent, false if dropped (cancelled/closed).
    pub fn try_send(&mut self, channel_id: u32, payload_len: u32, eos: bool) -> Result<bool, SendError> {
        // Control channel (0) is exempt
        if channel_id == 0 {
            return Ok(true);
        }

        let state = self.get_or_create(channel_id);

        // Check if we can send
        if !state.can_send() {
            // Silently dropped
            return Ok(false);
        }

        // Check credits
        if payload_len > state.send_credits {
            return Err(SendError::InsufficientCredits {
                need: payload_len,
                have: state.send_credits,
            });
        }

        // Deduct credits
        state.send_credits -= payload_len;
        state.frames_sent += 1;

        // Track EOS
        if eos {
            state.mark_local_eos();
        }

        Ok(true)
    }

    /// Receive a data frame from peer.
    ///
    /// Returns true if frame was delivered, false if dropped (cancelled/closed).
    pub fn recv(&mut self, channel_id: u32, eos: bool) -> bool {
        // Control channel is always delivered
        if channel_id == 0 {
            return true;
        }

        let state = self.get_or_create(channel_id);

        // Check if we can receive
        if !state.can_receive() {
            // Frame dropped
            return false;
        }

        state.frames_received += 1;

        // Track EOS from peer
        if eos {
            state.mark_remote_eos();
        }

        true
    }

    /// Grant credits to a channel.
    pub fn grant_credits(&mut self, channel_id: u32, bytes: u32) {
        let state = self.get_or_create(channel_id);
        state.send_credits = state.send_credits.saturating_add(bytes);
    }

    /// Cancel a channel.
    pub fn cancel(&mut self, channel_id: u32) {
        let state = self.get_or_create(channel_id);
        state.cancelled = true;
    }

    /// Check if a channel is cancelled.
    pub fn is_cancelled(&self, channel_id: u32) -> bool {
        self.channels
            .get(&channel_id)
            .map(|s| s.cancelled)
            .unwrap_or(false)
    }

    /// Get send credits for a channel.
    pub fn get_credits(&self, channel_id: u32) -> u32 {
        self.channels
            .get(&channel_id)
            .map(|s| s.send_credits)
            .unwrap_or(DEFAULT_INITIAL_CREDITS)
    }

    /// Get lifecycle state.
    pub fn get_lifecycle(&self, channel_id: u32) -> ChannelLifecycle {
        self.channels
            .get(&channel_id)
            .map(|s| s.lifecycle)
            .unwrap_or(ChannelLifecycle::Open)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SendError {
    InsufficientCredits { need: u32, have: u32 },
}

/// Operations on session channels.
#[derive(Clone, Debug)]
pub enum SessionOp {
    /// Try to send a data frame with given payload size and optional EOS.
    Send { channel_id: u8, payload_len: u16, eos: bool },
    /// Receive a data frame with optional EOS.
    Recv { channel_id: u8, eos: bool },
    /// Grant credits to a channel.
    GrantCredits { channel_id: u8, bytes: u16 },
    /// Cancel a channel.
    Cancel { channel_id: u8 },
}

/// Execute a sequence of operations and verify invariants.
pub fn execute_and_verify(ops: &[SessionOp]) -> Result<(), String> {
    let mut session = SessionModel::new();

    for (i, op) in ops.iter().enumerate() {
        // Capture state before operation for invariant checking
        let channel_ids: Vec<u32> = match op {
            SessionOp::Send { channel_id, .. } => vec![*channel_id as u32],
            SessionOp::Recv { channel_id, .. } => vec![*channel_id as u32],
            SessionOp::GrantCredits { channel_id, .. } => vec![*channel_id as u32],
            SessionOp::Cancel { channel_id } => vec![*channel_id as u32],
        };

        // Execute operation
        match op {
            SessionOp::Send { channel_id, payload_len, eos } => {
                let channel = *channel_id as u32;
                let len = *payload_len as u32;

                // Capture credits before
                let credits_before = session.get_credits(channel);
                let lifecycle_before = session.get_lifecycle(channel);
                let cancelled_before = session.is_cancelled(channel);

                let result = session.try_send(channel, len, *eos);

                // Verify invariants
                let credits_after = session.get_credits(channel);
                let lifecycle_after = session.get_lifecycle(channel);

                // INVARIANT: Credits never go negative (implied by u32)
                // Already enforced by type system

                match result {
                    Ok(true) => {
                        // Frame was sent
                        // INVARIANT: Credits should be deducted by payload_len
                        if channel != 0 {
                            let expected = credits_before.saturating_sub(len);
                            if credits_after != expected {
                                return Err(format!(
                                    "op {}: credits mismatch after send: expected {}, got {}",
                                    i, expected, credits_after
                                ));
                            }
                        }

                        // INVARIANT: If EOS, lifecycle should transition
                        if *eos && channel != 0 {
                            let valid_transition = match (lifecycle_before, lifecycle_after) {
                                (ChannelLifecycle::Open, ChannelLifecycle::HalfClosedLocal) => true,
                                (ChannelLifecycle::HalfClosedRemote, ChannelLifecycle::Closed) => true,
                                (ChannelLifecycle::HalfClosedLocal, ChannelLifecycle::HalfClosedLocal) => true,
                                (ChannelLifecycle::Closed, ChannelLifecycle::Closed) => true,
                                _ => false,
                            };
                            if !valid_transition {
                                return Err(format!(
                                    "op {}: invalid lifecycle transition after send EOS: {:?} -> {:?}",
                                    i, lifecycle_before, lifecycle_after
                                ));
                            }
                        }
                    }
                    Ok(false) => {
                        // Frame was dropped (cancelled/closed)
                        // INVARIANT: Credits should not change
                        if credits_after != credits_before {
                            return Err(format!(
                                "op {}: credits changed for dropped frame: {} -> {}",
                                i, credits_before, credits_after
                            ));
                        }

                        // INVARIANT: Must be cancelled or in a state that can't send
                        if !cancelled_before && lifecycle_before != ChannelLifecycle::HalfClosedLocal
                            && lifecycle_before != ChannelLifecycle::Closed
                        {
                            return Err(format!(
                                "op {}: frame dropped but channel was sendable: cancelled={}, lifecycle={:?}",
                                i, cancelled_before, lifecycle_before
                            ));
                        }
                    }
                    Err(SendError::InsufficientCredits { need, have }) => {
                        // INVARIANT: Should only fail if payload > credits
                        if len <= credits_before {
                            return Err(format!(
                                "op {}: InsufficientCredits but payload {} <= credits {}",
                                i, len, credits_before
                            ));
                        }
                        // INVARIANT: Error should report correct values
                        if need != len || have != credits_before {
                            return Err(format!(
                                "op {}: InsufficientCredits reports need={}, have={}, but actual payload={}, credits={}",
                                i, need, have, len, credits_before
                            ));
                        }
                        // INVARIANT: Credits should not change on error
                        if credits_after != credits_before {
                            return Err(format!(
                                "op {}: credits changed on error: {} -> {}",
                                i, credits_before, credits_after
                            ));
                        }
                    }
                }
            }
            SessionOp::Recv { channel_id, eos } => {
                let channel = *channel_id as u32;
                let lifecycle_before = session.get_lifecycle(channel);
                let cancelled_before = session.is_cancelled(channel);

                let delivered = session.recv(channel, *eos);

                let lifecycle_after = session.get_lifecycle(channel);

                if delivered {
                    // INVARIANT: If EOS, lifecycle should transition
                    if *eos && channel != 0 {
                        let valid_transition = match (lifecycle_before, lifecycle_after) {
                            (ChannelLifecycle::Open, ChannelLifecycle::HalfClosedRemote) => true,
                            (ChannelLifecycle::HalfClosedLocal, ChannelLifecycle::Closed) => true,
                            (ChannelLifecycle::HalfClosedRemote, ChannelLifecycle::HalfClosedRemote) => true,
                            (ChannelLifecycle::Closed, ChannelLifecycle::Closed) => true,
                            _ => false,
                        };
                        if !valid_transition {
                            return Err(format!(
                                "op {}: invalid lifecycle transition after recv EOS: {:?} -> {:?}",
                                i, lifecycle_before, lifecycle_after
                            ));
                        }
                    }
                } else {
                    // Frame was dropped
                    // INVARIANT: Must be cancelled or in a state that can't receive
                    if !cancelled_before && lifecycle_before != ChannelLifecycle::HalfClosedRemote
                        && lifecycle_before != ChannelLifecycle::Closed
                    {
                        return Err(format!(
                            "op {}: recv dropped but channel was receivable: cancelled={}, lifecycle={:?}",
                            i, cancelled_before, lifecycle_before
                        ));
                    }
                }
            }
            SessionOp::GrantCredits { channel_id, bytes } => {
                let channel = *channel_id as u32;
                let credits_before = session.get_credits(channel);

                session.grant_credits(channel, *bytes as u32);

                let credits_after = session.get_credits(channel);

                // INVARIANT: Credits should increase by bytes (saturating)
                let expected = credits_before.saturating_add(*bytes as u32);
                if credits_after != expected {
                    return Err(format!(
                        "op {}: credits after grant: expected {}, got {}",
                        i, expected, credits_after
                    ));
                }
            }
            SessionOp::Cancel { channel_id } => {
                let channel = *channel_id as u32;

                session.cancel(channel);

                // INVARIANT: Channel must be cancelled after cancel
                if !session.is_cancelled(channel) {
                    return Err(format!(
                        "op {}: channel {} not cancelled after cancel",
                        i, channel
                    ));
                }
            }
        }

        // Global invariants after each operation
        for &channel_id in &channel_ids {
            if let Some(state) = session.get(channel_id) {
                // INVARIANT: Once cancelled, stays cancelled
                // (Already guaranteed by code, but let's verify)

                // INVARIANT: Lifecycle transitions are valid
                // (Already checked per-operation)

                // INVARIANT: frames_sent >= 0 and frames_received >= 0
                // (Guaranteed by u64)

                // INVARIANT: Can't send if HalfClosedLocal or Closed (unless cancelled)
                if !state.cancelled {
                    let can_send = state.can_send();
                    let should_be_able_to_send = matches!(
                        state.lifecycle,
                        ChannelLifecycle::Open | ChannelLifecycle::HalfClosedRemote
                    );
                    if can_send != should_be_able_to_send {
                        return Err(format!(
                            "op {}: can_send mismatch for channel {}: can_send={}, lifecycle={:?}",
                            i, channel_id, can_send, state.lifecycle
                        ));
                    }
                }
            }
        }
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_basic_credit_flow() {
        let mut session = SessionModel::new();

        // Initial credits
        assert_eq!(session.get_credits(1), DEFAULT_INITIAL_CREDITS);

        // Send consumes credits
        assert_eq!(session.try_send(1, 1000, false), Ok(true));
        assert_eq!(session.get_credits(1), DEFAULT_INITIAL_CREDITS - 1000);

        // Grant restores credits
        session.grant_credits(1, 500);
        assert_eq!(session.get_credits(1), DEFAULT_INITIAL_CREDITS - 1000 + 500);
    }

    #[test]
    fn test_insufficient_credits() {
        let mut session = SessionModel::new();

        // Exhaust credits
        session.try_send(1, DEFAULT_INITIAL_CREDITS - 100, false).unwrap();

        // Try to send more than available
        let result = session.try_send(1, 200, false);
        assert_eq!(
            result,
            Err(SendError::InsufficientCredits { need: 200, have: 100 })
        );

        // Credits unchanged
        assert_eq!(session.get_credits(1), 100);
    }

    #[test]
    fn test_lifecycle_transitions() {
        let mut session = SessionModel::new();

        // Open -> HalfClosedLocal (local EOS)
        session.try_send(1, 0, true).unwrap();
        assert_eq!(session.get_lifecycle(1), ChannelLifecycle::HalfClosedLocal);

        // Can't send after local EOS
        assert_eq!(session.try_send(1, 0, false), Ok(false));

        // HalfClosedLocal -> Closed (remote EOS)
        assert!(session.recv(1, true));
        assert_eq!(session.get_lifecycle(1), ChannelLifecycle::Closed);
    }

    #[test]
    fn test_cancellation() {
        let mut session = SessionModel::new();

        // Cancel channel
        session.cancel(1);
        assert!(session.is_cancelled(1));

        // Frames are dropped for cancelled channels
        assert_eq!(session.try_send(1, 1000, false), Ok(false));
        assert!(!session.recv(1, false));
    }

    #[test]
    fn test_control_channel_exempt() {
        let mut session = SessionModel::new();

        // Control channel (0) is exempt from credit checks
        // (In the model, we return Ok(true) immediately)
        assert_eq!(session.try_send(0, u16::MAX as u32, false), Ok(true));
    }
}
