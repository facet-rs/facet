// src/fault.rs
//! Fault injection for testing.
//!
//! This module provides facilities for injecting faults (drops, delays, errors)
//! into message processing for testing purposes. Fault injection happens AFTER
//! validation to ensure validation bugs are not masked.
//!
//! # Example
//!
//! ```rust
//! use rapace::fault::{FaultInjector, FaultAction};
//!
//! let injector = FaultInjector::new();
//! injector.set_drop_rate(500); // 5.00% drop rate
//!
//! match injector.check(None) {
//!     FaultAction::Pass => { /* process normally */ }
//!     FaultAction::Drop => { /* drop the frame */ }
//!     FaultAction::Error(code) => { /* inject error */ }
//!     FaultAction::Delay(duration) => { /* delay processing */ }
//! }
//! ```

use crate::error::ErrorCode;
use crate::types::ChannelId;
use std::collections::hash_map::RandomState;
use std::hash::{BuildHasher, Hash, Hasher};
use std::sync::atomic::{AtomicU32, AtomicU64, Ordering};

/// Fault injection configuration.
///
/// IMPORTANT: Fault injection should only be applied AFTER validation
/// to ensure validation bugs are detected during testing.
pub struct FaultInjector {
    /// Global drop rate in basis points (0-10000 = 0.00%-100.00%)
    global_drop_rate: AtomicU32,
    /// Global error injection rate in basis points (0-10000 = 0.00%-100.00%)
    global_error_rate: AtomicU32,
    /// Global delay in milliseconds
    global_delay_ms: AtomicU32,
    /// Counter for generating pseudo-random values
    counter: AtomicU64,
    /// Random state for hashing
    random_state: RandomState,
}

/// Action to take for a frame after checking fault injection rules.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FaultAction {
    /// Process the frame normally.
    Pass,
    /// Drop the frame silently.
    Drop,
    /// Inject an error response.
    Error(ErrorCode),
    /// Delay processing by the specified duration.
    Delay(std::time::Duration),
}

/// Commands for controlling fault injection at runtime.
///
/// These would typically be sent over the control channel to dynamically
/// adjust fault injection parameters during testing.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FaultCommand {
    /// Set the drop rate for a specific channel or globally.
    SetDropRate {
        /// Target channel, or None for global setting.
        channel_id: Option<u32>,
        /// Drop rate in basis points (0-10000 = 0.00%-100.00%)
        rate: u32,
    },
    /// Set the delay for a specific channel or globally.
    SetDelay {
        /// Target channel, or None for global setting.
        channel_id: Option<u32>,
        /// Delay in milliseconds
        delay_ms: u32,
    },
    /// Set the error injection rate for a specific channel or globally.
    SetErrorRate {
        /// Target channel, or None for global setting.
        channel_id: Option<u32>,
        /// Error rate in basis points (0-10000 = 0.00%-100.00%)
        rate: u32,
    },
}

impl FaultInjector {
    /// Create a new fault injector with all faults disabled.
    pub fn new() -> Self {
        FaultInjector {
            global_drop_rate: AtomicU32::new(0),
            global_error_rate: AtomicU32::new(0),
            global_delay_ms: AtomicU32::new(0),
            counter: AtomicU64::new(0),
            random_state: RandomState::new(),
        }
    }

    /// Create a fault injector with all faults explicitly disabled.
    ///
    /// This is equivalent to `new()` but makes the intent clearer in code.
    pub fn disabled() -> Self {
        Self::new()
    }

    /// Check what action to take for this frame.
    ///
    /// IMPORTANT: This MUST be called AFTER validation to ensure that
    /// validation bugs are not masked by fault injection.
    ///
    /// Returns a `FaultAction` indicating whether to process normally,
    /// drop, inject an error, or delay.
    pub fn check(&self, _channel: Option<ChannelId>) -> FaultAction {
        // TODO: In the future, we could use _channel to implement
        // per-channel fault injection overrides using a concurrent map.

        let drop_rate = self.global_drop_rate.load(Ordering::Relaxed);
        if drop_rate > 0 && self.rand_percent() < drop_rate {
            return FaultAction::Drop;
        }

        let error_rate = self.global_error_rate.load(Ordering::Relaxed);
        if error_rate > 0 && self.rand_percent() < error_rate {
            return FaultAction::Error(ErrorCode::Internal);
        }

        let delay_ms = self.global_delay_ms.load(Ordering::Relaxed);
        if delay_ms > 0 {
            return FaultAction::Delay(std::time::Duration::from_millis(delay_ms as u64));
        }

        FaultAction::Pass
    }

    /// Set the global drop rate.
    ///
    /// # Arguments
    /// * `rate` - Drop rate in basis points (0-10000 = 0.00%-100.00%).
    ///            Values above 10000 are clamped to 10000.
    pub fn set_drop_rate(&self, rate: u32) {
        self.global_drop_rate
            .store(rate.min(10000), Ordering::Relaxed);
    }

    /// Set the global error injection rate.
    ///
    /// # Arguments
    /// * `rate` - Error rate in basis points (0-10000 = 0.00%-100.00%).
    ///            Values above 10000 are clamped to 10000.
    pub fn set_error_rate(&self, rate: u32) {
        self.global_error_rate
            .store(rate.min(10000), Ordering::Relaxed);
    }

    /// Set the global delay.
    ///
    /// # Arguments
    /// * `delay_ms` - Delay in milliseconds.
    pub fn set_delay(&self, delay_ms: u32) {
        self.global_delay_ms.store(delay_ms, Ordering::Relaxed);
    }

    /// Get the current global drop rate.
    ///
    /// Returns the drop rate in basis points (0-10000 = 0.00%-100.00%).
    pub fn drop_rate(&self) -> u32 {
        self.global_drop_rate.load(Ordering::Relaxed)
    }

    /// Get the current global error injection rate.
    ///
    /// Returns the error rate in basis points (0-10000 = 0.00%-100.00%).
    pub fn error_rate(&self) -> u32 {
        self.global_error_rate.load(Ordering::Relaxed)
    }

    /// Get the current global delay.
    ///
    /// Returns the delay in milliseconds.
    pub fn delay_ms(&self) -> u32 {
        self.global_delay_ms.load(Ordering::Relaxed)
    }

    /// Generate a pseudo-random value in the range [0, 10000).
    ///
    /// Uses a counter and hash function to generate deterministic but
    /// pseudo-random values without requiring an external RNG dependency.
    fn rand_percent(&self) -> u32 {
        let counter = self.counter.fetch_add(1, Ordering::Relaxed);
        let mut hasher = self.random_state.build_hasher();
        counter.hash(&mut hasher);
        let hash = hasher.finish();
        // Map hash to 0-9999 range
        (hash % 10000) as u32
    }
}

impl Default for FaultInjector {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn new_injector_passes_all() {
        let injector = FaultInjector::new();
        for _ in 0..100 {
            assert_eq!(injector.check(None), FaultAction::Pass);
        }
    }

    #[test]
    fn disabled_injector_passes_all() {
        let injector = FaultInjector::disabled();
        for _ in 0..100 {
            assert_eq!(injector.check(None), FaultAction::Pass);
        }
    }

    #[test]
    fn set_drop_rate_clamps_to_max() {
        let injector = FaultInjector::new();
        injector.set_drop_rate(20000);
        assert_eq!(injector.drop_rate(), 10000);
    }

    #[test]
    fn set_error_rate_clamps_to_max() {
        let injector = FaultInjector::new();
        injector.set_error_rate(20000);
        assert_eq!(injector.error_rate(), 10000);
    }

    #[test]
    fn drop_rate_100_percent_drops_all() {
        let injector = FaultInjector::new();
        injector.set_drop_rate(10000); // 100%

        for _ in 0..100 {
            assert_eq!(injector.check(None), FaultAction::Drop);
        }
    }

    #[test]
    fn error_rate_100_percent_errors_all() {
        let injector = FaultInjector::new();
        injector.set_error_rate(10000); // 100%

        for _ in 0..100 {
            match injector.check(None) {
                FaultAction::Error(ErrorCode::Internal) => (),
                other => panic!("expected Error(Internal), got {:?}", other),
            }
        }
    }

    #[test]
    fn delay_always_delays() {
        let injector = FaultInjector::new();
        injector.set_delay(50);

        for _ in 0..100 {
            match injector.check(None) {
                FaultAction::Delay(d) => {
                    assert_eq!(d.as_millis(), 50);
                }
                other => panic!("expected Delay(50ms), got {:?}", other),
            }
        }
    }

    #[test]
    fn priority_order_drop_before_error() {
        let injector = FaultInjector::new();
        injector.set_drop_rate(10000); // 100%
        injector.set_error_rate(10000); // 100%

        // Drop should take priority over error
        assert_eq!(injector.check(None), FaultAction::Drop);
    }

    #[test]
    fn priority_order_error_before_delay() {
        let injector = FaultInjector::new();
        injector.set_error_rate(10000); // 100%
        injector.set_delay(50);

        // Error should take priority over delay
        match injector.check(None) {
            FaultAction::Error(ErrorCode::Internal) => (),
            other => panic!("expected Error(Internal), got {:?}", other),
        }
    }

    #[test]
    fn drop_rate_is_statistical() {
        let injector = FaultInjector::new();
        injector.set_drop_rate(5000); // 50%

        let mut drops = 0;
        let mut passes = 0;
        let trials = 1000;

        for _ in 0..trials {
            match injector.check(None) {
                FaultAction::Drop => drops += 1,
                FaultAction::Pass => passes += 1,
                other => panic!("unexpected action: {:?}", other),
            }
        }

        // With 50% drop rate and 1000 trials, we expect roughly 500 drops.
        // Allow for statistical variance: 400-600 drops should be reasonable.
        assert!(
            drops >= 400 && drops <= 600,
            "expected ~500 drops with 50% rate, got {}",
            drops
        );
        assert_eq!(drops + passes, trials);
    }

    #[test]
    fn error_rate_is_statistical() {
        let injector = FaultInjector::new();
        injector.set_error_rate(5000); // 50%

        let mut errors = 0;
        let mut passes = 0;
        let trials = 1000;

        for _ in 0..trials {
            match injector.check(None) {
                FaultAction::Error(ErrorCode::Internal) => errors += 1,
                FaultAction::Pass => passes += 1,
                other => panic!("unexpected action: {:?}", other),
            }
        }

        // With 50% error rate and 1000 trials, we expect roughly 500 errors.
        // Allow for statistical variance: 400-600 errors should be reasonable.
        assert!(
            errors >= 400 && errors <= 600,
            "expected ~500 errors with 50% rate, got {}",
            errors
        );
        assert_eq!(errors + passes, trials);
    }

    #[test]
    fn getters_return_set_values() {
        let injector = FaultInjector::new();

        injector.set_drop_rate(1234);
        assert_eq!(injector.drop_rate(), 1234);

        injector.set_error_rate(5678);
        assert_eq!(injector.error_rate(), 5678);

        injector.set_delay(100);
        assert_eq!(injector.delay_ms(), 100);
    }

    #[test]
    fn zero_rates_always_pass() {
        let injector = FaultInjector::new();
        injector.set_drop_rate(0);
        injector.set_error_rate(0);
        injector.set_delay(0);

        for _ in 0..100 {
            assert_eq!(injector.check(None), FaultAction::Pass);
        }
    }

    #[test]
    fn fault_action_equality() {
        assert_eq!(FaultAction::Pass, FaultAction::Pass);
        assert_eq!(FaultAction::Drop, FaultAction::Drop);
        assert_eq!(
            FaultAction::Error(ErrorCode::Internal),
            FaultAction::Error(ErrorCode::Internal)
        );
        assert_eq!(
            FaultAction::Delay(std::time::Duration::from_millis(50)),
            FaultAction::Delay(std::time::Duration::from_millis(50))
        );

        assert_ne!(FaultAction::Pass, FaultAction::Drop);
        assert_ne!(
            FaultAction::Error(ErrorCode::Internal),
            FaultAction::Error(ErrorCode::Cancelled)
        );
    }

    #[test]
    fn fault_command_variants() {
        let cmd1 = FaultCommand::SetDropRate {
            channel_id: None,
            rate: 1000,
        };
        let cmd2 = FaultCommand::SetDelay {
            channel_id: Some(1),
            delay_ms: 50,
        };
        let cmd3 = FaultCommand::SetErrorRate {
            channel_id: None,
            rate: 2000,
        };

        // Just verify we can construct and pattern match on them
        match cmd1 {
            FaultCommand::SetDropRate {
                channel_id: None,
                rate: 1000,
            } => (),
            _ => panic!("wrong variant"),
        }
        match cmd2 {
            FaultCommand::SetDelay {
                channel_id: Some(1),
                delay_ms: 50,
            } => (),
            _ => panic!("wrong variant"),
        }
        match cmd3 {
            FaultCommand::SetErrorRate {
                channel_id: None,
                rate: 2000,
            } => (),
            _ => panic!("wrong variant"),
        }
    }
}
