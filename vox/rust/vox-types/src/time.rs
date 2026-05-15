//! Cross-platform time primitives.
//!
//! On native targets these are re-exports of `std::time` and `tokio::time`. On
//! `wasm32-unknown-unknown` they come from `wasmtimer`, because both
//! `std::time::Instant::now()` and tokio's time driver are unsupported there.

#[cfg(not(target_arch = "wasm32"))]
pub use std::time::Instant;
#[cfg(target_arch = "wasm32")]
pub use wasmtimer::std::Instant;

/// Tokio time primitives that work on wasm.
///
/// Mirrors the subset of `tokio::time` we actually use.
pub mod tokio {
    #[cfg(not(target_arch = "wasm32"))]
    pub use ::tokio::time::{Instant, MissedTickBehavior, interval, sleep, timeout};
    #[cfg(target_arch = "wasm32")]
    pub use wasmtimer::std::Instant;
    #[cfg(target_arch = "wasm32")]
    pub use wasmtimer::tokio::{MissedTickBehavior, interval, sleep, timeout};
}
