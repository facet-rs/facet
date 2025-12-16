//! Shared memory (SHM) transport.

mod alloc;
mod doorbell;
pub mod futex;
mod hub_alloc;
#[cfg(unix)]
mod hub_host;
pub mod hub_layout;
pub mod hub_session;
mod hub_transport;
pub mod layout;
mod session;
mod slot_guard;
mod transport;

pub use alloc::ShmAllocator;
pub use allocator_api2;
pub use doorbell::{Doorbell, close_peer_fd};
pub use hub_alloc::HubAllocator;
#[cfg(unix)]
pub use hub_host::HubPeerTicket;
pub use hub_session::{HubConfig, HubHost, HubPeer, HubSessionError, PeerInfo};
pub use hub_transport::{HubHostPeerTransport, HubPeerTransport};
pub use session::{ShmSession, ShmSessionConfig};
pub use slot_guard::SlotGuard;
pub use transport::{ShmMetrics, ShmTransport};
