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
#[cfg(unix)]
pub mod doorbell;
#[cfg(windows)]
pub mod doorbell_windows;
pub mod futex;
#[cfg(unix)]
pub mod mmap;
#[cfg(windows)]
pub mod mmap_windows;

#[cfg(unix)]
pub use doorbell::{
    Doorbell, DoorbellHandle, SignalResult, clear_cloexec, close_peer_fd, set_nonblocking,
    validate_fd,
};
#[cfg(windows)]
pub use doorbell_windows::{
    Doorbell, DoorbellHandle, SignalResult, close_handle, set_handle_inheritable, validate_handle,
};
pub use futex::{futex_signal, futex_wait, futex_wait_async, futex_wait_async_ptr, futex_wake};
#[cfg(unix)]
pub use mmap::{FileCleanup, MmapRegion};
#[cfg(windows)]
pub use mmap_windows::{FileCleanup, MmapRegion};

#[cfg(all(test, loom))]
mod loom_tests;
