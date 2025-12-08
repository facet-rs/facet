// src/flow.rs

use std::sync::atomic::{AtomicU32, Ordering};
use crate::types::ByteLen;

/// Credit pool for flow control.
pub struct Credits {
    available: AtomicU32,
    initial: u32,
}

/// A permit representing reserved credits. Released on drop if not consumed.
pub struct CreditPermit<'a> {
    credits: &'a Credits,
    amount: u32,
    consumed: bool,
}

/// Error when credits are insufficient
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct InsufficientCredits {
    pub needed: u32,
    pub available: u32,
}

impl Credits {
    /// Create a new credit pool with initial credits
    pub fn new(initial: u32) -> Self {
        Credits {
            available: AtomicU32::new(initial),
            initial,
        }
    }

    /// Get the initial credit amount
    pub fn initial(&self) -> u32 {
        self.initial
    }

    /// Get currently available credits
    pub fn available(&self) -> u32 {
        self.available.load(Ordering::Acquire)
    }

    /// Try to reserve credits. Returns Err if insufficient.
    pub fn try_reserve(&self, needed: ByteLen) -> Result<CreditPermit<'_>, InsufficientCredits> {
        let needed = needed.get();
        loop {
            let current = self.available.load(Ordering::Acquire);
            if current < needed {
                return Err(InsufficientCredits {
                    needed,
                    available: current,
                });
            }
            if self.available.compare_exchange_weak(
                current,
                current - needed,
                Ordering::AcqRel,
                Ordering::Relaxed,
            ).is_ok() {
                return Ok(CreditPermit {
                    credits: self,
                    amount: needed,
                    consumed: false,
                });
            }
            // CAS failed, retry
        }
    }

    /// Reserve credits, blocking if necessary (for async, use try_reserve + wait)
    pub fn reserve_blocking(&self, needed: ByteLen) -> CreditPermit<'_> {
        loop {
            match self.try_reserve(needed) {
                Ok(permit) => return permit,
                Err(_) => {
                    // Spin wait - in real code, use a proper wait mechanism
                    std::hint::spin_loop();
                }
            }
        }
    }

    /// Add credits (called when receiving CREDITS frame from peer).
    pub fn grant(&self, amount: u32) {
        self.available.fetch_add(amount, Ordering::Release);
    }

    /// Check if there are enough credits for a given amount
    pub fn has_credits(&self, needed: u32) -> bool {
        self.available.load(Ordering::Acquire) >= needed
    }

    /// Reset to initial credits
    pub fn reset(&self) {
        self.available.store(self.initial, Ordering::Release);
    }
}

impl<'a> CreditPermit<'a> {
    /// Get the amount of credits reserved
    pub fn amount(&self) -> u32 {
        self.amount
    }

    /// Mark credits as consumed (data was sent successfully).
    /// After this, credits will NOT be returned on drop.
    pub fn consume(mut self) {
        self.consumed = true;
    }

    /// Manually release the permit (returns credits to pool)
    pub fn release(self) {
        // Drop will handle it since consumed is false
    }
}

impl Drop for CreditPermit<'_> {
    fn drop(&mut self) {
        if !self.consumed {
            // Return credits if not used (e.g., send failed)
            self.credits.grant(self.amount);
        }
    }
}

/// Per-channel flow control state for the sender side
pub struct ChannelFlowSender {
    credits: Credits,
}

/// Per-channel flow control state for the receiver side
pub struct ChannelFlowReceiver {
    /// Bytes consumed since last credit grant
    consumed_bytes: AtomicU32,
    /// Grant credits when consumed exceeds this threshold
    threshold: u32,
    /// Amount to grant each time
    grant_amount: u32,
}

impl ChannelFlowSender {
    /// Create new sender-side flow control
    pub fn new(initial_credits: u32) -> Self {
        ChannelFlowSender {
            credits: Credits::new(initial_credits),
        }
    }

    /// Try to acquire credits for sending
    pub fn try_acquire(&self, len: ByteLen) -> Result<CreditPermit<'_>, InsufficientCredits> {
        self.credits.try_reserve(len)
    }

    /// Called when peer grants us more credits
    pub fn on_credits_received(&self, amount: u32) {
        self.credits.grant(amount);
    }

    /// Get available credits
    pub fn available(&self) -> u32 {
        self.credits.available()
    }
}

impl ChannelFlowReceiver {
    /// Create new receiver-side flow control
    ///
    /// - `threshold`: Grant credits when consumed bytes exceed this
    /// - `grant_amount`: Amount of credits to grant each time
    pub fn new(threshold: u32, grant_amount: u32) -> Self {
        ChannelFlowReceiver {
            consumed_bytes: AtomicU32::new(0),
            threshold,
            grant_amount,
        }
    }

    /// Record that we consumed some bytes. Returns Some(credits) if we should grant.
    pub fn on_bytes_consumed(&self, len: u32) -> Option<u32> {
        let prev = self.consumed_bytes.fetch_add(len, Ordering::AcqRel);
        let new_total = prev + len;

        if new_total >= self.threshold {
            // Reset counter and return grant amount
            self.consumed_bytes.store(0, Ordering::Release);
            Some(self.grant_amount)
        } else {
            None
        }
    }

    /// Get current consumed bytes count
    pub fn consumed(&self) -> u32 {
        self.consumed_bytes.load(Ordering::Acquire)
    }

    /// Get the threshold
    pub fn threshold(&self) -> u32 {
        self.threshold
    }
}

/// Default flow control parameters
pub mod defaults {
    /// Default initial credits per channel (64KB)
    pub const INITIAL_CREDITS: u32 = 64 * 1024;

    /// Default threshold for granting credits (32KB consumed)
    pub const CREDIT_THRESHOLD: u32 = 32 * 1024;

    /// Default amount to grant each time (32KB)
    pub const CREDIT_GRANT: u32 = 32 * 1024;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reserve_and_consume() {
        let credits = Credits::new(1000);

        let permit = credits.try_reserve(ByteLen::new(100, 1000).unwrap()).unwrap();
        assert_eq!(credits.available(), 900);

        permit.consume();
        assert_eq!(credits.available(), 900); // Not returned
    }

    #[test]
    fn reserve_and_drop_returns_credits() {
        let credits = Credits::new(1000);

        {
            let _permit = credits.try_reserve(ByteLen::new(100, 1000).unwrap()).unwrap();
            assert_eq!(credits.available(), 900);
            // Dropped without consume
        }

        assert_eq!(credits.available(), 1000); // Returned
    }

    #[test]
    fn insufficient_credits() {
        let credits = Credits::new(50);

        let result = credits.try_reserve(ByteLen::new(100, 1000).unwrap());
        assert_eq!(result.err(), Some(InsufficientCredits {
            needed: 100,
            available: 50,
        }));
    }

    #[test]
    fn grant_credits() {
        let credits = Credits::new(100);
        credits.grant(50);
        assert_eq!(credits.available(), 150);
    }

    #[test]
    fn channel_flow_sender() {
        let sender = ChannelFlowSender::new(1000);

        let permit = sender.try_acquire(ByteLen::new(500, 1000).unwrap()).unwrap();
        assert_eq!(sender.available(), 500);

        permit.consume();

        sender.on_credits_received(200);
        assert_eq!(sender.available(), 700);
    }

    #[test]
    fn channel_flow_receiver() {
        let receiver = ChannelFlowReceiver::new(100, 100);

        // Consume less than threshold
        assert_eq!(receiver.on_bytes_consumed(50), None);
        assert_eq!(receiver.consumed(), 50);

        // Consume past threshold
        let grant = receiver.on_bytes_consumed(60);
        assert_eq!(grant, Some(100));
        assert_eq!(receiver.consumed(), 0); // Reset
    }

    #[test]
    fn multiple_reserves() {
        let credits = Credits::new(100);

        let p1 = credits.try_reserve(ByteLen::new(30, 100).unwrap()).unwrap();
        let p2 = credits.try_reserve(ByteLen::new(30, 100).unwrap()).unwrap();
        let p3 = credits.try_reserve(ByteLen::new(30, 100).unwrap()).unwrap();

        assert_eq!(credits.available(), 10);

        // Fourth should fail
        assert!(credits.try_reserve(ByteLen::new(20, 100).unwrap()).is_err());

        p1.consume();
        p2.consume();
        drop(p3); // Return 30

        assert_eq!(credits.available(), 40);
    }
}
