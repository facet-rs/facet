//! Lock-free primitives for shared memory IPC.
//!
//! This crate provides `no_std`-compatible, lock-free data structures designed
//! for use in shared memory contexts where you work with raw pointers to
//! memory-mapped regions.
//!
//! # Primitives
//!
//! - [`SpscRing`] / [`SpscRingRaw`]: Single-producer single-consumer ring buffer
//! - [`TreiberSlab`] / [`TreiberSlabRaw`]: Treiber stack-based slab allocator with
//!   generation counting for ABA protection
//!
//! # Raw vs Region APIs
//!
//! Each primitive has two variants:
//!
//! - **Raw** (`SpscRingRaw`, `TreiberSlabRaw`): Work with raw pointers, suitable for
//!   shared memory where you have `*mut` pointers from mmap. Caller manages memory lifetime.
//!
//! - **Region** (`SpscRing`, `TreiberSlab`): Convenience wrappers that own their backing
//!   memory via a [`Region`]. These delegate to the Raw implementations internally.
//!
//! # Loom Testing
//!
//! Enable the `loom` feature for concurrency verification. All algorithms are tested
//! under loom to verify correctness across all possible thread interleavings.
//!
//! ```text
//! cargo test -p shm-primitives --features loom
//! ```

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

#[cfg(all(test, feature = "loom"))]
mod loom_tests;
