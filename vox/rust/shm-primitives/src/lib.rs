#![doc = include_str!("../README.md")]
#![no_std]

#[cfg(any(test, feature = "alloc"))]
extern crate alloc;
#[cfg(any(test, feature = "std"))]
extern crate std;

pub mod region;
pub mod slot;
pub mod spsc;
pub mod sync;
pub mod treiber;

#[cfg(any(test, feature = "alloc"))]
pub use region::HeapRegion;
pub use region::Region;
pub use slot::{SlotMeta, SlotState};
pub use spsc::{
    PushResult, RingFull, SpscConsumer, SpscProducer, SpscRing, SpscRingHeader, SpscRingRaw,
};
pub use treiber::{
    AllocResult, FreeError, SlotError, SlotHandle, TreiberSlab, TreiberSlabHeader, TreiberSlabRaw,
};

// OS-level primitives for SHM (requires std)
#[cfg(all(feature = "std", unix))]
pub mod doorbell;
#[cfg(feature = "std")]
pub mod futex;

#[cfg(all(feature = "std", unix))]
pub use doorbell::{Doorbell, SignalResult, close_peer_fd, set_nonblocking, validate_fd};
#[cfg(feature = "std")]
pub use futex::{futex_signal, futex_wait, futex_wait_async, futex_wait_async_ptr, futex_wake};

#[cfg(all(test, loom))]
mod loom_tests;
