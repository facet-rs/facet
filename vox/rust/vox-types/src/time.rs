//! Cross-platform time primitives.

pub use vox_rt::time::Instant;

/// Runtime time primitives.
///
/// This module keeps the historical path used by Vox internals while delegating
/// to `vox-rt`, whose wasm implementation does not depend on Tokio.
pub mod tokio {
    pub use vox_rt::time::{Instant, MissedTickBehavior, interval, sleep, timeout};
}
