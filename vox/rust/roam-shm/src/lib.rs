//! Shared-memory transport binding for roam.
//!
//! This crate implements the SHM hub transport as specified in
//! `docs/content/shm-spec/_index.md`. It enables high-performance IPC
//! between a host and multiple guests via shared memory.
//!
//! # Architecture
//!
//! The SHM transport uses a hub topology with one host and multiple guests:
//!
//! ```text
//!          ┌─────────┐
//!          │  Host   │
//!          └────┬────┘
//!               │
//!     ┌─────────┼─────────┐
//!     │         │         │
//! ┌───┴───┐ ┌───┴───┐ ┌───┴───┐
//! │Guest 1│ │Guest 2│ │Guest 3│
//! └───────┘ └───────┘ └───────┘
//! ```
//!
//! Guests communicate only with the host, not with each other. Each guest
//! has its own rings and slot pool within the shared segment.
//!
//! # Usage
//!
//! ## Host side
//!
//! ```ignore
//! use roam_shm::{ShmHost, SegmentConfig};
//!
//! // Create a new SHM segment
//! let config = SegmentConfig::default();
//! let host = ShmHost::create("/dev/shm/myapp", config)?;
//!
//! // Poll for guest messages
//! while let Some((peer_id, frame)) = host.poll() {
//!     // Handle message from guest
//! }
//! ```
//!
//! ## Guest side
//!
//! ```ignore
//! use roam_shm::ShmGuest;
//!
//! // Attach to existing segment
//! let guest = ShmGuest::attach("/dev/shm/myapp")?;
//!
//! // Send message to host
//! guest.send(frame)?;
//!
//! // Receive message from host
//! if let Some(frame) = guest.recv() {
//!     // Handle message
//! }
//! ```
//!
//! # Spec Coverage
//!
//! shm[impl shm.scope]
//! shm[impl shm.architecture]

#![cfg_attr(not(feature = "std"), no_std)]

#[macro_use]
mod macros;

pub mod channel;
pub mod layout;
pub mod msg;
pub mod peer;
mod slot_pool;
pub mod var_slot_pool;

#[cfg(feature = "std")]
pub mod host;

#[cfg(feature = "std")]
pub mod guest;

#[cfg(feature = "std")]
pub mod transport;

#[cfg(feature = "tokio")]
pub mod driver;

#[cfg(feature = "std")]
pub mod spawn;

#[cfg(feature = "std")]
pub mod wait;

#[cfg(all(feature = "std", unix))]
pub mod cleanup;

// Re-export key types
pub use channel::{
    ChannelEntry, ChannelId, ChannelIdAllocator, ChannelState, FlowControl, RequestId,
    RequestIdAllocator,
};
pub use layout::{
    HEADER_SIZE, MAGIC, SegmentConfig, SegmentHeader, SegmentLayout, SizeClass, VERSION,
};
pub use msg::msg_type;
pub use peer::{PeerEntry, PeerId, PeerState};
pub use var_slot_pool::{SizeClassHeader, VarFreeError, VarSlotHandle, VarSlotPool};

// Re-export MsgDesc from roam-frame
pub use roam_frame::{Frame, INLINE_PAYLOAD_LEN, INLINE_PAYLOAD_SLOT, MsgDesc, Payload};

// Re-export FileCleanup from shm-primitives
#[cfg(feature = "std")]
pub use shm_primitives::FileCleanup;

#[cfg(feature = "std")]
pub use host::{PollResult, ShmHost};

#[cfg(feature = "std")]
pub use guest::ShmGuest;

#[cfg(feature = "std")]
pub use transport::{
    ConvertError, ShmGuestTransport, ShmHostGuestTransport, frame_to_message, message_to_frame,
};

#[cfg(feature = "std")]
pub use spawn::{
    AddPeerOptions, DeathCallback, SpawnArgs, SpawnArgsError, SpawnTicket, die_with_parent,
};

#[cfg(feature = "std")]
pub use wait::{
    WaitError, wait_for_credit, wait_for_ring_data, wait_for_ring_space, wait_for_slot,
    wake_credit_waiters, wake_ring_consumers, wake_ring_producers, wake_slot_waiters,
};

/// Handshake is implicit via segment header.
///
/// shm[impl shm.handshake]
/// shm[impl shm.handshake.no-negotiation]
///
/// SHM does not use Hello messages. The segment header fields serve as the
/// host's unilateral configuration. Guests accept these values by attaching.
pub const fn _handshake_is_implicit() {}

/// Payload encoding is postcard.
///
/// shm[impl shm.payload.encoding]
pub const fn _payload_encoding_is_postcard() {}
