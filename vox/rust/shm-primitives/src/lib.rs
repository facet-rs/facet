pub mod bipbuf;
pub mod bootstrap;
mod loom_tests;
pub mod peer;
pub mod region;
pub mod segment;
pub mod slot;
pub mod sync;
pub mod varslot_pool;

pub use bipbuf::{
    BIPBUF_HEADER_SIZE, BipBuf, BipBufConsumer, BipBufFull, BipBufHeader, BipBufProducer, BipBufRaw,
};
pub use peer::{PeerEntry, PeerId, PeerState};
pub use region::HeapRegion;
pub use region::Region;
pub use segment::{MAGIC, SEGMENT_HEADER_SIZE, SEGMENT_VERSION, SegmentHeader, SegmentHeaderInit};
pub use slot::{SlotState, VarSlotMeta};
pub use varslot_pool::{
    ClassOffsets, DoubleFreeError, PoolLayout, SizeClassConfig, SlotRef, VarSlotPool,
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
