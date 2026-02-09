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

#[macro_use]
mod macros;

pub mod channel;
pub mod layout;
pub mod msg;
pub mod peer;
pub mod var_slot_pool;

pub mod auditable;
pub mod bootstrap;
pub mod cleanup;
pub mod diagnostic;
pub mod driver;
pub mod guest;
pub mod host;
pub mod spawn;
pub mod transport;

// Re-export key types
pub use channel::{
    ChannelEntry, ChannelId, ChannelIdAllocator, ChannelState, FlowControl, RequestId,
    RequestIdAllocator,
};
pub use layout::{
    HEADER_SIZE, MAGIC, SegmentConfig, SegmentHeader, SegmentLayout, SizeClass, VERSION,
};
pub use msg::{ShmMsg, msg_type};
pub use peer::{PeerEntry, PeerId, PeerState};
pub use var_slot_pool::{SizeClassHeader, VarFreeError, VarSlotHandle, VarSlotPool};

// Re-export FileCleanup from shm-primitives
pub use shm_primitives::FileCleanup;

pub use auditable::dump_all_channels;
pub use diagnostic::{ShmDiagnosticView, ShmDiagnostics};
pub use host::{PollResult, ShmHost};

pub use guest::ShmGuest;

pub use transport::{
    ConvertError, ShmGuestTransport, ShmHostGuestTransport, message_to_shm_msg, shm_msg_to_message,
};

pub use spawn::{
    AddPeerOptions, DeathCallback, SpawnArgs, SpawnArgsError, SpawnTicket, die_with_parent,
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
