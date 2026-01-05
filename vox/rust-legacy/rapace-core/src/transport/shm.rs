//! Shared memory (SHM) transport.
//!
//! This module provides the hub-based shared memory transport, which supports
//! one host communicating with many peers through a shared memory segment.

// SHM transport requires unsafe for low-level memory operations
#![allow(unsafe_code)]

mod hub_alloc;
#[cfg(unix)]
mod hub_host;
pub mod hub_layout;
pub mod hub_session;
mod hub_transport;
pub mod layout;
mod slot_guard;
mod transport;

pub use allocator_api2;
pub use hub_alloc::HubAllocator;
#[cfg(unix)]
pub use hub_host::{AddPeerOptions, HubPeerTicket};
pub use hub_session::{HubConfig, HubHost, HubPeer, HubSessionError, PeerInfo};
pub use hub_transport::{HubHostPeerTransport, HubPeerTransport, PeerDeathCallback};
pub use slot_guard::SlotGuard;
pub use transport::{ShmMetrics, ShmTransport};

// Re-export OS primitives from shm-primitives
pub use shm_primitives::futex;
pub use shm_primitives::{Doorbell, SignalResult, close_peer_fd};
