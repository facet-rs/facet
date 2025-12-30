//! Hub transport adapters.
//!
//! Ported from `rapace-transport-shm` hub transport, adapted to the unified
//! `Frame` + `Transport` API in `rapace-core`.

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

use tokio::sync::Mutex as AsyncMutex;

use crate::{
    BufferPool, EncodeError, Frame, INLINE_PAYLOAD_SIZE, INLINE_PAYLOAD_SLOT, Payload,
    TransportError, ValidationError,
};

use super::hub_layout::{HubSlotError, decode_slot_ref, encode_slot_ref};
use super::hub_session::{HubHost, HubPeer};
use crate::transport::Transport;
use shm_primitives::{Doorbell, SignalResult, futex};

/// Callback invoked when a peer dies.
///
/// The callback receives the peer_id of the dead peer. This can be used
/// to trigger automatic relaunching of the cell.
pub type PeerDeathCallback = Arc<dyn Fn(u16) + Send + Sync + 'static>;

fn hub_debug_enabled() -> bool {
    std::env::var_os("RAPACE_HUB_DEBUG").is_some() || std::env::var_os("RAPACE_DEBUG").is_some()
}

fn slot_error_to_transport(e: HubSlotError) -> TransportError {
    match e {
        HubSlotError::NoFreeSlots => TransportError::Encode(EncodeError::NoSlotAvailable),
        HubSlotError::PayloadTooLarge { len, max } => {
            TransportError::Validation(ValidationError::PayloadTooLarge {
                len: len as u32,
                max: max as u32,
            })
        }
        HubSlotError::StaleGeneration => {
            TransportError::Validation(ValidationError::StaleGeneration {
                expected: 0,
                actual: 0,
            })
        }
        HubSlotError::InvalidSlotRef
        | HubSlotError::InvalidState
        | HubSlotError::InvalidSizeClass
        | HubSlotError::InvalidExtent => {
            TransportError::Encode(EncodeError::EncodeFailed(e.to_string()))
        }
    }
}

// =============================================================================
// Plugin-side hub transport
// =============================================================================

#[derive(Clone)]
pub struct HubPeerTransport {
    inner: Arc<HubPeerTransportInner>,
}

struct HubPeerTransportInner {
    peer: Arc<HubPeer>,
    doorbell: Doorbell,
    local_send_head: AsyncMutex<u64>,
    closed: AtomicBool,
    name: String,
    buffer_pool: BufferPool,
}

impl std::fmt::Debug for HubPeerTransport {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("HubPeerTransport")
            .field("peer_id", &self.inner.peer.peer_id())
            .field("name", &self.inner.name)
            .finish_non_exhaustive()
    }
}

impl HubPeerTransport {
    pub fn new(peer: Arc<HubPeer>, doorbell: Doorbell, name: impl Into<String>) -> Self {
        Self::with_buffer_pool(peer, doorbell, name, BufferPool::new())
    }

    pub fn with_buffer_pool(
        peer: Arc<HubPeer>,
        doorbell: Doorbell,
        name: impl Into<String>,
        buffer_pool: BufferPool,
    ) -> Self {
        Self {
            inner: Arc::new(HubPeerTransportInner {
                peer,
                doorbell,
                local_send_head: AsyncMutex::new(0),
                closed: AtomicBool::new(false),
                name: name.into(),
                buffer_pool,
            }),
        }
    }

    pub fn peer(&self) -> &Arc<HubPeer> {
        &self.inner.peer
    }
}

impl Transport for HubPeerTransport {
    async fn send_frame(&self, frame: Frame) -> Result<(), TransportError> {
        if self.is_closed() {
            return Err(TransportError::Closed);
        }

        self.inner.peer.update_heartbeat();

        let mut desc = frame.desc;
        let payload = frame.payload_bytes();

        if payload.len() <= INLINE_PAYLOAD_SIZE {
            desc.payload_slot = INLINE_PAYLOAD_SLOT;
            desc.payload_generation = 0;
            desc.payload_offset = 0;
            desc.payload_len = payload.len() as u32;
            desc.inline_payload[..payload.len()].copy_from_slice(payload);
        } else {
            const MAX_WAIT: Duration = Duration::from_secs(10);
            const FUTEX_TIMEOUT: Duration = Duration::from_millis(50);

            let mut retries = 0u32;
            let start = std::time::Instant::now();
            let (class, global_index, generation) = loop {
                match self
                    .inner
                    .peer
                    .allocator()
                    .alloc(payload.len(), self.inner.peer.peer_id() as u32)
                {
                    Ok(result) => break result,
                    Err(HubSlotError::NoFreeSlots) => {
                        if start.elapsed() >= MAX_WAIT {
                            tracing::warn!(
                                transport = ?self.inner.name,
                                retries,
                                payload_len = payload.len(),
                                waited_ms = start.elapsed().as_millis() as u64,
                                "HubPeerTransport slot allocation timed out (no free slots)"
                            );
                            if hub_debug_enabled() {
                                eprintln!(
                                    "[rapace-hub] HubPeerTransport: slot allocation TIMED OUT after {} retries (waited={}ms, payload={}B)",
                                    retries,
                                    start.elapsed().as_millis(),
                                    payload.len()
                                );
                            }
                            return Err(slot_error_to_transport(HubSlotError::NoFreeSlots));
                        }
                        retries += 1;
                        if retries == 1 && hub_debug_enabled() {
                            eprintln!(
                                "[rapace-hub] HubPeerTransport: no free slots; waiting (payload={}B)",
                                payload.len()
                            );
                        }

                        if retries == 10 || retries == 25 || retries == 40 {
                            tracing::debug!(
                                transport = ?self.inner.name,
                                retries,
                                payload_len = payload.len(),
                                waited_ms = start.elapsed().as_millis() as u64,
                                "HubPeerTransport waiting for slot availability"
                            );
                            if hub_debug_enabled() {
                                eprintln!(
                                    "[rapace-hub] HubPeerTransport: waiting for slot (retry={}, waited={}ms, payload={}B)",
                                    retries,
                                    start.elapsed().as_millis(),
                                    payload.len()
                                );
                            }
                        }

                        if self.is_closed() {
                            return Err(TransportError::Closed);
                        }

                        tokio::task::yield_now().await;

                        // Wait for some slot to be freed (cross-process futex).
                        // We use class 0 as a global "some slot freed" futex.
                        let futex = self.inner.peer.allocator().slot_available_futex(0);
                        let _ = futex::futex_wait_async_ptr(futex, Some(FUTEX_TIMEOUT)).await;

                        self.inner.peer.update_heartbeat();
                    }
                    Err(e) => return Err(slot_error_to_transport(e)),
                }
            };

            let slot_ptr = unsafe {
                self.inner
                    .peer
                    .allocator()
                    .slot_data_ptr(class as usize, global_index)
            };
            unsafe {
                std::ptr::copy_nonoverlapping(payload.as_ptr(), slot_ptr, payload.len());
            }

            self.inner
                .peer
                .allocator()
                .mark_in_flight(class, global_index, generation)
                .map_err(slot_error_to_transport)?;

            desc.payload_slot = encode_slot_ref(class, global_index);
            desc.payload_generation = generation;
            desc.payload_offset = 0;
            desc.payload_len = payload.len() as u32;
        }

        let send_ring = self.inner.peer.send_ring();

        loop {
            {
                let mut local_head = self.inner.local_send_head.lock().await;
                if send_ring.enqueue(&mut local_head, &desc).is_ok() {
                    break;
                }
            }

            if self.is_closed() {
                return Err(TransportError::Closed);
            }

            let _ = futex::futex_wait_async_ptr(
                self.inner.peer.send_data_futex(),
                Some(Duration::from_millis(100)),
            )
            .await;
        }

        let signal_result = self.inner.doorbell.signal();
        futex::futex_signal(self.inner.peer.send_data_futex());

        // If host died, mark transport closed (cell will exit via ur-taking-me-with-you)
        if signal_result == SignalResult::PeerDead {
            self.inner.closed.store(true, Ordering::Release);
            return Err(TransportError::Closed);
        }

        Ok(())
    }

    async fn recv_frame(&self) -> Result<Frame, TransportError> {
        if self.is_closed() {
            return Err(TransportError::Closed);
        }

        self.inner.peer.update_heartbeat();

        let recv_ring = self.inner.peer.recv_ring();

        loop {
            if let Some(mut desc) = recv_ring.dequeue() {
                futex::futex_signal(self.inner.peer.recv_data_futex());

                if desc.payload_slot == INLINE_PAYLOAD_SLOT {
                    return Ok(Frame {
                        desc,
                        payload: Payload::Inline,
                    });
                }

                let (class, global_index) = decode_slot_ref(desc.payload_slot);
                let slot_ptr = unsafe {
                    self.inner
                        .peer
                        .allocator()
                        .slot_data_ptr(class as usize, global_index)
                };
                let slot_ptr = unsafe { slot_ptr.add(desc.payload_offset as usize) };
                let payload_slice =
                    unsafe { std::slice::from_raw_parts(slot_ptr, desc.payload_len as usize) };

                // Use pooled buffer to copy payload from SHM
                let mut pooled_buf = self.inner.buffer_pool.get();
                pooled_buf.extend_from_slice(payload_slice);

                if let Err(e) =
                    self.inner
                        .peer
                        .allocator()
                        .free(class, global_index, desc.payload_generation)
                {
                    tracing::warn!(
                        transport = ?self.inner.name,
                        class,
                        global_index,
                        generation = desc.payload_generation,
                        error = %e,
                        "HubPeerTransport slot free failed"
                    );
                    if hub_debug_enabled() {
                        eprintln!(
                            "[rapace-hub] HubPeerTransport::recv_frame: free() FAILED for class={} index={} gen={}: {:?}",
                            class, global_index, desc.payload_generation, e
                        );
                    }
                }

                // Normalize descriptor to match the fact we copied bytes out.
                desc.payload_slot = 0;
                desc.payload_generation = 0;
                desc.payload_offset = 0;
                desc.payload_len = pooled_buf.len() as u32;

                return Ok(Frame {
                    desc,
                    payload: Payload::Pooled(pooled_buf),
                });
            }

            if self.is_closed() {
                return Err(TransportError::Closed);
            }

            let _ = self.inner.doorbell.wait().await;
            self.inner.doorbell.drain();
        }
    }

    fn close(&self) {
        self.inner.closed.store(true, Ordering::Release);
    }

    fn is_closed(&self) -> bool {
        self.inner.closed.load(Ordering::Acquire)
    }

    fn buffer_pool(&self) -> &BufferPool {
        &self.inner.buffer_pool
    }
}

// =============================================================================
// Host-side per-peer hub transport
// =============================================================================

#[derive(Clone)]
pub struct HubHostPeerTransport {
    inner: Arc<HubHostPeerTransportInner>,
}

struct HubHostPeerTransportInner {
    host: Arc<HubHost>,
    peer_id: u16,
    /// Human-readable name for the peer (e.g., cell name).
    peer_name: String,
    doorbell: Doorbell,
    local_send_head: AsyncMutex<u64>,
    closed: AtomicBool,
    /// Whether we've already logged that the peer died (to avoid spam).
    peer_death_logged: AtomicBool,
    /// Optional callback to invoke when peer death is detected.
    on_peer_death: Option<PeerDeathCallback>,
    buffer_pool: BufferPool,
}

impl std::fmt::Debug for HubHostPeerTransport {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("HubHostPeerTransport")
            .field("peer_id", &self.inner.peer_id)
            .finish_non_exhaustive()
    }
}

impl HubHostPeerTransport {
    pub fn new(host: Arc<HubHost>, peer_id: u16, doorbell: Doorbell) -> Self {
        Self::with_name(host, peer_id, doorbell, format!("peer-{}", peer_id))
    }

    /// Create a transport with a human-readable name for the peer.
    pub fn with_name(
        host: Arc<HubHost>,
        peer_id: u16,
        doorbell: Doorbell,
        peer_name: impl Into<String>,
    ) -> Self {
        Self::with_options(host, peer_id, doorbell, peer_name, None, BufferPool::new())
    }

    pub fn with_buffer_pool(
        host: Arc<HubHost>,
        peer_id: u16,
        doorbell: Doorbell,
        buffer_pool: BufferPool,
    ) -> Self {
        Self::with_options(
            host,
            peer_id,
            doorbell,
            format!("peer-{}", peer_id),
            None,
            buffer_pool,
        )
    }

    /// Create a transport with full configuration options.
    pub fn with_options(
        host: Arc<HubHost>,
        peer_id: u16,
        doorbell: Doorbell,
        peer_name: impl Into<String>,
        on_peer_death: Option<PeerDeathCallback>,
        buffer_pool: BufferPool,
    ) -> Self {
        Self {
            inner: Arc::new(HubHostPeerTransportInner {
                host,
                peer_id,
                peer_name: peer_name.into(),
                doorbell,
                local_send_head: AsyncMutex::new(0),
                closed: AtomicBool::new(false),
                peer_death_logged: AtomicBool::new(false),
                on_peer_death,
                buffer_pool,
            }),
        }
    }

    /// Set the peer name (for logging purposes).
    ///
    /// This is typically called after the cell sends its `ready` message
    /// with its actual name.
    pub fn set_peer_name(&self, name: impl Into<String>) {
        // Note: This requires interior mutability. For now, we'll just
        // log with peer_id if name wasn't set at construction time.
        // A real implementation would use a Mutex<String> or similar.
        let _ = name;
    }

    pub fn host(&self) -> &Arc<HubHost> {
        &self.inner.host
    }

    pub fn peer_id(&self) -> u16 {
        self.inner.peer_id
    }

    pub fn peer_name(&self) -> &str {
        &self.inner.peer_name
    }

    fn allocator(&self) -> &super::hub_alloc::HubAllocator {
        self.inner.host.allocator()
    }

    /// Handle a signal result, logging and closing if the peer is dead.
    fn handle_signal_result(&self, result: SignalResult) {
        if result == SignalResult::PeerDead {
            // Only log once per transport
            if !self.inner.peer_death_logged.swap(true, Ordering::Relaxed) {
                tracing::warn!(
                    peer_id = self.inner.peer_id,
                    peer_name = %self.inner.peer_name,
                    "Cell died (doorbell signal failed)"
                );

                // Invoke callback if set
                if let Some(ref callback) = self.inner.on_peer_death {
                    callback(self.inner.peer_id);
                }
            }

            // Mark transport as closed
            self.inner.closed.store(true, Ordering::Release);
        }
    }
}

impl Transport for HubHostPeerTransport {
    async fn send_frame(&self, frame: Frame) -> Result<(), TransportError> {
        if self.is_closed() {
            return Err(TransportError::Closed);
        }

        let mut desc = frame.desc;
        let payload = frame.payload_bytes();

        if payload.len() <= INLINE_PAYLOAD_SIZE {
            desc.payload_slot = INLINE_PAYLOAD_SLOT;
            desc.payload_generation = 0;
            desc.payload_offset = 0;
            desc.payload_len = payload.len() as u32;
            desc.inline_payload[..payload.len()].copy_from_slice(payload);
        } else {
            const MAX_WAIT: Duration = Duration::from_secs(10);
            const FUTEX_TIMEOUT: Duration = Duration::from_millis(50);

            let mut retries = 0u32;
            let start = std::time::Instant::now();
            let (class, global_index, generation) = loop {
                match self
                    .allocator()
                    .alloc(payload.len(), self.inner.peer_id as u32)
                {
                    Ok(result) => break result,
                    Err(HubSlotError::NoFreeSlots) => {
                        if start.elapsed() >= MAX_WAIT {
                            tracing::warn!(
                                peer_id = self.inner.peer_id,
                                retries,
                                payload_len = payload.len(),
                                waited_ms = start.elapsed().as_millis() as u64,
                                "HubHostPeerTransport slot allocation timed out (no free slots)"
                            );
                            if hub_debug_enabled() {
                                eprintln!(
                                    "[rapace-hub] HubHostPeerTransport: slot allocation TIMED OUT after {} retries (waited={}ms, peer={}, payload={}B)",
                                    retries,
                                    start.elapsed().as_millis(),
                                    self.inner.peer_id,
                                    payload.len()
                                );
                            }
                            return Err(slot_error_to_transport(HubSlotError::NoFreeSlots));
                        }
                        retries += 1;
                        if retries == 1 && hub_debug_enabled() {
                            eprintln!(
                                "[rapace-hub] HubHostPeerTransport: no free slots; waiting (peer={}, payload={}B)",
                                self.inner.peer_id,
                                payload.len()
                            );
                        }

                        if retries == 10 || retries == 25 || retries == 40 {
                            tracing::debug!(
                                peer_id = self.inner.peer_id,
                                retries,
                                payload_len = payload.len(),
                                waited_ms = start.elapsed().as_millis() as u64,
                                "HubHostPeerTransport waiting for slot availability"
                            );
                            if hub_debug_enabled() {
                                eprintln!(
                                    "[rapace-hub] HubHostPeerTransport: waiting for slot (peer={}, retry={}, waited={}ms, payload={}B)",
                                    self.inner.peer_id,
                                    retries,
                                    start.elapsed().as_millis(),
                                    payload.len()
                                );
                            }
                        }

                        if self.is_closed() {
                            return Err(TransportError::Closed);
                        }

                        tokio::task::yield_now().await;

                        // Wait for some slot to be freed (cross-process futex).
                        // Use class 0 as a global "some slot freed" futex.
                        let futex = self.allocator().slot_available_futex(0);
                        let _ = futex::futex_wait_async_ptr(futex, Some(FUTEX_TIMEOUT)).await;
                    }
                    Err(e) => return Err(slot_error_to_transport(e)),
                }
            };

            let slot_ptr = unsafe { self.allocator().slot_data_ptr(class as usize, global_index) };
            unsafe {
                std::ptr::copy_nonoverlapping(payload.as_ptr(), slot_ptr, payload.len());
            }

            self.allocator()
                .mark_in_flight(class, global_index, generation)
                .map_err(slot_error_to_transport)?;

            desc.payload_slot = encode_slot_ref(class, global_index);
            desc.payload_generation = generation;
            desc.payload_offset = 0;
            desc.payload_len = payload.len() as u32;
        }

        let recv_ring = self.inner.host.peer_recv_ring(self.inner.peer_id);
        const FUTEX_TIMEOUT: Duration = Duration::from_millis(100);

        loop {
            {
                let mut local_head = self.inner.local_send_head.lock().await;
                if recv_ring.enqueue(&mut local_head, &desc).is_ok() {
                    break;
                }
            }

            if self.is_closed() {
                return Err(TransportError::Closed);
            }

            let futex = self.inner.host.peer_recv_data_futex(self.inner.peer_id);
            let _ = futex::futex_wait_async_ptr(futex, Some(FUTEX_TIMEOUT)).await;
        }

        let signal_result = self.inner.doorbell.signal();
        self.handle_signal_result(signal_result);
        futex::futex_signal(self.inner.host.peer_recv_data_futex(self.inner.peer_id));

        // If peer died during send, return error
        if signal_result == SignalResult::PeerDead {
            return Err(TransportError::Closed);
        }

        Ok(())
    }

    async fn recv_frame(&self) -> Result<Frame, TransportError> {
        if self.is_closed() {
            return Err(TransportError::Closed);
        }

        let send_ring = self.inner.host.peer_send_ring(self.inner.peer_id);

        loop {
            if let Some(mut desc) = send_ring.dequeue() {
                futex::futex_signal(self.inner.host.peer_send_data_futex(self.inner.peer_id));

                if desc.payload_slot == INLINE_PAYLOAD_SLOT {
                    return Ok(Frame {
                        desc,
                        payload: Payload::Inline,
                    });
                }

                let (class, global_index) = decode_slot_ref(desc.payload_slot);
                let slot_ptr =
                    unsafe { self.allocator().slot_data_ptr(class as usize, global_index) };
                let slot_ptr = unsafe { slot_ptr.add(desc.payload_offset as usize) };
                let payload_slice =
                    unsafe { std::slice::from_raw_parts(slot_ptr, desc.payload_len as usize) };

                // Use pooled buffer to copy payload from SHM
                let mut pooled_buf = self.inner.buffer_pool.get();
                pooled_buf.extend_from_slice(payload_slice);

                if let Err(e) = self
                    .allocator()
                    .free(class, global_index, desc.payload_generation)
                {
                    tracing::warn!(
                        peer_id = self.inner.peer_id,
                        class,
                        global_index,
                        generation = desc.payload_generation,
                        error = %e,
                        "HubHostPeerTransport slot free failed"
                    );
                    if hub_debug_enabled() {
                        eprintln!(
                            "[rapace-hub] HubHostPeerTransport::recv_frame: free() FAILED for class={} index={} gen={}: {:?}",
                            class, global_index, desc.payload_generation, e
                        );
                    }
                }

                desc.payload_slot = 0;
                desc.payload_generation = 0;
                desc.payload_offset = 0;
                desc.payload_len = pooled_buf.len() as u32;

                return Ok(Frame {
                    desc,
                    payload: Payload::Pooled(pooled_buf),
                });
            }

            if self.is_closed() {
                return Err(TransportError::Closed);
            }

            let _ = self.inner.doorbell.wait().await;
            self.inner.doorbell.drain();
        }
    }

    fn close(&self) {
        self.inner.closed.store(true, Ordering::Release);
    }

    fn is_closed(&self) -> bool {
        self.inner.closed.load(Ordering::Acquire)
    }

    fn buffer_pool(&self) -> &BufferPool {
        &self.inner.buffer_pool
    }
}
