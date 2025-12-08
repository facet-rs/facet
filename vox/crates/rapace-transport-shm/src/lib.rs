//! rapace-transport-shm: Shared memory transport for rapace.
//!
//! This is the **performance reference** implementation. It defines the
//! canonical memory layout and zero-copy patterns.
//!
//! # Characteristics
//!
//! - SPSC rings for descriptors
//! - Slab allocator for payloads
//! - Zero-copy when data is already in SHM
//! - eventfd doorbells for async notification (future)
//! - Generation counters for crash safety
//!
//! # Architecture
//!
//! Each SHM segment represents a session between exactly two peers (A and B).
//! The design is intentionally SPSC (single-producer, single-consumer) per ring.
//!
//! ```text
//! ┌─────────────────────────────────────────────────────────────────────┐
//! │  Segment Header (64 bytes)                                           │
//! ├─────────────────────────────────────────────────────────────────────┤
//! │  A→B Descriptor Ring                                                 │
//! ├─────────────────────────────────────────────────────────────────────┤
//! │  B→A Descriptor Ring                                                 │
//! ├─────────────────────────────────────────────────────────────────────┤
//! │  Data Segment (slab allocator)                                       │
//! └─────────────────────────────────────────────────────────────────────┘
//! ```

pub mod layout;
mod session;
mod transport;

pub use layout::{
    calculate_segment_size, DataSegment, DataSegmentHeader, DescRing, DescRingHeader,
    LayoutError, RingError, SegmentHeader, SegmentOffsets, SlotError, SlotMeta, SlotState,
    DEFAULT_RING_CAPACITY, DEFAULT_SLOT_COUNT, DEFAULT_SLOT_SIZE,
};
pub use session::{ShmSession, ShmSessionConfig};
pub use transport::ShmTransport;
