//! SHM transport implementation.

use std::sync::Arc;
use std::sync::atomic::Ordering;
use std::time::Duration;

use crate::{
    EncodeError, Frame, INLINE_PAYLOAD_SIZE, INLINE_PAYLOAD_SLOT, Payload, TransportError,
    ValidationError,
};
use tokio::sync::Notify;

use super::futex::futex_signal;
use super::hub_transport::{HubHostPeerTransport, HubPeerTransport};
use super::layout::{RingError, SlotError};
use super::session::ShmSession;
use crate::transport::TransportBackend;

/// Convert SHM-specific errors to TransportError.
fn slot_error_to_transport(e: SlotError, context: &str) -> TransportError {
    match e {
        SlotError::NoFreeSlots => TransportError::Encode(EncodeError::NoSlotAvailable),
        SlotError::InvalidIndex => TransportError::Validation(ValidationError::SlotOutOfBounds {
            slot: u32::MAX,
            max: 0,
        }),
        SlotError::StaleGeneration => {
            TransportError::Validation(ValidationError::StaleGeneration {
                expected: 0,
                actual: 0,
            })
        }
        SlotError::InvalidState => TransportError::Encode(EncodeError::EncodeFailed(format!(
            "{}: invalid state",
            context
        ))),
        SlotError::PayloadTooLarge { len, max } => {
            TransportError::Validation(ValidationError::PayloadTooLarge {
                len: len as u32,
                max: max as u32,
            })
        }
    }
}

/// SHM transport (pair or hub mode).
#[derive(Clone, Debug)]
pub enum ShmTransport {
    /// Two-peer SHM session transport (A↔B).
    Pair(PairTransport),
    /// Hub plugin-side transport (peer↔host).
    HubPeer(HubPeerTransport),
    /// Hub host-side per-peer transport (host↔peer).
    HubHostPeer(HubHostPeerTransport),
}

#[derive(Clone)]
pub struct PairTransport {
    inner: Arc<PairTransportInner>,
}

impl std::fmt::Debug for PairTransport {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("PairTransport").finish_non_exhaustive()
    }
}

struct PairTransportInner {
    /// The underlying SHM session.
    session: Arc<ShmSession>,
    /// Whether the transport is closed.
    closed: std::sync::atomic::AtomicBool,
    /// Optional metrics for tracking zero-copy performance.
    metrics: Option<Arc<ShmMetrics>>,
    /// Optional name for tracing/debugging purposes.
    name: Option<String>,
    /// Notify waiters when a slot is freed (in-process notification).
    /// This complements the futex for faster in-process wakeups.
    slot_freed_notify: Arc<Notify>,
    /// Buffer pool for serialization.
    buffer_pool: crate::BufferPool,
}

impl PairTransport {
    /// Create a new SHM transport from a session.
    pub fn new(session: Arc<ShmSession>) -> Self {
        Self {
            inner: Arc::new(PairTransportInner {
                session,
                closed: std::sync::atomic::AtomicBool::new(false),
                metrics: None,
                name: None,
                slot_freed_notify: Arc::new(Notify::new()),
                buffer_pool: crate::BufferPool::new(),
            }),
        }
    }

    /// Create a new SHM transport with a name for tracing.
    pub fn with_name(session: Arc<ShmSession>, name: impl Into<String>) -> Self {
        Self {
            inner: Arc::new(PairTransportInner {
                session,
                closed: std::sync::atomic::AtomicBool::new(false),
                metrics: None,
                name: Some(name.into()),
                slot_freed_notify: Arc::new(Notify::new()),
                buffer_pool: crate::BufferPool::new(),
            }),
        }
    }

    /// Create a new SHM transport with metrics enabled.
    pub fn new_with_metrics(session: Arc<ShmSession>, metrics: Arc<ShmMetrics>) -> Self {
        Self {
            inner: Arc::new(PairTransportInner {
                session,
                closed: std::sync::atomic::AtomicBool::new(false),
                metrics: Some(metrics),
                name: None,
                slot_freed_notify: Arc::new(Notify::new()),
                buffer_pool: crate::BufferPool::new(),
            }),
        }
    }

    /// Create a new SHM transport with name and metrics.
    pub fn with_name_and_metrics(
        session: Arc<ShmSession>,
        name: impl Into<String>,
        metrics: Arc<ShmMetrics>,
    ) -> Self {
        Self {
            inner: Arc::new(PairTransportInner {
                session,
                closed: std::sync::atomic::AtomicBool::new(false),
                metrics: Some(metrics),
                name: Some(name.into()),
                slot_freed_notify: Arc::new(Notify::new()),
                buffer_pool: crate::BufferPool::new(),
            }),
        }
    }

    /// Create a connected pair of SHM transports for testing.
    pub fn pair() -> Result<(Self, Self), TransportError> {
        let (session_a, session_b) = ShmSession::create_pair().map_err(|e| {
            TransportError::Encode(EncodeError::EncodeFailed(format!(
                "failed to create SHM session pair: {}",
                e
            )))
        })?;

        Ok((Self::new(session_a), Self::new(session_b)))
    }

    /// Create a connected pair of SHM transports with shared metrics.
    ///
    /// Both transports will report to the same metrics instance.
    pub fn pair_with_metrics(metrics: Arc<ShmMetrics>) -> Result<(Self, Self), TransportError> {
        let (session_a, session_b) = ShmSession::create_pair().map_err(|e| {
            TransportError::Encode(EncodeError::EncodeFailed(format!(
                "failed to create SHM session pair: {}",
                e
            )))
        })?;

        Ok((
            Self::new_with_metrics(session_a, metrics.clone()),
            Self::new_with_metrics(session_b, metrics),
        ))
    }

    /// Check if the transport is closed.
    #[inline]
    pub fn is_closed(&self) -> bool {
        self.inner.closed.load(Ordering::Acquire)
    }

    /// Get the underlying session.
    #[inline]
    pub fn session(&self) -> &Arc<ShmSession> {
        &self.inner.session
    }

    /// Get the metrics instance, if enabled.
    #[inline]
    pub fn metrics(&self) -> Option<&Arc<ShmMetrics>> {
        self.inner.metrics.as_ref()
    }
}

impl ShmTransport {
    pub fn new(session: Arc<ShmSession>) -> Self {
        ShmTransport::Pair(PairTransport::new(session))
    }

    pub fn with_name(session: Arc<ShmSession>, name: impl Into<String>) -> Self {
        ShmTransport::Pair(PairTransport::with_name(session, name))
    }

    pub fn new_with_metrics(session: Arc<ShmSession>, metrics: Arc<ShmMetrics>) -> Self {
        ShmTransport::Pair(PairTransport::new_with_metrics(session, metrics))
    }

    pub fn with_name_and_metrics(
        session: Arc<ShmSession>,
        name: impl Into<String>,
        metrics: Arc<ShmMetrics>,
    ) -> Self {
        ShmTransport::Pair(PairTransport::with_name_and_metrics(session, name, metrics))
    }

    pub fn pair() -> Result<(Self, Self), TransportError> {
        let (a, b) = PairTransport::pair()?;
        Ok((ShmTransport::Pair(a), ShmTransport::Pair(b)))
    }

    pub fn pair_with_metrics(metrics: Arc<ShmMetrics>) -> Result<(Self, Self), TransportError> {
        let (a, b) = PairTransport::pair_with_metrics(metrics)?;
        Ok((ShmTransport::Pair(a), ShmTransport::Pair(b)))
    }

    pub fn hub_peer(
        peer: Arc<super::hub_session::HubPeer>,
        doorbell: super::Doorbell,
        name: impl Into<String>,
    ) -> Self {
        ShmTransport::HubPeer(HubPeerTransport::new(peer, doorbell, name))
    }

    pub fn hub_host_peer(
        host: Arc<super::hub_session::HubHost>,
        peer_id: u16,
        doorbell: super::Doorbell,
    ) -> Self {
        ShmTransport::HubHostPeer(HubHostPeerTransport::new(host, peer_id, doorbell))
    }

    pub fn is_closed(&self) -> bool {
        match self {
            ShmTransport::Pair(t) => t.is_closed(),
            ShmTransport::HubPeer(t) => t.is_closed(),
            ShmTransport::HubHostPeer(t) => t.is_closed(),
        }
    }
}

impl TransportBackend for ShmTransport {
    async fn send_frame(&self, frame: Frame) -> Result<(), TransportError> {
        match self {
            ShmTransport::Pair(t) => TransportBackend::send_frame(t, frame).await,
            ShmTransport::HubPeer(t) => TransportBackend::send_frame(t, frame).await,
            ShmTransport::HubHostPeer(t) => TransportBackend::send_frame(t, frame).await,
        }
    }

    async fn recv_frame(&self) -> Result<Frame, TransportError> {
        match self {
            ShmTransport::Pair(t) => TransportBackend::recv_frame(t).await,
            ShmTransport::HubPeer(t) => TransportBackend::recv_frame(t).await,
            ShmTransport::HubHostPeer(t) => TransportBackend::recv_frame(t).await,
        }
    }

    fn close(&self) {
        match self {
            ShmTransport::Pair(t) => TransportBackend::close(t),
            ShmTransport::HubPeer(t) => TransportBackend::close(t),
            ShmTransport::HubHostPeer(t) => TransportBackend::close(t),
        }
    }

    fn is_closed(&self) -> bool {
        ShmTransport::is_closed(self)
    }

    fn buffer_pool(&self) -> &crate::BufferPool {
        match self {
            ShmTransport::Pair(t) => TransportBackend::buffer_pool(t),
            ShmTransport::HubPeer(t) => TransportBackend::buffer_pool(t),
            ShmTransport::HubHostPeer(t) => TransportBackend::buffer_pool(t),
        }
    }
}

impl TransportBackend for PairTransport {
    async fn send_frame(&self, frame: Frame) -> Result<(), TransportError> {
        if self.is_closed() {
            return Err(TransportError::Closed);
        }

        // Update our heartbeat when sending to signal we're alive
        self.inner.session.update_heartbeat();

        let send_ring = self.inner.session.send_ring();
        let data_segment = self.inner.session.data_segment();

        let Frame { mut desc, payload } = frame;

        let payload_len = payload.len(&desc);

        if payload_len <= INLINE_PAYLOAD_SIZE {
            // Inline payload.
            desc.payload_slot = INLINE_PAYLOAD_SLOT;
            desc.payload_generation = 0;
            desc.payload_offset = 0;
            desc.payload_len = payload_len as u32;

            // Only copy into inline storage if the payload isn't already inline.
            if let Some(payload_bytes) = payload.external_slice() {
                desc.inline_payload[..payload_len].copy_from_slice(payload_bytes);
            }

            if let Some(ref metrics) = self.inner.metrics {
                metrics.record_inline_send();
            }
        } else {
            // Large payloads are never represented as inline payloads.
            let payload_bytes = payload
                .external_slice()
                .expect("inline payloads never reach slot allocation");
            // Need to allocate a slot, waiting if none available.
            const SLOT_FUTEX_TIMEOUT: Duration = Duration::from_millis(100);
            const SLOT_PEER_TIMEOUT_NANOS: u64 = 3_000_000_000; // 3 seconds
            const DEADLOCK_WARN_INTERVAL: Duration = Duration::from_secs(1);

            let wait_start = std::time::Instant::now();
            let mut last_warn = wait_start;
            let mut wait_count = 0u32;

            let (slot_idx, generation) = loop {
                match data_segment.alloc() {
                    Ok(result) => {
                        if let Some(ref metrics) = self.inner.metrics {
                            metrics.record_alloc_success();
                        }
                        // Log if we waited a long time
                        if wait_count > 10 {
                            let waited = wait_start.elapsed();
                            tracing::info!(
                                transport = ?self.inner.name,
                                waited_ms = waited.as_millis() as u64,
                                wait_count,
                                "Slot allocation succeeded after waiting"
                            );
                        }
                        break result;
                    }
                    Err(SlotError::NoFreeSlots) => {
                        wait_count += 1;
                        if let Some(ref metrics) = self.inner.metrics {
                            metrics.record_alloc_failure();
                        }

                        // Check if peer is still alive
                        if !self.inner.session.is_peer_alive(SLOT_PEER_TIMEOUT_NANOS) {
                            tracing::warn!(transport = ?self.inner.name, "SHM peer died while waiting for slot");
                            return Err(TransportError::Closed);
                        }

                        if self.is_closed() {
                            return Err(TransportError::Closed);
                        }

                        // Deadlock detection: warn periodically while waiting
                        let now = std::time::Instant::now();
                        if now.duration_since(last_warn) >= DEADLOCK_WARN_INTERVAL {
                            let slot_status = data_segment.slot_status();
                            let waited_ms = wait_start.elapsed().as_millis() as u64;

                            eprintln!(
                                "[DEADLOCK?] transport={:?} waited={}ms retries={} payload={}B {}",
                                self.inner.name, waited_ms, wait_count, payload_len, slot_status
                            );

                            tracing::warn!(
                                transport = ?self.inner.name,
                                waited_ms,
                                wait_count,
                                payload_len,
                                %slot_status,
                                "Potential deadlock: waiting for slot allocation"
                            );
                            last_warn = now;
                        }

                        // Wait for a slot to become available.
                        let futex = data_segment.slot_available_futex();
                        let current = futex.load(Ordering::Acquire);

                        // SAFETY: The futex lives in shared memory owned by the ShmSession,
                        // which outlives this blocking call. We cast to usize to satisfy
                        // spawn_blocking's 'static requirement, then reconstruct the reference.
                        let futex_ptr = futex as *const _ as usize;
                        let futex_wait = tokio::task::spawn_blocking(move || {
                            let futex =
                                unsafe { &*(futex_ptr as *const std::sync::atomic::AtomicU32) };
                            super::futex::futex_wait(futex, current, Some(SLOT_FUTEX_TIMEOUT))
                        });

                        use futures::FutureExt;
                        let notified = self.inner.slot_freed_notify.notified();
                        futures::pin_mut!(notified);
                        futures::pin_mut!(futex_wait);
                        let mut notified = notified.fuse();
                        let mut futex_wait = futex_wait.fuse();
                        futures::select_biased! {
                            _ = notified => {
                                // Slot freed in-process, retry immediately
                            }
                            _ = futex_wait => {
                                // Futex wait completed (timeout or signal)
                            }
                        }

                        self.inner.session.update_heartbeat();
                    }
                    Err(e) => {
                        tracing::debug!(
                            transport = ?self.inner.name,
                            error = %e,
                            slot_count = data_segment.slot_count(),
                            slot_size = data_segment.slot_size(),
                            payload_len,
                            "SHM slot allocation failed"
                        );
                        return Err(slot_error_to_transport(e, "alloc"));
                    }
                }
            };

            // SAFETY: We just allocated this slot via `data_segment.alloc()` above,
            // giving us exclusive write access. The slot index and generation are valid,
            // and the payload size was validated during allocation.
            unsafe {
                data_segment
                    .copy_to_slot(slot_idx, payload_bytes)
                    .map_err(|e| slot_error_to_transport(e, "copy_to_slot"))?;
            }

            if let Some(ref metrics) = self.inner.metrics {
                metrics.record_slot_copy(payload_len);
                metrics.record_slot_send();
            }

            // Mark in-flight.
            data_segment
                .mark_in_flight(slot_idx, generation)
                .map_err(|e| slot_error_to_transport(e, "mark_in_flight"))?;

            desc.payload_slot = slot_idx;
            desc.payload_generation = generation;
            desc.payload_offset = 0;
            desc.payload_len = payload_len as u32;
        }

        // Enqueue the descriptor, waiting if ring is full.
        let mut local_head = self.inner.session.local_send_head().load(Ordering::Relaxed);

        const FUTEX_TIMEOUT: Duration = Duration::from_millis(100);
        const PEER_TIMEOUT_NANOS: u64 = 3_000_000_000;
        const RING_DEADLOCK_WARN_INTERVAL: Duration = Duration::from_secs(1);

        let ring_wait_start = std::time::Instant::now();
        let mut ring_last_warn = ring_wait_start;
        let mut ring_wait_count = 0u32;

        loop {
            match send_ring.enqueue(&mut local_head, &desc) {
                Ok(()) => {
                    if let Some(ref metrics) = self.inner.metrics {
                        metrics.record_ring_enqueue();
                    }
                    if ring_wait_count > 10 {
                        let waited = ring_wait_start.elapsed();
                        tracing::info!(
                            transport = ?self.inner.name,
                            waited_ms = waited.as_millis() as u64,
                            ring_wait_count,
                            "Ring enqueue succeeded after waiting"
                        );
                    }
                    break;
                }
                Err(RingError::Full) => {
                    ring_wait_count += 1;
                    if let Some(ref metrics) = self.inner.metrics {
                        metrics.record_ring_full();
                    }

                    if !self.inner.session.is_peer_alive(PEER_TIMEOUT_NANOS) {
                        tracing::warn!(transport = ?self.inner.name, "SHM peer died while waiting for ring space");
                        return Err(TransportError::Closed);
                    }

                    if self.is_closed() {
                        return Err(TransportError::Closed);
                    }

                    let now = std::time::Instant::now();
                    if now.duration_since(ring_last_warn) >= RING_DEADLOCK_WARN_INTERVAL {
                        let waited_ms = ring_wait_start.elapsed().as_millis() as u64;
                        eprintln!(
                            "[DEADLOCK?] transport={:?} ring full waited={}ms retries={} capacity={}",
                            self.inner.name,
                            waited_ms,
                            ring_wait_count,
                            send_ring.capacity()
                        );
                        tracing::warn!(
                            transport = ?self.inner.name,
                            waited_ms,
                            ring_wait_count,
                            ring_capacity = send_ring.capacity(),
                            "Potential deadlock: waiting for ring space"
                        );
                        ring_last_warn = now;
                    }

                    let futex = self.inner.session.send_space_futex();
                    let current = futex.load(Ordering::Acquire);

                    // SAFETY: The futex lives in shared memory owned by the ShmSession,
                    // which outlives this blocking call. We cast to usize to satisfy
                    // spawn_blocking's 'static requirement, then reconstruct the reference.
                    let futex_ptr = futex as *const _ as usize;
                    let _ = tokio::task::spawn_blocking(move || {
                        let futex = unsafe { &*(futex_ptr as *const std::sync::atomic::AtomicU32) };
                        super::futex::futex_wait(futex, current, Some(FUTEX_TIMEOUT))
                    })
                    .await;

                    self.inner.session.update_heartbeat();
                }
            }
        }

        self.inner
            .session
            .local_send_head()
            .store(local_head, Ordering::Release);

        futex_signal(self.inner.session.send_data_futex());

        Ok(())
    }

    async fn recv_frame(&self) -> Result<Frame, TransportError> {
        if self.is_closed() {
            return Err(TransportError::Closed);
        }

        let recv_ring = self.inner.session.recv_ring();
        let data_segment = self.inner.session.data_segment();

        self.inner.session.update_heartbeat();

        const PEER_TIMEOUT_NANOS: u64 = 3_000_000_000;
        let mut last_heartbeat_update = std::time::Instant::now();
        const HEARTBEAT_INTERVAL: std::time::Duration = std::time::Duration::from_millis(500);
        const FUTEX_TIMEOUT: Duration = Duration::from_millis(100);

        loop {
            if let Some(desc) = recv_ring.dequeue() {
                if let Some(ref metrics) = self.inner.metrics {
                    metrics.record_ring_dequeue();
                }

                futex_signal(self.inner.session.recv_space_futex());

                if desc.is_inline() {
                    return Ok(Frame {
                        desc,
                        payload: Payload::Inline,
                    });
                } else {
                    // SAFETY: The descriptor was dequeued from the ring, meaning the peer
                    // has written valid data to this slot. The slot index, offset, and length
                    // in the descriptor were set by the peer during send_frame.
                    let payload_data = unsafe {
                        data_segment
                            .read_slot(
                                desc.payload_slot,
                                desc.payload_generation,
                                desc.payload_offset,
                                desc.payload_len,
                            )
                            .map_err(|e| slot_error_to_transport(e, "read_slot"))?
                    };
                    let _ = payload_data;

                    let guard = crate::transport::shm::SlotGuard::new(
                        self.inner.session.clone(),
                        desc.payload_slot,
                        desc.payload_generation,
                        desc.payload_offset,
                        desc.payload_len,
                        Some(self.inner.slot_freed_notify.clone()),
                        self.inner.metrics.clone(),
                    );

                    return Ok(Frame {
                        desc,
                        payload: Payload::Shm(guard),
                    });
                }
            }

            if !self.inner.session.is_peer_alive(PEER_TIMEOUT_NANOS) {
                tracing::warn!(transport = ?self.inner.name, "SHM peer appears to have died (no heartbeat for 3s)");
                return Err(TransportError::Closed);
            }

            if self.is_closed() {
                return Err(TransportError::Closed);
            }

            let now = std::time::Instant::now();
            if now.duration_since(last_heartbeat_update) >= HEARTBEAT_INTERVAL {
                self.inner.session.update_heartbeat();
                last_heartbeat_update = now;
            }

            let futex = self.inner.session.recv_data_futex();
            let current = futex.load(Ordering::Acquire);

            // SAFETY: The futex lives in shared memory owned by the ShmSession,
            // which outlives this blocking call. We cast to usize to satisfy
            // spawn_blocking's 'static requirement, then reconstruct the reference.
            let futex_ptr = futex as *const _ as usize;
            let _ = tokio::task::spawn_blocking(move || {
                let futex = unsafe { &*(futex_ptr as *const std::sync::atomic::AtomicU32) };
                super::futex::futex_wait(futex, current, Some(FUTEX_TIMEOUT))
            })
            .await;
        }
    }

    fn close(&self) {
        self.inner.closed.store(true, Ordering::Release);
    }

    fn is_closed(&self) -> bool {
        self.inner.closed.load(Ordering::Acquire)
    }

    fn buffer_pool(&self) -> &crate::BufferPool {
        &self.inner.buffer_pool
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::FrameFlags;
    use crate::MsgDescHot;

    #[tokio_test_lite::test]
    async fn test_pair_creation() {
        let (a, b) = ShmTransport::pair().unwrap();
        assert!(!a.is_closed());
        assert!(!b.is_closed());
    }

    #[tokio_test_lite::test]
    async fn test_send_recv_inline() {
        let (a, b) = ShmTransport::pair().unwrap();

        // Create a frame with inline payload.
        let mut desc = MsgDescHot::new();
        desc.msg_id = 1;
        desc.channel_id = 1;
        desc.method_id = 42;
        desc.flags = FrameFlags::DATA;

        let frame = Frame::with_inline_payload(desc, b"hello").unwrap();

        // Send from A.
        a.send_frame(frame).await.unwrap();

        // Receive on B.
        let recv = b.recv_frame().await.unwrap();
        assert_eq!(recv.desc.msg_id, 1);
        assert_eq!(recv.desc.channel_id, 1);
        assert_eq!(recv.desc.method_id, 42);
        assert_eq!(recv.payload_bytes(), b"hello");
    }

    #[tokio_test_lite::test]
    async fn test_send_recv_external_payload() {
        let (a, b) = ShmTransport::pair().unwrap();

        let mut desc = MsgDescHot::new();
        desc.msg_id = 2;
        desc.flags = FrameFlags::DATA;

        let payload = vec![0u8; 1000]; // Larger than inline.
        let frame = Frame::with_payload(desc, payload.clone());

        a.send_frame(frame).await.unwrap();

        let recv = b.recv_frame().await.unwrap();
        assert_eq!(recv.desc.msg_id, 2);
        assert_eq!(recv.payload_bytes().len(), 1000);
    }

    #[tokio_test_lite::test]
    async fn test_bidirectional() {
        let (a, b) = ShmTransport::pair().unwrap();

        // A -> B.
        let mut desc_a = MsgDescHot::new();
        desc_a.msg_id = 1;
        let frame_a = Frame::with_inline_payload(desc_a, b"from A").unwrap();
        a.send_frame(frame_a).await.unwrap();

        // B -> A.
        let mut desc_b = MsgDescHot::new();
        desc_b.msg_id = 2;
        let frame_b = Frame::with_inline_payload(desc_b, b"from B").unwrap();
        b.send_frame(frame_b).await.unwrap();

        // Receive both.
        let recv_b = b.recv_frame().await.unwrap();
        assert_eq!(recv_b.payload_bytes(), b"from A");

        let recv_a = a.recv_frame().await.unwrap();
        assert_eq!(recv_a.payload_bytes(), b"from B");
    }

    #[tokio_test_lite::test]
    async fn test_close() {
        let (a, _b) = ShmTransport::pair().unwrap();

        a.close();
        assert!(a.is_closed());

        // Sending on closed transport should fail.
        let frame = Frame::new(MsgDescHot::new());
        assert!(matches!(
            a.send_frame(frame).await,
            Err(TransportError::Closed)
        ));
    }
}
