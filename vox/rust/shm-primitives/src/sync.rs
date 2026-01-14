#[cfg(not(loom))]
pub use core::hint::spin_loop;
#[cfg(loom)]
pub use loom::hint::spin_loop;

#[cfg(not(loom))]
pub use core::sync::atomic::{AtomicU32, AtomicU64, Ordering};
#[cfg(loom)]
pub use loom::sync::atomic::{AtomicU32, AtomicU64, Ordering};

#[cfg(loom)]
pub use loom::thread;
#[cfg(not(loom))]
pub use std::thread;
