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
//!
//! # Optional: SHM Allocator
//!
//! Enable the `allocator` feature to get [`ShmAllocator`], which allows allocating
//! data directly into SHM slots. When such data is passed through the encoder,
//! it's detected as already in SHM and referenced zero-copy (no memcpy).
//!
//! ```toml
//! [dependencies]
//! rapace-transport-shm = { version = "0.1", features = ["allocator"] }
//! ```
//!
//! **Important:** This is an optional optimization. Service traits remain
//! transport-agnostic—they don't know or care whether data is in SHM.
//! The allocator is for callers who want to pre-allocate in SHM for performance.

#[cfg(any(feature = "allocator", test))]
mod alloc;
pub mod doorbell;
pub mod futex;
pub mod hub_alloc;
pub mod hub_layout;
pub mod hub_session;
pub mod hub_transport;
pub mod layout;
mod session;
mod transport;

#[cfg(any(feature = "allocator", test))]
pub use alloc::ShmAllocator;
pub use layout::{
    DEFAULT_RING_CAPACITY, DEFAULT_SLOT_COUNT, DEFAULT_SLOT_SIZE, DataSegment, DataSegmentHeader,
    DescRing, DescRingHeader, LayoutError, RingError, RingStatus, SegmentHeader, SegmentOffsets,
    SlotError, SlotMeta, SlotState, calculate_segment_size,
};
pub use session::{ShmSession, ShmSessionConfig};
pub use transport::{ShmMetrics, ShmTransport};

// Hub architecture re-exports
pub use doorbell::{Doorbell, close_peer_fd};
pub use hub_alloc::{HubAllocator, HubSlotStatus, SizeClassStatus};
pub use hub_layout::{
    ExtentHeader, HUB_SIZE_CLASSES, HubHeader, HubOffsets, HubSlotError, HubSlotMeta, PeerEntry,
    SizeClassHeader, decode_slot_ref, encode_slot_ref,
};
pub use hub_session::{HubConfig, HubHost, HubPeer, PeerInfo};
pub use hub_transport::{
    HostPeerHandle, HubHostPeerTransport, HubHostTransport, HubPeerTransport, HubTransportError,
    INLINE_PAYLOAD_SIZE, INLINE_PAYLOAD_SLOT,
};

// Re-export allocator-api2 types for convenience when the feature is enabled.
#[cfg(feature = "allocator")]
pub use allocator_api2;

// ============================================================================
// Helper functions for easy SHM allocation
// ============================================================================

/// Create a `Vec<u8>` allocated in SHM from a byte slice.
///
/// This is a convenience function for the common pattern of allocating
/// a buffer in SHM and copying data into it. When the resulting Vec is
/// passed through the encoder, it will be detected as already in SHM
/// and referenced zero-copy.
///
/// # Example
///
/// ```ignore
/// use rapace_transport_shm::{ShmSession, ShmAllocator, shm_vec};
///
/// let (session, _) = ShmSession::create_pair().unwrap();
/// let alloc = ShmAllocator::new(session.clone());
///
/// // Allocate PNG data in SHM
/// let png_bytes = std::fs::read("image.png").unwrap();
/// let shm_png = shm_vec(&alloc, &png_bytes);
///
/// // When passed to the encoder, this is zero-copy!
/// ```
#[cfg(any(feature = "allocator", test))]
pub fn shm_vec(alloc: &ShmAllocator, bytes: &[u8]) -> allocator_api2::vec::Vec<u8, ShmAllocator> {
    let mut vec = allocator_api2::vec::Vec::new_in(alloc.clone());
    vec.extend_from_slice(bytes);
    vec
}

/// Create an empty `Vec<u8>` allocated in SHM with the given capacity.
///
/// Use this when you need to build up data incrementally but want it
/// to end up in SHM for zero-copy transmission.
///
/// # Example
///
/// ```ignore
/// use rapace_transport_shm::{ShmSession, ShmAllocator, shm_vec_with_capacity};
///
/// let (session, _) = ShmSession::create_pair().unwrap();
/// let alloc = ShmAllocator::new(session.clone());
///
/// // Pre-allocate buffer in SHM
/// let mut buf = shm_vec_with_capacity(&alloc, 4096);
/// buf.extend_from_slice(b"header: ");
/// buf.extend_from_slice(b"value\n");
///
/// // When passed to the encoder, this is zero-copy!
/// ```
#[cfg(any(feature = "allocator", test))]
pub fn shm_vec_with_capacity(
    alloc: &ShmAllocator,
    capacity: usize,
) -> allocator_api2::vec::Vec<u8, ShmAllocator> {
    allocator_api2::vec::Vec::with_capacity_in(capacity, alloc.clone())
}
