#![doc = include_str!("../README.md")]

pub mod region;
pub mod slot;
pub mod spsc;
pub mod sync;
pub mod treiber;

pub use region::HeapRegion;
pub use region::Region;
pub use slot::{SlotMeta, SlotState, VarSlotMeta};
pub use spsc::{
    PushResult, RingFull, SpscConsumer, SpscProducer, SpscRing, SpscRingHeader, SpscRingRaw,
};
pub use treiber::{
    AllocResult, FreeError, SlotError, SlotHandle, TreiberSlab, TreiberSlabHeader, TreiberSlabRaw,
};

// OS-level primitives for SHM
pub mod futex;

#[cfg(unix)]
mod unix;
#[cfg(unix)]
pub use unix::*;

#[cfg(windows)]
mod windows;
#[cfg(windows)]
pub use windows::*;

pub use futex::{futex_signal, futex_wait, futex_wait_async, futex_wait_async_ptr, futex_wake};

#[cfg(all(test, loom))]
mod loom_tests;
