#[cfg(not(feature = "loom"))]
pub use core::hint::spin_loop;
#[cfg(feature = "loom")]
pub use loom::hint::spin_loop;

#[cfg(not(feature = "loom"))]
pub use core::sync::atomic::{AtomicU32, AtomicU64, Ordering};
#[cfg(feature = "loom")]
pub use loom::sync::atomic::{AtomicU32, AtomicU64, Ordering};

#[cfg(feature = "loom")]
pub use loom::thread;
#[cfg(all(not(feature = "loom"), any(test, feature = "std")))]
pub use std::thread;
