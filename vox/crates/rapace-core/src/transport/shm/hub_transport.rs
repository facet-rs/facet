//! Hub transport adapters.
//!
//! Ported from `rapace-transport-shm` hub transport, adapted to the unified
//! `Frame` + `TransportBackend` API in `rapace-core`.

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

use tokio::sync::Mutex as AsyncMutex;

use crate::{
    EncodeError, Frame, INLINE_PAYLOAD_SIZE, INLINE_PAYLOAD_SLOT, Payload, TransportError,
    ValidationError,
};

use super::doorbell::Doorbell;
use super::futex;
use super::hub_layout::{HubSlotError, decode_slot_ref, encode_slot_ref};
use super::hub_session::{HubHost, HubPeer};
use crate::transport::TransportBackend;

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
        Self {
            inner: Arc::new(HubPeerTransportInner {
                peer,
                doorbell,
                local_send_head: AsyncMutex::new(0),
                closed: AtomicBool::new(false),
                name: name.into(),
            }),
        }
    }

    pub fn peer(&self) -> &Arc<HubPeer> {
        &self.inner.peer
    }
}

impl TransportBackend for HubPeerTransport {
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
            let (class, global_index, generation) = self
                .inner
                .peer
                .allocator()
                .alloc(payload.len(), self.inner.peer.peer_id() as u32)
                .map_err(slot_error_to_transport)?;

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

        self.inner.doorbell.signal();
        futex::futex_signal(self.inner.peer.send_data_futex());
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
                let payload =
                    unsafe { std::slice::from_raw_parts(slot_ptr, desc.payload_len as usize) }
                        .to_vec();

                let _ =
                    self.inner
                        .peer
                        .allocator()
                        .free(class, global_index, desc.payload_generation);

                // Normalize descriptor to match the fact we copied bytes out.
                desc.payload_slot = 0;
                desc.payload_generation = 0;
                desc.payload_offset = 0;
                desc.payload_len = payload.len() as u32;

                return Ok(Frame {
                    desc,
                    payload: Payload::Owned(payload),
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
    doorbell: Doorbell,
    local_send_head: AsyncMutex<u64>,
    closed: AtomicBool,
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
        Self {
            inner: Arc::new(HubHostPeerTransportInner {
                host,
                peer_id,
                doorbell,
                local_send_head: AsyncMutex::new(0),
                closed: AtomicBool::new(false),
            }),
        }
    }

    pub fn host(&self) -> &Arc<HubHost> {
        &self.inner.host
    }

    pub fn peer_id(&self) -> u16 {
        self.inner.peer_id
    }

    fn allocator(&self) -> &super::hub_alloc::HubAllocator {
        self.inner.host.allocator()
    }
}

impl TransportBackend for HubHostPeerTransport {
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
            let (class, global_index, generation) = self
                .allocator()
                .alloc(payload.len(), self.inner.peer_id as u32)
                .map_err(slot_error_to_transport)?;

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

        self.inner.doorbell.signal();
        futex::futex_signal(self.inner.host.peer_recv_data_futex(self.inner.peer_id));
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
                let payload =
                    unsafe { std::slice::from_raw_parts(slot_ptr, desc.payload_len as usize) }
                        .to_vec();

                let _ = self
                    .allocator()
                    .free(class, global_index, desc.payload_generation);

                desc.payload_slot = 0;
                desc.payload_generation = 0;
                desc.payload_offset = 0;
                desc.payload_len = payload.len() as u32;

                return Ok(Frame {
                    desc,
                    payload: Payload::Owned(payload),
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
}
