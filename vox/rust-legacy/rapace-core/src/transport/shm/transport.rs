//! SHM transport implementation.
//!
//! This module provides the hub-based shared memory transport, which supports
//! one host communicating with many peers through a shared memory segment.

use std::sync::Arc;

use super::hub_transport::{HubHostPeerTransport, HubPeerTransport};
use crate::transport::Transport;
use crate::{Frame, TransportError};

/// SHM transport (hub mode).
///
/// The hub architecture supports one host process communicating with many peer
/// processes through a shared memory segment with multiple size classes for
/// efficient payload handling (1KB to 16MB).
#[derive(Clone, Debug)]
pub enum ShmTransport {
    /// Hub plugin-side transport (peer↔host).
    HubPeer(HubPeerTransport),
    /// Hub host-side per-peer transport (host↔peer).
    HubHostPeer(HubHostPeerTransport),
}

impl ShmTransport {
    /// Create a hub peer transport (plugin/cell side).
    pub fn hub_peer(
        peer: Arc<super::hub_session::HubPeer>,
        doorbell: super::Doorbell,
        name: impl Into<String>,
    ) -> Self {
        ShmTransport::HubPeer(HubPeerTransport::new(peer, doorbell, name))
    }

    /// Create a hub host-side per-peer transport.
    pub fn hub_host_peer(
        host: Arc<super::hub_session::HubHost>,
        peer_id: u16,
        doorbell: super::Doorbell,
    ) -> Self {
        ShmTransport::HubHostPeer(HubHostPeerTransport::new(host, peer_id, doorbell))
    }

    /// Create a pair of hub transports for in-process testing.
    ///
    /// This creates a hub with a single peer and returns the host-side and peer-side
    /// transports. The hub is backed by a temporary file that is cleaned up when
    /// both transports are dropped.
    ///
    /// Returns `(host_transport, peer_transport)`.
    pub fn hub_pair() -> Result<(Self, Self), super::hub_session::HubSessionError> {
        use super::hub_session::{HubConfig, HubHost, HubPeer};

        // Create a temporary file for the hub
        let temp_dir = std::env::temp_dir();
        let hub_path = temp_dir.join(format!("rapace-hub-test-{}.shm", std::process::id()));

        // Create the hub host
        let host = Arc::new(HubHost::create(&hub_path, HubConfig::default())?);

        // Add a peer and get its info
        let peer_info = host.add_peer()?;
        let peer_id = peer_info.peer_id;

        // Create the host-side doorbell from the peer's doorbell fd
        let peer_doorbell = super::Doorbell::from_raw_fd(peer_info.peer_doorbell_fd)
            .map_err(super::hub_session::HubSessionError::Io)?;

        // Open the hub from the peer side
        let peer = Arc::new(HubPeer::open(&hub_path, peer_id)?);
        peer.register();

        // Create transports
        let host_transport = ShmTransport::hub_host_peer(host, peer_id, peer_info.doorbell);
        let peer_transport = ShmTransport::hub_peer(peer, peer_doorbell, "test-peer");

        Ok((host_transport, peer_transport))
    }

    /// Check if the transport is closed.
    pub fn is_closed(&self) -> bool {
        match self {
            ShmTransport::HubPeer(t) => t.is_closed(),
            ShmTransport::HubHostPeer(t) => t.is_closed(),
        }
    }
}

impl Transport for ShmTransport {
    async fn send_frame(&self, frame: Frame) -> Result<(), TransportError> {
        match self {
            ShmTransport::HubPeer(t) => Transport::send_frame(t, frame).await,
            ShmTransport::HubHostPeer(t) => Transport::send_frame(t, frame).await,
        }
    }

    async fn recv_frame(&self) -> Result<Frame, TransportError> {
        match self {
            ShmTransport::HubPeer(t) => Transport::recv_frame(t).await,
            ShmTransport::HubHostPeer(t) => Transport::recv_frame(t).await,
        }
    }

    fn close(&self) {
        match self {
            ShmTransport::HubPeer(t) => Transport::close(t),
            ShmTransport::HubHostPeer(t) => Transport::close(t),
        }
    }

    fn is_closed(&self) -> bool {
        ShmTransport::is_closed(self)
    }

    fn buffer_pool(&self) -> &crate::BufferPool {
        match self {
            ShmTransport::HubPeer(t) => Transport::buffer_pool(t),
            ShmTransport::HubHostPeer(t) => Transport::buffer_pool(t),
        }
    }
}

// ============================================================================
// Metrics for zero-copy tracking
// ============================================================================

/// Metrics for tracking zero-copy performance and slot allocation.
///
/// This is useful for monitoring and testing the zero-copy path,
/// as well as diagnosing slot exhaustion issues.
#[derive(Debug, Default)]
pub struct ShmMetrics {
    /// Number of times encode_bytes detected data already in SHM (zero-copy).
    pub zero_copy_encodes: std::sync::atomic::AtomicU64,
    /// Number of times encode_bytes had to copy data (not in SHM).
    pub copy_encodes: std::sync::atomic::AtomicU64,
    /// Total bytes that were zero-copy encoded.
    pub zero_copy_bytes: std::sync::atomic::AtomicU64,
    /// Total bytes that were copied during encoding.
    pub copied_bytes: std::sync::atomic::AtomicU64,

    // Slot allocation metrics
    /// Number of successful slot allocations.
    pub alloc_success: std::sync::atomic::AtomicU64,
    /// Number of failed slot allocations (NoFreeSlots).
    pub alloc_failures: std::sync::atomic::AtomicU64,
    /// Number of slot frees.
    pub slot_frees: std::sync::atomic::AtomicU64,
    /// Total bytes copied to slots.
    pub slot_copy_bytes: std::sync::atomic::AtomicU64,

    // Ring metrics
    /// Number of successful ring enqueues.
    pub ring_enqueues: std::sync::atomic::AtomicU64,
    /// Number of ring full errors.
    pub ring_full_errors: std::sync::atomic::AtomicU64,
    /// Number of successful ring dequeues.
    pub ring_dequeues: std::sync::atomic::AtomicU64,

    // Frame metrics
    /// Number of frames sent with inline payload.
    pub inline_sends: std::sync::atomic::AtomicU64,
    /// Number of frames sent with slot payload.
    pub slot_sends: std::sync::atomic::AtomicU64,
}

impl ShmMetrics {
    /// Create a new metrics instance.
    pub fn new() -> Self {
        Self::default()
    }

    /// Record a zero-copy encode.
    pub fn record_zero_copy(&self, bytes: usize) {
        self.zero_copy_encodes
            .fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        self.zero_copy_bytes
            .fetch_add(bytes as u64, std::sync::atomic::Ordering::Relaxed);
    }

    /// Record a copy encode.
    pub fn record_copy(&self, bytes: usize) {
        self.copy_encodes
            .fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        self.copied_bytes
            .fetch_add(bytes as u64, std::sync::atomic::Ordering::Relaxed);
    }

    /// Get the number of zero-copy encodes.
    pub fn zero_copy_count(&self) -> u64 {
        self.zero_copy_encodes
            .load(std::sync::atomic::Ordering::Relaxed)
    }

    /// Get the number of copy encodes.
    pub fn copy_count(&self) -> u64 {
        self.copy_encodes.load(std::sync::atomic::Ordering::Relaxed)
    }

    /// Get the zero-copy efficiency as a percentage (0.0 to 1.0).
    pub fn zero_copy_ratio(&self) -> f64 {
        let zero = self.zero_copy_count() as f64;
        let copy = self.copy_count() as f64;
        let total = zero + copy;
        if total == 0.0 { 0.0 } else { zero / total }
    }

    // Slot allocation metrics

    /// Record a successful slot allocation.
    pub fn record_alloc_success(&self) {
        self.alloc_success
            .fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    }

    /// Record a failed slot allocation (NoFreeSlots).
    pub fn record_alloc_failure(&self) {
        self.alloc_failures
            .fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    }

    /// Record a slot free.
    pub fn record_slot_free(&self) {
        self.slot_frees
            .fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    }

    /// Record bytes copied to a slot.
    pub fn record_slot_copy(&self, bytes: usize) {
        self.slot_copy_bytes
            .fetch_add(bytes as u64, std::sync::atomic::Ordering::Relaxed);
    }

    // Ring metrics

    /// Record a successful ring enqueue.
    pub fn record_ring_enqueue(&self) {
        self.ring_enqueues
            .fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    }

    /// Record a ring full error.
    pub fn record_ring_full(&self) {
        self.ring_full_errors
            .fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    }

    /// Record a ring dequeue.
    pub fn record_ring_dequeue(&self) {
        self.ring_dequeues
            .fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    }

    // Frame metrics

    /// Record an inline send.
    pub fn record_inline_send(&self) {
        self.inline_sends
            .fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    }

    /// Record a slot send.
    pub fn record_slot_send(&self) {
        self.slot_sends
            .fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    }

    /// Get alloc failure count.
    pub fn alloc_failure_count(&self) -> u64 {
        self.alloc_failures
            .load(std::sync::atomic::Ordering::Relaxed)
    }

    /// Get alloc success count.
    pub fn alloc_success_count(&self) -> u64 {
        self.alloc_success
            .load(std::sync::atomic::Ordering::Relaxed)
    }

    /// Get ring full error count.
    pub fn ring_full_count(&self) -> u64 {
        self.ring_full_errors
            .load(std::sync::atomic::Ordering::Relaxed)
    }

    /// Format a summary of all metrics.
    pub fn summary(&self) -> String {
        use std::sync::atomic::Ordering::Relaxed;
        format!(
            "ShmMetrics {{ \
            alloc: {}/{} ok/fail, \
            slot_frees: {}, \
            ring: {}/{}/{} enq/deq/full, \
            frames: {}/{} inline/slot, \
            zero_copy: {:.1}% ({}/{}) \
            }}",
            self.alloc_success.load(Relaxed),
            self.alloc_failures.load(Relaxed),
            self.slot_frees.load(Relaxed),
            self.ring_enqueues.load(Relaxed),
            self.ring_dequeues.load(Relaxed),
            self.ring_full_errors.load(Relaxed),
            self.inline_sends.load(Relaxed),
            self.slot_sends.load(Relaxed),
            self.zero_copy_ratio() * 100.0,
            self.zero_copy_encodes.load(Relaxed),
            self.copy_encodes.load(Relaxed),
        )
    }
}

// Static assertions: ShmTransport must be Send + Sync
static_assertions::assert_impl_all!(ShmTransport: Send, Sync);
