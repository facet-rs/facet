//! Hub transport implementation.
//!
//! This module provides the transport layer for the hub architecture.
//!
//! - `HubPeerTransport`: For plugins, implements send/recv to/from host
//! - `HubHostPeerTransport`: Per-peer transport for host side (implements Transport trait)
//! - `HubHostTransport`: For host, manages communication with all peers

use std::sync::Arc;
use std::sync::atomic::Ordering;
use std::time::Duration;

use rapace_core::{
    EncodeCtx, EncodeError, Frame, MsgDescHot, RecvFrame, Transport, TransportError,
};
use tokio::sync::Mutex as AsyncMutex;

use crate::doorbell::Doorbell;
use crate::futex;
use crate::hub_alloc::HubAllocator;
use crate::hub_layout::{HubSlotError, decode_slot_ref, encode_slot_ref};
use crate::hub_session::{HubHost, HubPeer};

/// Maximum payload size that can be inlined in a descriptor.
pub const INLINE_PAYLOAD_SIZE: usize = 16;

/// Sentinel value for inline payloads (no slot used).
pub const INLINE_PAYLOAD_SLOT: u32 = u32::MAX;

/// Transport for a plugin peer.
///
/// Provides send/recv operations to communicate with the host through
/// the shared hub SHM.
pub struct HubPeerTransport {
    /// The hub peer session.
    peer: Arc<HubPeer>,
    /// Doorbell for signaling/waiting.
    doorbell: Doorbell,
    /// Local send head (producer-private).
    ///
    /// IMPORTANT: this ring is single-producer; we must serialize senders.
    local_send_head: AsyncMutex<u64>,
    /// Name for debugging.
    name: String,
}

impl HubPeerTransport {
    /// Create a new peer transport.
    pub fn new(peer: Arc<HubPeer>, doorbell: Doorbell, name: impl Into<String>) -> Self {
        Self {
            peer,
            doorbell,
            local_send_head: AsyncMutex::new(0),
            name: name.into(),
        }
    }

    /// Get the peer ID.
    pub fn peer_id(&self) -> u16 {
        self.peer.peer_id()
    }

    /// Get the allocator.
    pub fn allocator(&self) -> &HubAllocator {
        self.peer.allocator()
    }

    /// Send a frame to the host.
    ///
    /// If the payload is small enough, it's inlined in the descriptor.
    /// Otherwise, a slot is allocated from the appropriate size class.
    pub async fn send_frame(
        &self,
        desc: &MsgDescHot,
        payload: &[u8],
    ) -> Result<(), HubTransportError> {
        self.peer.update_heartbeat();

        let mut desc = *desc;

        if payload.len() <= INLINE_PAYLOAD_SIZE {
            // Inline payload
            desc.payload_slot = INLINE_PAYLOAD_SLOT;
            desc.payload_generation = 0;
            desc.payload_offset = 0;
            desc.payload_len = payload.len() as u32;
            desc.inline_payload[..payload.len()].copy_from_slice(payload);
        } else {
            // Allocate slot
            let (class, global_index, generation) = self
                .allocator()
                .alloc(payload.len(), self.peer.peer_id() as u32)
                .map_err(HubTransportError::SlotAlloc)?;

            // Copy payload to slot
            let slot_ptr = unsafe { self.allocator().slot_data_ptr(class as usize, global_index) };
            unsafe {
                std::ptr::copy_nonoverlapping(payload.as_ptr(), slot_ptr, payload.len());
            }

            // Mark in-flight
            self.allocator()
                .mark_in_flight(class, global_index, generation)
                .map_err(HubTransportError::SlotAlloc)?;

            desc.payload_slot = encode_slot_ref(class, global_index);
            desc.payload_generation = generation;
            desc.payload_offset = 0;
            desc.payload_len = payload.len() as u32;
        }

        // Enqueue to send ring
        let send_ring = self.peer.send_ring();

        loop {
            {
                let mut local_head = self.local_send_head.lock().await;
                if send_ring.enqueue(&mut local_head, &desc).is_ok() {
                    break;
                }
            }

            // Use async futex wait to avoid blocking the executor
            // This moves the blocking wait to spawn_blocking thread pool
            let _ = futex::futex_wait_async_ptr(
                self.peer.send_data_futex(),
                Some(std::time::Duration::from_millis(100)),
            )
            .await;
        }

        // Signal host
        self.doorbell.signal();
        futex::futex_signal(self.peer.send_data_futex());

        Ok(())
    }

    /// Receive a frame from the host.
    ///
    /// Returns the descriptor and payload data.
    pub async fn recv_frame(&self) -> Result<(MsgDescHot, Vec<u8>), HubTransportError> {
        self.peer.update_heartbeat();

        let recv_ring = self.peer.recv_ring();

        loop {
            if let Some(desc) = recv_ring.dequeue() {
                // Extract payload
                let payload = if desc.payload_slot == INLINE_PAYLOAD_SLOT {
                    // Inline payload
                    desc.inline_payload[..desc.payload_len as usize].to_vec()
                } else {
                    // Slot payload
                    let (class, global_index) = decode_slot_ref(desc.payload_slot);
                    let slot_ptr =
                        unsafe { self.allocator().slot_data_ptr(class as usize, global_index) };
                    let slot_ptr = unsafe { slot_ptr.add(desc.payload_offset as usize) };
                    let payload =
                        unsafe { std::slice::from_raw_parts(slot_ptr, desc.payload_len as usize) }
                            .to_vec();

                    // Free the slot
                    self.allocator()
                        .free(class, global_index, desc.payload_generation)
                        .map_err(HubTransportError::SlotAlloc)?;

                    payload
                };

                // Signal space available
                futex::futex_signal(self.peer.recv_data_futex());

                return Ok((desc, payload));
            }

            // Wait for data
            self.doorbell.wait().await?;
            self.doorbell.drain();
        }
    }

    /// Get the transport name.
    pub fn name(&self) -> &str {
        &self.name
    }

    /// Get the number of bytes pending in the doorbell (for diagnostics).
    pub fn doorbell_pending_bytes(&self) -> usize {
        self.doorbell.pending_bytes()
    }

    /// Get the underlying peer (for diagnostics).
    pub fn peer(&self) -> &Arc<HubPeer> {
        &self.peer
    }
}

// ============================================================================
// Per-peer Transport for Host Side
// ============================================================================

/// Per-peer transport adapter for the host side.
///
/// This wraps a HubHost for a single peer and implements the Transport trait,
/// allowing it to be used with RpcSession.
pub struct HubHostPeerTransport {
    /// The hub host.
    host: Arc<HubHost>,
    /// The peer ID this transport communicates with.
    peer_id: u16,
    /// Doorbell for signaling/waiting.
    doorbell: Doorbell,
    /// Local send head for peer's recv ring.
    ///
    /// IMPORTANT: this ring is single-producer; we must serialize senders.
    local_send_head: AsyncMutex<u64>,
    /// Whether the transport is closed.
    closed: std::sync::atomic::AtomicBool,
    /// Optional name for debugging.
    name: Option<String>,
}

impl HubHostPeerTransport {
    /// Create a new per-peer transport for the host side.
    pub fn new(host: Arc<HubHost>, peer_id: u16, doorbell: Doorbell) -> Self {
        Self {
            host,
            peer_id,
            doorbell,
            local_send_head: AsyncMutex::new(0),
            closed: std::sync::atomic::AtomicBool::new(false),
            name: None,
        }
    }

    /// Create a new per-peer transport with a name for debugging.
    pub fn with_name(
        host: Arc<HubHost>,
        peer_id: u16,
        doorbell: Doorbell,
        name: impl Into<String>,
    ) -> Self {
        Self {
            host,
            peer_id,
            doorbell,
            local_send_head: AsyncMutex::new(0),
            closed: std::sync::atomic::AtomicBool::new(false),
            name: Some(name.into()),
        }
    }

    /// Get the peer ID.
    pub fn peer_id(&self) -> u16 {
        self.peer_id
    }

    /// Get the allocator.
    pub fn allocator(&self) -> &HubAllocator {
        self.host.allocator()
    }

    /// Check if the transport is closed.
    #[inline]
    pub fn is_closed(&self) -> bool {
        self.closed.load(Ordering::Acquire)
    }

    /// Get the number of bytes pending in the doorbell (for diagnostics).
    pub fn doorbell_pending_bytes(&self) -> usize {
        self.doorbell.pending_bytes()
    }
}

impl Transport for HubHostPeerTransport {
    type RecvPayload = Vec<u8>;

    async fn send_frame(&self, frame: &Frame) -> Result<(), TransportError> {
        if self.is_closed() {
            return Err(TransportError::Closed);
        }

        let mut desc = frame.desc;
        let payload = frame.payload();

        if payload.len() <= INLINE_PAYLOAD_SIZE {
            // Inline payload
            desc.payload_slot = INLINE_PAYLOAD_SLOT;
            desc.payload_generation = 0;
            desc.payload_offset = 0;
            desc.payload_len = payload.len() as u32;
            desc.inline_payload[..payload.len()].copy_from_slice(payload);
        } else {
            // Allocate slot from shared pool (host owns the slot, owner=0)
            let (class, global_index, generation) = self
                .allocator()
                .alloc(payload.len(), 0)
                .map_err(|_| TransportError::Encode(EncodeError::NoSlotAvailable))?;

            // Copy payload to slot
            let slot_ptr = unsafe { self.allocator().slot_data_ptr(class as usize, global_index) };
            unsafe {
                std::ptr::copy_nonoverlapping(payload.as_ptr(), slot_ptr, payload.len());
            }

            // Mark in-flight
            self.allocator()
                .mark_in_flight(class, global_index, generation)
                .map_err(|_| TransportError::Encode(EncodeError::NoSlotAvailable))?;

            desc.payload_slot = encode_slot_ref(class, global_index);
            desc.payload_generation = generation;
            desc.payload_offset = 0;
            desc.payload_len = payload.len() as u32;
        }

        // Enqueue to peer's recv ring (host sends TO peer's recv)
        let recv_ring = self.host.peer_recv_ring(self.peer_id);

        const FUTEX_TIMEOUT: Duration = Duration::from_millis(100);

        loop {
            {
                let mut local_head = self.local_send_head.lock().await;
                if recv_ring.enqueue(&mut local_head, &desc).is_ok() {
                    break;
                }
            }

            if self.is_closed() {
                return Err(TransportError::Closed);
            }
            // Use async futex wait to avoid blocking the executor
            let futex = self.host.peer_recv_data_futex(self.peer_id);
            let _ = futex::futex_wait_async_ptr(futex, Some(FUTEX_TIMEOUT)).await;
        }

        // Signal peer
        self.doorbell.signal();
        futex::futex_signal(self.host.peer_recv_data_futex(self.peer_id));

        Ok(())
    }

    async fn recv_frame(&self) -> Result<RecvFrame<Self::RecvPayload>, TransportError> {
        if self.is_closed() {
            return Err(TransportError::Closed);
        }

        // Dequeue from peer's send ring (peer sends TO host, host receives)
        let send_ring = self.host.peer_send_ring(self.peer_id);

        loop {
            if let Some(desc) = send_ring.dequeue() {
                // Signal space available
                futex::futex_signal(self.host.peer_send_data_futex(self.peer_id));

                // Extract payload
                let payload = if desc.payload_slot == INLINE_PAYLOAD_SLOT {
                    desc.inline_payload[..desc.payload_len as usize].to_vec()
                } else {
                    let (class, global_index) = decode_slot_ref(desc.payload_slot);
                    let slot_ptr =
                        unsafe { self.allocator().slot_data_ptr(class as usize, global_index) };
                    let slot_ptr = unsafe { slot_ptr.add(desc.payload_offset as usize) };
                    let payload_data =
                        unsafe { std::slice::from_raw_parts(slot_ptr, desc.payload_len as usize) }
                            .to_vec();

                    // Free the slot
                    let _ = self
                        .allocator()
                        .free(class, global_index, desc.payload_generation);

                    payload_data
                };

                return Ok(RecvFrame::with_payload(desc, payload));
            }

            if self.is_closed() {
                return Err(TransportError::Closed);
            }

            // Wait for data via doorbell
            if let Err(e) = self.doorbell.wait().await {
                tracing::warn!(
                    peer_id = self.peer_id,
                    peer_name = self.name.as_deref(),
                    error = %e,
                    "doorbell wait failed"
                );
            }
            self.doorbell.drain();
        }
    }

    fn encoder(&self) -> Box<dyn EncodeCtx + '_> {
        Box::new(HubEncoder::new())
    }

    async fn close(&self) -> Result<(), TransportError> {
        self.closed.store(true, Ordering::Release);
        Ok(())
    }
}

// ============================================================================
// Transport impl for HubPeerTransport (plugin side)
// ============================================================================

impl Transport for HubPeerTransport {
    type RecvPayload = Vec<u8>;

    async fn send_frame(&self, frame: &Frame) -> Result<(), TransportError> {
        self.peer.update_heartbeat();

        let mut desc = frame.desc;
        let payload = frame.payload();

        if payload.len() <= INLINE_PAYLOAD_SIZE {
            desc.payload_slot = INLINE_PAYLOAD_SLOT;
            desc.payload_generation = 0;
            desc.payload_offset = 0;
            desc.payload_len = payload.len() as u32;
            desc.inline_payload[..payload.len()].copy_from_slice(payload);
        } else {
            let (class, global_index, generation) = self
                .allocator()
                .alloc(payload.len(), self.peer.peer_id() as u32)
                .map_err(|_| TransportError::Encode(EncodeError::NoSlotAvailable))?;

            let slot_ptr = unsafe { self.allocator().slot_data_ptr(class as usize, global_index) };
            unsafe {
                std::ptr::copy_nonoverlapping(payload.as_ptr(), slot_ptr, payload.len());
            }

            self.allocator()
                .mark_in_flight(class, global_index, generation)
                .map_err(|_| TransportError::Encode(EncodeError::NoSlotAvailable))?;

            desc.payload_slot = encode_slot_ref(class, global_index);
            desc.payload_generation = generation;
            desc.payload_offset = 0;
            desc.payload_len = payload.len() as u32;
        }

        let send_ring = self.peer.send_ring();

        loop {
            {
                let mut local_head = self.local_send_head.lock().await;
                if send_ring.enqueue(&mut local_head, &desc).is_ok() {
                    break;
                }
            }

            // Use async futex wait to avoid blocking the executor
            let _ = futex::futex_wait_async_ptr(
                self.peer.send_data_futex(),
                Some(Duration::from_millis(100)),
            )
            .await;
        }

        self.doorbell.signal();
        futex::futex_signal(self.peer.send_data_futex());

        Ok(())
    }

    async fn recv_frame(&self) -> Result<RecvFrame<Self::RecvPayload>, TransportError> {
        self.peer.update_heartbeat();

        let recv_ring = self.peer.recv_ring();

        loop {
            if let Some(desc) = recv_ring.dequeue() {
                // Signal space available
                futex::futex_signal(self.peer.recv_data_futex());

                // Extract payload
                let payload = if desc.payload_slot == INLINE_PAYLOAD_SLOT {
                    desc.inline_payload[..desc.payload_len as usize].to_vec()
                } else {
                    let (class, global_index) = decode_slot_ref(desc.payload_slot);
                    let slot_ptr =
                        unsafe { self.allocator().slot_data_ptr(class as usize, global_index) };
                    let slot_ptr = unsafe { slot_ptr.add(desc.payload_offset as usize) };
                    let payload_data =
                        unsafe { std::slice::from_raw_parts(slot_ptr, desc.payload_len as usize) }
                            .to_vec();

                    // Free the slot
                    let _ = self
                        .allocator()
                        .free(class, global_index, desc.payload_generation);

                    payload_data
                };

                return Ok(RecvFrame::with_payload(desc, payload));
            }

            // Wait for data via doorbell
            if let Err(e) = self.doorbell.wait().await {
                tracing::warn!(error = %e, "peer doorbell wait failed");
            }
            self.doorbell.drain();
        }
    }

    fn encoder(&self) -> Box<dyn EncodeCtx + '_> {
        Box::new(HubEncoder::new())
    }

    async fn close(&self) -> Result<(), TransportError> {
        Ok(())
    }
}

// ============================================================================
// Encoder
// ============================================================================

/// Encoder for Hub transport.
pub struct HubEncoder {
    desc: MsgDescHot,
    payload: Vec<u8>,
}

impl HubEncoder {
    fn new() -> Self {
        Self {
            desc: MsgDescHot::new(),
            payload: Vec::new(),
        }
    }
}

impl EncodeCtx for HubEncoder {
    fn encode_bytes(&mut self, bytes: &[u8]) -> Result<(), EncodeError> {
        // For hub transport, we always copy to the payload buffer
        // Zero-copy detection could be added later
        self.payload.extend_from_slice(bytes);
        Ok(())
    }

    fn finish(mut self: Box<Self>) -> Result<Frame, EncodeError> {
        if self.payload.len() <= INLINE_PAYLOAD_SIZE {
            self.desc.payload_slot = INLINE_PAYLOAD_SLOT;
            self.desc.payload_generation = 0;
            self.desc.payload_offset = 0;
            self.desc.payload_len = self.payload.len() as u32;
            self.desc.inline_payload[..self.payload.len()].copy_from_slice(&self.payload);
            Ok(Frame::new(self.desc))
        } else {
            Ok(Frame::with_payload(self.desc, self.payload))
        }
    }
}

/// Information about a connected peer for the host.
pub struct HostPeerHandle {
    /// Peer ID.
    pub peer_id: u16,
    /// Doorbell for this peer.
    pub doorbell: Doorbell,
    /// Local send head for this peer's recv ring.
    ///
    /// IMPORTANT: this ring is single-producer; we must serialize senders.
    local_send_head: AsyncMutex<u64>,
}

impl HostPeerHandle {
    /// Create a new peer handle.
    pub fn new(peer_id: u16, doorbell: Doorbell) -> Self {
        Self {
            peer_id,
            doorbell,
            local_send_head: AsyncMutex::new(0),
        }
    }
}

/// Transport for the host side.
///
/// Manages communication with all connected peers.
pub struct HubHostTransport {
    /// The hub host session.
    host: Arc<HubHost>,
    /// Connected peer handles.
    peers: Vec<HostPeerHandle>,
}

impl HubHostTransport {
    /// Create a new host transport.
    pub fn new(host: Arc<HubHost>) -> Self {
        Self {
            host,
            peers: Vec::new(),
        }
    }

    /// Add a peer to the transport.
    pub fn add_peer(&mut self, peer_id: u16, doorbell: Doorbell) {
        self.peers.push(HostPeerHandle::new(peer_id, doorbell));
    }

    /// Get the allocator.
    pub fn allocator(&self) -> &HubAllocator {
        self.host.allocator()
    }

    /// Send a frame to a specific peer.
    pub async fn send_to_peer(
        &self,
        peer_id: u16,
        desc: &MsgDescHot,
        payload: &[u8],
    ) -> Result<(), HubTransportError> {
        let peer_handle = self
            .peers
            .iter()
            .find(|p| p.peer_id == peer_id)
            .ok_or(HubTransportError::PeerNotFound)?;

        let mut desc = *desc;

        if payload.len() <= INLINE_PAYLOAD_SIZE {
            desc.payload_slot = INLINE_PAYLOAD_SLOT;
            desc.payload_generation = 0;
            desc.payload_offset = 0;
            desc.payload_len = payload.len() as u32;
            desc.inline_payload[..payload.len()].copy_from_slice(payload);
        } else {
            let (class, global_index, generation) = self
                .allocator()
                .alloc(payload.len(), 0) // Host owns the slot
                .map_err(HubTransportError::SlotAlloc)?;

            let slot_ptr = unsafe { self.allocator().slot_data_ptr(class as usize, global_index) };
            unsafe {
                std::ptr::copy_nonoverlapping(payload.as_ptr(), slot_ptr, payload.len());
            }

            self.allocator()
                .mark_in_flight(class, global_index, generation)
                .map_err(HubTransportError::SlotAlloc)?;

            desc.payload_slot = encode_slot_ref(class, global_index);
            desc.payload_generation = generation;
            desc.payload_offset = 0;
            desc.payload_len = payload.len() as u32;
        }

        // Enqueue to peer's recv ring
        let recv_ring = self.host.peer_recv_ring(peer_id);

        loop {
            {
                let mut local_head = peer_handle.local_send_head.lock().await;
                if recv_ring.enqueue(&mut local_head, &desc).is_ok() {
                    break;
                }
            }

            // Use async futex wait to avoid blocking the executor
            let futex = self.host.peer_recv_data_futex(peer_id);
            let _ = futex::futex_wait_async_ptr(futex, Some(std::time::Duration::from_millis(100)))
                .await;
        }

        // Signal peer
        peer_handle.doorbell.signal();
        futex::futex_signal(self.host.peer_recv_data_futex(peer_id));

        Ok(())
    }

    /// Receive a frame from any peer.
    ///
    /// Returns (peer_id, descriptor, payload).
    pub async fn recv_from_any(&self) -> Result<(u16, MsgDescHot, Vec<u8>), HubTransportError> {
        loop {
            // Try to dequeue from each peer's send ring (round-robin)
            for peer_handle in &self.peers {
                let send_ring = self.host.peer_send_ring(peer_handle.peer_id);

                if let Some(desc) = send_ring.dequeue() {
                    let payload = if desc.payload_slot == INLINE_PAYLOAD_SLOT {
                        desc.inline_payload[..desc.payload_len as usize].to_vec()
                    } else {
                        let (class, global_index) = decode_slot_ref(desc.payload_slot);
                        let slot_ptr =
                            unsafe { self.allocator().slot_data_ptr(class as usize, global_index) };
                        let slot_ptr = unsafe { slot_ptr.add(desc.payload_offset as usize) };
                        let payload = unsafe {
                            std::slice::from_raw_parts(slot_ptr, desc.payload_len as usize)
                        }
                        .to_vec();

                        // Free the slot
                        self.allocator()
                            .free(class, global_index, desc.payload_generation)
                            .map_err(HubTransportError::SlotAlloc)?;

                        payload
                    };

                    // Signal space available
                    futex::futex_signal(self.host.peer_send_data_futex(peer_handle.peer_id));

                    return Ok((peer_handle.peer_id, desc, payload));
                }
            }

            // No data from any peer, wait on any doorbell
            // For now, use a simple timeout-based approach
            // TODO: Use select! to wait on multiple doorbells
            tokio::time::sleep(std::time::Duration::from_micros(100)).await;

            // Also check doorbells
            for peer_handle in &self.peers {
                peer_handle.doorbell.drain();
            }
        }
    }

    /// Try to receive a frame from any peer without blocking.
    pub fn try_recv_from_any(&self) -> Option<(u16, MsgDescHot, Vec<u8>)> {
        for peer_handle in &self.peers {
            let send_ring = self.host.peer_send_ring(peer_handle.peer_id);

            if let Some(desc) = send_ring.dequeue() {
                let payload = if desc.payload_slot == INLINE_PAYLOAD_SLOT {
                    desc.inline_payload[..desc.payload_len as usize].to_vec()
                } else {
                    let (class, global_index) = decode_slot_ref(desc.payload_slot);
                    let slot_ptr =
                        unsafe { self.allocator().slot_data_ptr(class as usize, global_index) };
                    let slot_ptr = unsafe { slot_ptr.add(desc.payload_offset as usize) };
                    let payload =
                        unsafe { std::slice::from_raw_parts(slot_ptr, desc.payload_len as usize) }
                            .to_vec();

                    // Free the slot
                    if let Err(e) =
                        self.allocator()
                            .free(class, global_index, desc.payload_generation)
                    {
                        tracing::warn!("Failed to free slot: {:?}", e);
                    }

                    payload
                };

                futex::futex_signal(self.host.peer_send_data_futex(peer_handle.peer_id));

                return Some((peer_handle.peer_id, desc, payload));
            }
        }

        None
    }

    /// Get the number of connected peers.
    pub fn peer_count(&self) -> usize {
        self.peers.len()
    }
}

/// Errors from hub transport operations.
#[derive(Debug)]
pub enum HubTransportError {
    /// Slot allocation error.
    SlotAlloc(HubSlotError),
    /// I/O error.
    Io(std::io::Error),
    /// Peer not found.
    PeerNotFound,
    /// Peer disconnected.
    PeerDisconnected,
}

impl std::fmt::Display for HubTransportError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::SlotAlloc(e) => write!(f, "slot allocation error: {}", e),
            Self::Io(e) => write!(f, "I/O error: {}", e),
            Self::PeerNotFound => write!(f, "peer not found"),
            Self::PeerDisconnected => write!(f, "peer disconnected"),
        }
    }
}

impl std::error::Error for HubTransportError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::SlotAlloc(e) => Some(e),
            Self::Io(e) => Some(e),
            _ => None,
        }
    }
}

impl From<std::io::Error> for HubTransportError {
    fn from(e: std::io::Error) -> Self {
        Self::Io(e)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::hub_session::{HubConfig, HubHost};
    use std::path::PathBuf;

    fn test_hub_path() -> PathBuf {
        std::env::temp_dir().join(format!("test_hub_transport_{}.shm", std::process::id()))
    }

    #[tokio::test]
    async fn test_hub_transport_roundtrip() {
        let path = test_hub_path();

        // Create hub
        let host = Arc::new(HubHost::create(&path, HubConfig::default()).unwrap());

        // Add a peer
        let peer_info = host.add_peer().unwrap();
        let peer_id = peer_info.peer_id;

        // Create host transport
        let mut host_transport = HubHostTransport::new(host.clone());
        host_transport.add_peer(peer_id, peer_info.doorbell);

        // Create peer transport
        let peer = Arc::new(HubPeer::open(&path, peer_id).unwrap());
        peer.register();

        let peer_doorbell = Doorbell::from_raw_fd(peer_info.peer_doorbell_fd).unwrap();
        let peer_transport = HubPeerTransport::new(peer, peer_doorbell, "test-peer");

        // Send from peer to host
        let desc = MsgDescHot {
            msg_id: 42,
            channel_id: 1,
            ..Default::default()
        };

        let payload = b"hello from peer";
        peer_transport.send_frame(&desc, payload).await.unwrap();

        // Receive on host
        let (recv_peer_id, recv_desc, recv_payload) = host_transport.recv_from_any().await.unwrap();
        assert_eq!(recv_peer_id, peer_id);
        assert_eq!(recv_desc.msg_id, 42);
        assert_eq!(recv_payload, payload);

        // Send from host to peer
        let desc2 = MsgDescHot {
            msg_id: 100,
            channel_id: 2,
            ..Default::default()
        };

        let payload2 = b"hello from host";
        host_transport
            .send_to_peer(peer_id, &desc2, payload2)
            .await
            .unwrap();

        // Receive on peer
        let (recv_desc2, recv_payload2) = peer_transport.recv_frame().await.unwrap();
        assert_eq!(recv_desc2.msg_id, 100);
        assert_eq!(recv_payload2, payload2);

        // Cleanup
        std::fs::remove_file(&path).ok();
    }

    #[tokio::test]
    async fn test_hub_transport_large_payload() {
        let path = test_hub_path();
        let path = path.with_file_name(format!(
            "test_hub_transport_large_{}.shm",
            std::process::id()
        ));

        let host = Arc::new(HubHost::create(&path, HubConfig::default()).unwrap());
        let peer_info = host.add_peer().unwrap();
        let peer_id = peer_info.peer_id;

        let mut host_transport = HubHostTransport::new(host.clone());
        host_transport.add_peer(peer_id, peer_info.doorbell);

        let peer = Arc::new(HubPeer::open(&path, peer_id).unwrap());
        peer.register();

        let peer_doorbell = Doorbell::from_raw_fd(peer_info.peer_doorbell_fd).unwrap();
        let peer_transport = HubPeerTransport::new(peer, peer_doorbell, "test-peer");

        // Send a large payload (larger than inline size)
        let desc = MsgDescHot {
            msg_id: 1,
            ..Default::default()
        };

        let payload = vec![42u8; 1000]; // 1KB payload
        peer_transport.send_frame(&desc, &payload).await.unwrap();

        // Receive on host
        let (_, _, recv_payload) = host_transport.recv_from_any().await.unwrap();
        assert_eq!(recv_payload, payload);

        std::fs::remove_file(&path).ok();
    }
}
