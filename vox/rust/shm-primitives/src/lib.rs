#![doc = include_str!("../README.md")]

pub mod bipbuf;
pub mod region;
pub mod slot;
pub mod spsc;
pub mod sync;
pub mod treiber;

pub use bipbuf::{
    BIPBUF_HEADER_SIZE, BipBuf, BipBufConsumer, BipBufFull, BipBufHeader, BipBufProducer, BipBufRaw,
};
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
#[cfg(unix)]
mod unix;
#[cfg(unix)]
pub use unix::*;

#[cfg(windows)]
mod windows;
#[cfg(windows)]
pub use windows::*;

#[cfg(all(test, loom))]
mod loom_tests;
