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

#[cfg(all(test, loom))]
mod loom_tests;
