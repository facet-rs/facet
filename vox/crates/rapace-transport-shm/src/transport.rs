//! SHM transport implementation.

use std::sync::atomic::Ordering;
use std::sync::Arc;

use parking_lot::Mutex;
use rapace_core::{
    DecodeError, EncodeCtx, EncodeError, Frame, FrameView, MsgDescHot, Transport, TransportError,
    ValidationError, INLINE_PAYLOAD_SIZE, INLINE_PAYLOAD_SLOT,
};

use crate::layout::{RingError, SlotError};
use crate::session::ShmSession;

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
        SlotError::PayloadTooLarge => {
            TransportError::Validation(ValidationError::PayloadTooLarge { len: 0, max: 0 })
        }
    }
}

fn ring_error_to_transport(e: RingError) -> TransportError {
    match e {
        RingError::Full => TransportError::Encode(EncodeError::EncodeFailed("ring full".into())),
    }
}

/// SHM transport implementation.
///
/// This transport uses shared memory rings and slots to move frames
/// between two peers with zero-copy when possible.
pub struct ShmTransport {
    /// The underlying SHM session.
    session: Arc<ShmSession>,
    /// Most recently received frame (for FrameView lifetime).
    last_frame: Mutex<Option<ReceivedFrame>>,
    /// Whether the transport is closed.
    closed: std::sync::atomic::AtomicBool,
    /// Optional metrics for tracking zero-copy performance.
    metrics: Option<Arc<ShmMetrics>>,
}

/// A frame received from SHM, with its slot info for later freeing.
struct ReceivedFrame {
    desc: MsgDescHot,
    /// If payload was in a slot, this holds the slot info for freeing.
    slot_info: Option<(u32, u32)>, // (slot_index, generation)
    /// Payload data (either copied from inline or referencing slot).
    payload: Vec<u8>,
}

impl ShmTransport {
    /// Create a new SHM transport from a session.
    pub fn new(session: Arc<ShmSession>) -> Self {
        Self {
            session,
            last_frame: Mutex::new(None),
            closed: std::sync::atomic::AtomicBool::new(false),
            metrics: None,
        }
    }

    /// Create a new SHM transport with metrics enabled.
    pub fn new_with_metrics(session: Arc<ShmSession>, metrics: Arc<ShmMetrics>) -> Self {
        Self {
            session,
            last_frame: Mutex::new(None),
            closed: std::sync::atomic::AtomicBool::new(false),
            metrics: Some(metrics),
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
    pub fn pair_with_metrics(
        metrics: Arc<ShmMetrics>,
    ) -> Result<(Self, Self), TransportError> {
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
        self.closed.load(Ordering::Acquire)
    }

    /// Get the underlying session.
    #[inline]
    pub fn session(&self) -> &Arc<ShmSession> {
        &self.session
    }

    /// Get the metrics instance, if enabled.
    #[inline]
    pub fn metrics(&self) -> Option<&Arc<ShmMetrics>> {
        self.metrics.as_ref()
    }
}

impl Transport for ShmTransport {
    async fn send_frame(&self, frame: &Frame) -> Result<(), TransportError> {
        if self.is_closed() {
            return Err(TransportError::Closed);
        }

        let send_ring = self.session.send_ring();
        let data_segment = self.session.data_segment();

        // Prepare the descriptor.
        let mut desc = frame.desc;
        let payload = frame.payload();

        if payload.len() <= INLINE_PAYLOAD_SIZE {
            // Inline payload.
            desc.payload_slot = INLINE_PAYLOAD_SLOT;
            desc.payload_generation = 0;
            desc.payload_offset = 0;
            desc.payload_len = payload.len() as u32;
            desc.inline_payload[..payload.len()].copy_from_slice(payload);

            if let Some(ref metrics) = self.metrics {
                metrics.record_inline_send();
            }
        } else {
            // Need to allocate a slot.
            let (slot_idx, gen) = match data_segment.alloc() {
                Ok(result) => {
                    if let Some(ref metrics) = self.metrics {
                        metrics.record_alloc_success();
                    }
                    result
                }
                Err(e) => {
                    if let Some(ref metrics) = self.metrics {
                        metrics.record_alloc_failure();
                    }
                    return Err(slot_error_to_transport(e, "alloc"));
                }
            };

            // Copy payload into slot.
            unsafe {
                data_segment
                    .copy_to_slot(slot_idx, payload)
                    .map_err(|e| slot_error_to_transport(e, "copy_to_slot"))?;
            }

            if let Some(ref metrics) = self.metrics {
                metrics.record_slot_copy(payload.len());
                metrics.record_slot_send();
            }

            // Mark in-flight.
            data_segment
                .mark_in_flight(slot_idx, gen)
                .map_err(|e| slot_error_to_transport(e, "mark_in_flight"))?;

            desc.payload_slot = slot_idx;
            desc.payload_generation = gen;
            desc.payload_offset = 0;
            desc.payload_len = payload.len() as u32;
        }

        // Enqueue the descriptor.
        let mut local_head = self.session.local_send_head().load(Ordering::Relaxed);
        match send_ring.enqueue(&mut local_head, &desc) {
            Ok(()) => {
                if let Some(ref metrics) = self.metrics {
                    metrics.record_ring_enqueue();
                }
            }
            Err(e) => {
                if let Some(ref metrics) = self.metrics {
                    metrics.record_ring_full();
                }
                return Err(ring_error_to_transport(e));
            }
        }
        self.session
            .local_send_head()
            .store(local_head, Ordering::Release);

        // TODO: Signal peer via eventfd doorbell.

        Ok(())
    }

    async fn recv_frame(&self) -> Result<FrameView<'_>, TransportError> {
        if self.is_closed() {
            return Err(TransportError::Closed);
        }

        let recv_ring = self.session.recv_ring();
        let data_segment = self.session.data_segment();

        // Free any previous frame's slot.
        {
            let mut last = self.last_frame.lock();
            if let Some(prev) = last.take() {
                if let Some((slot_idx, gen)) = prev.slot_info {
                    // Ignore errors on free (slot may have been freed already).
                    if data_segment.free(slot_idx, gen).is_ok() {
                        if let Some(ref metrics) = self.metrics {
                            metrics.record_slot_free();
                        }
                    }
                }
            }
        }

        // Poll for a descriptor.
        // TODO: Use eventfd for proper async notification instead of polling.
        loop {
            if let Some(desc) = recv_ring.dequeue() {
                if let Some(ref metrics) = self.metrics {
                    metrics.record_ring_dequeue();
                }

                // Got a descriptor. Extract payload.
                let (payload, slot_info) = if desc.is_inline() {
                    // Inline payload - copy from descriptor.
                    let payload = desc.inline_payload[..desc.payload_len as usize].to_vec();
                    (payload, None)
                } else {
                    // Payload in slot - read it.
                    let payload_data = unsafe {
                        data_segment
                            .read_slot(desc.payload_slot, desc.payload_offset, desc.payload_len)
                            .map_err(|e| slot_error_to_transport(e, "read_slot"))?
                    };
                    (
                        payload_data.to_vec(),
                        Some((desc.payload_slot, desc.payload_generation)),
                    )
                };

                // Store for FrameView lifetime.
                let received = ReceivedFrame {
                    desc,
                    slot_info,
                    payload,
                };

                {
                    let mut last = self.last_frame.lock();
                    *last = Some(received);
                }

                // Build FrameView with lifetime tied to &self.
                let last = self.last_frame.lock();
                let frame_ref = last.as_ref().unwrap();

                // SAFETY: Extending lifetime is safe because:
                // - Data lives in self.last_frame which lives as long as self.
                // - FrameView borrows &self, preventing concurrent recv_frame.
                let desc_ptr = &frame_ref.desc as *const MsgDescHot;
                let payload_ptr = frame_ref.payload.as_ptr();
                let payload_len = frame_ref.payload.len();

                let desc: &MsgDescHot = unsafe { &*desc_ptr };
                let payload: &[u8] =
                    unsafe { std::slice::from_raw_parts(payload_ptr, payload_len) };

                return Ok(FrameView::new(desc, payload));
            }

            // No descriptor available. Yield and try again.
            // TODO: Wait on eventfd instead of polling.
            tokio::task::yield_now().await;

            if self.is_closed() {
                return Err(TransportError::Closed);
            }
        }
    }

    fn encoder(&self) -> Box<dyn EncodeCtx + '_> {
        match &self.metrics {
            Some(metrics) => Box::new(ShmEncoder::new_with_metrics(
                self.session.clone(),
                metrics.clone(),
            )),
            None => Box::new(ShmEncoder::new(self.session.clone())),
        }
    }

    async fn close(&self) -> Result<(), TransportError> {
        self.closed.store(true, Ordering::Release);

        // Free any held slot.
        let mut last = self.last_frame.lock();
        if let Some(prev) = last.take() {
            if let Some((slot_idx, gen)) = prev.slot_info {
                let data_segment = self.session.data_segment();
                let _ = data_segment.free(slot_idx, gen);
            }
        }

        Ok(())
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
        self.copy_encodes
            .load(std::sync::atomic::Ordering::Relaxed)
    }

    /// Get the zero-copy efficiency as a percentage (0.0 to 1.0).
    pub fn zero_copy_ratio(&self) -> f64 {
        let zero = self.zero_copy_count() as f64;
        let copy = self.copy_count() as f64;
        let total = zero + copy;
        if total == 0.0 {
            0.0
        } else {
            zero / total
        }
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

// ============================================================================
// Encoder
// ============================================================================

/// Encoder for SHM transport.
///
/// Can detect if bytes are already in the SHM segment and reference them
/// zero-copy, otherwise copies to a new slot.
pub struct ShmEncoder {
    session: Arc<ShmSession>,
    desc: MsgDescHot,
    payload: Vec<u8>,
    metrics: Option<Arc<ShmMetrics>>,
}

impl ShmEncoder {
    fn new(session: Arc<ShmSession>) -> Self {
        Self {
            session,
            desc: MsgDescHot::new(),
            payload: Vec::new(),
            metrics: None,
        }
    }

    fn new_with_metrics(session: Arc<ShmSession>, metrics: Arc<ShmMetrics>) -> Self {
        Self {
            session,
            desc: MsgDescHot::new(),
            payload: Vec::new(),
            metrics: Some(metrics),
        }
    }
}

impl EncodeCtx for ShmEncoder {
    fn encode_bytes(&mut self, bytes: &[u8]) -> Result<(), EncodeError> {
        // Check if bytes are already in our SHM segment's slot data region.
        if let Some((slot_idx, offset)) =
            self.session.find_slot_location(bytes.as_ptr(), bytes.len())
        {
            // Zero-copy: just record the slot reference.
            // Note: This assumes the caller has ownership of the slot.
            // In practice, this would need more careful lifetime management.
            self.desc.payload_slot = slot_idx;
            self.desc.payload_offset = offset;
            self.desc.payload_len = bytes.len() as u32;

            // Record metrics if available.
            if let Some(ref metrics) = self.metrics {
                metrics.record_zero_copy(bytes.len());
            }

            // Don't set payload - it's in SHM already.
            return Ok(());
        }

        // Not in SHM - accumulate in payload buffer.
        if let Some(ref metrics) = self.metrics {
            metrics.record_copy(bytes.len());
        }
        self.payload.extend_from_slice(bytes);
        Ok(())
    }

    fn finish(mut self: Box<Self>) -> Result<Frame, EncodeError> {
        // If we already have a slot reference (zero-copy case), use it.
        if self.desc.payload_slot != INLINE_PAYLOAD_SLOT && self.payload.is_empty() {
            // Already referencing SHM data.
            return Ok(Frame::new(self.desc));
        }

        // Otherwise, create a frame with the accumulated payload.
        if self.payload.len() <= INLINE_PAYLOAD_SIZE {
            // Fits inline.
            self.desc.payload_slot = INLINE_PAYLOAD_SLOT;
            self.desc.payload_generation = 0;
            self.desc.payload_offset = 0;
            self.desc.payload_len = self.payload.len() as u32;
            self.desc.inline_payload[..self.payload.len()].copy_from_slice(&self.payload);
            Ok(Frame::new(self.desc))
        } else {
            // Need a slot - but encoder doesn't allocate slots.
            // Return a frame with external payload; transport will allocate slot on send.
            Ok(Frame::with_payload(self.desc, self.payload))
        }
    }
}

/// Decoder for SHM transport.
#[allow(dead_code)]
pub struct ShmDecoder<'a> {
    data: &'a [u8],
    pos: usize,
}

#[allow(dead_code)]
impl<'a> ShmDecoder<'a> {
    /// Create a new decoder from a byte slice.
    pub fn new(data: &'a [u8]) -> Self {
        Self { data, pos: 0 }
    }
}

impl<'a> rapace_core::DecodeCtx<'a> for ShmDecoder<'a> {
    fn decode_bytes(&mut self) -> Result<&'a [u8], DecodeError> {
        let result = &self.data[self.pos..];
        self.pos = self.data.len();
        Ok(result)
    }

    fn remaining(&self) -> &'a [u8] {
        &self.data[self.pos..]
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rapace_core::FrameFlags;

    #[tokio::test]
    async fn test_pair_creation() {
        let (a, b) = ShmTransport::pair().unwrap();
        assert!(!a.is_closed());
        assert!(!b.is_closed());
    }

    #[tokio::test]
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
        a.send_frame(&frame).await.unwrap();

        // Receive on B.
        let view = b.recv_frame().await.unwrap();
        assert_eq!(view.desc.msg_id, 1);
        assert_eq!(view.desc.channel_id, 1);
        assert_eq!(view.desc.method_id, 42);
        assert_eq!(view.payload, b"hello");
    }

    #[tokio::test]
    async fn test_send_recv_external_payload() {
        let (a, b) = ShmTransport::pair().unwrap();

        let mut desc = MsgDescHot::new();
        desc.msg_id = 2;
        desc.flags = FrameFlags::DATA;

        let payload = vec![0u8; 1000]; // Larger than inline.
        let frame = Frame::with_payload(desc, payload.clone());

        a.send_frame(&frame).await.unwrap();

        let view = b.recv_frame().await.unwrap();
        assert_eq!(view.desc.msg_id, 2);
        assert_eq!(view.payload.len(), 1000);
    }

    #[tokio::test]
    async fn test_bidirectional() {
        let (a, b) = ShmTransport::pair().unwrap();

        // A -> B.
        let mut desc_a = MsgDescHot::new();
        desc_a.msg_id = 1;
        let frame_a = Frame::with_inline_payload(desc_a, b"from A").unwrap();
        a.send_frame(&frame_a).await.unwrap();

        // B -> A.
        let mut desc_b = MsgDescHot::new();
        desc_b.msg_id = 2;
        let frame_b = Frame::with_inline_payload(desc_b, b"from B").unwrap();
        b.send_frame(&frame_b).await.unwrap();

        // Receive both.
        let view_b = b.recv_frame().await.unwrap();
        assert_eq!(view_b.payload, b"from A");

        let view_a = a.recv_frame().await.unwrap();
        assert_eq!(view_a.payload, b"from B");
    }

    #[tokio::test]
    async fn test_close() {
        let (a, _b) = ShmTransport::pair().unwrap();

        a.close().await.unwrap();
        assert!(a.is_closed());

        // Sending on closed transport should fail.
        let frame = Frame::new(MsgDescHot::new());
        assert!(matches!(
            a.send_frame(&frame).await,
            Err(TransportError::Closed)
        ));
    }

    #[tokio::test]
    async fn test_encoder() {
        let (a, _b) = ShmTransport::pair().unwrap();

        let mut encoder = a.encoder();
        encoder.encode_bytes(b"test data").unwrap();
        let frame = encoder.finish().unwrap();

        assert_eq!(frame.payload(), b"test data");
    }

    #[tokio::test]
    async fn test_metrics_with_copy() {
        let metrics = Arc::new(ShmMetrics::new());
        let (a, _b) = ShmTransport::pair_with_metrics(metrics.clone()).unwrap();

        // Encode regular heap data (will copy).
        let heap_data = vec![0u8; 100];
        let mut encoder = a.encoder();
        encoder.encode_bytes(&heap_data).unwrap();
        let _ = encoder.finish().unwrap();

        // Should have recorded a copy.
        assert_eq!(metrics.zero_copy_count(), 0);
        assert_eq!(metrics.copy_count(), 1);
    }

    #[tokio::test]
    async fn test_metrics_with_zero_copy() {
        use crate::ShmAllocator;
        use allocator_api2::vec::Vec as AllocVec;

        let metrics = Arc::new(ShmMetrics::new());

        // Create sessions manually so we can get the allocator.
        let (session_a, session_b) = ShmSession::create_pair().unwrap();
        let alloc = ShmAllocator::new(session_a.clone());

        let a = ShmTransport::new_with_metrics(session_a, metrics.clone());
        let _b = ShmTransport::new_with_metrics(session_b, metrics.clone());

        // Allocate data in SHM.
        let mut shm_data: AllocVec<u8, _> = AllocVec::new_in(alloc);
        shm_data.extend_from_slice(&[42u8; 100]);

        // Encode the SHM data (should be zero-copy).
        let mut encoder = a.encoder();
        encoder.encode_bytes(&shm_data).unwrap();
        let frame = encoder.finish().unwrap();

        // Should have recorded a zero-copy encode.
        assert_eq!(metrics.zero_copy_count(), 1, "Expected 1 zero-copy encode");
        assert_eq!(metrics.copy_count(), 0, "Expected 0 copy encodes");

        // The frame should reference the slot directly (no external payload).
        assert!(
            frame.payload.is_none(),
            "Zero-copy frame should not have external payload"
        );
    }

    #[tokio::test]
    async fn test_metrics_mixed() {
        use crate::ShmAllocator;
        use allocator_api2::vec::Vec as AllocVec;

        let metrics = Arc::new(ShmMetrics::new());
        let (session_a, session_b) = ShmSession::create_pair().unwrap();
        let alloc = ShmAllocator::new(session_a.clone());

        let a = ShmTransport::new_with_metrics(session_a, metrics.clone());
        let _b = ShmTransport::new_with_metrics(session_b, metrics.clone());

        // First: encode heap data (copy).
        let heap_data = vec![1u8; 50];
        let mut encoder = a.encoder();
        encoder.encode_bytes(&heap_data).unwrap();
        let _ = encoder.finish().unwrap();

        // Second: encode SHM data (zero-copy).
        let mut shm_data: AllocVec<u8, _> = AllocVec::new_in(alloc);
        shm_data.extend_from_slice(&[2u8; 50]);
        let mut encoder = a.encoder();
        encoder.encode_bytes(&shm_data).unwrap();
        let _ = encoder.finish().unwrap();

        // Check metrics.
        assert_eq!(metrics.zero_copy_count(), 1);
        assert_eq!(metrics.copy_count(), 1);
        assert!((metrics.zero_copy_ratio() - 0.5).abs() < 0.01);
    }
}

/// Conformance tests using rapace-testkit.
#[cfg(test)]
mod conformance_tests {
    use super::*;
    use rapace_testkit::{TestError, TransportFactory};

    struct ShmFactory;

    impl TransportFactory for ShmFactory {
        type Transport = ShmTransport;

        async fn connect_pair() -> Result<(Self::Transport, Self::Transport), TestError> {
            ShmTransport::pair().map_err(|e| TestError::Setup(format!("{}", e)))
        }
    }

    #[tokio::test]
    async fn unary_happy_path() {
        rapace_testkit::run_unary_happy_path::<ShmFactory>().await;
    }

    #[tokio::test]
    async fn unary_multiple_calls() {
        rapace_testkit::run_unary_multiple_calls::<ShmFactory>().await;
    }

    #[tokio::test]
    async fn ping_pong() {
        rapace_testkit::run_ping_pong::<ShmFactory>().await;
    }

    #[tokio::test]
    async fn deadline_success() {
        rapace_testkit::run_deadline_success::<ShmFactory>().await;
    }

    #[tokio::test]
    async fn deadline_exceeded() {
        rapace_testkit::run_deadline_exceeded::<ShmFactory>().await;
    }

    #[tokio::test]
    async fn cancellation() {
        rapace_testkit::run_cancellation::<ShmFactory>().await;
    }

    #[tokio::test]
    async fn credit_grant() {
        rapace_testkit::run_credit_grant::<ShmFactory>().await;
    }

    #[tokio::test]
    async fn error_response() {
        rapace_testkit::run_error_response::<ShmFactory>().await;
    }

    // Session-level tests (semantic enforcement)

    #[tokio::test]
    async fn session_credit_exhaustion() {
        rapace_testkit::run_session_credit_exhaustion::<ShmFactory>().await;
    }

    #[tokio::test]
    async fn session_cancelled_channel_drop() {
        rapace_testkit::run_session_cancelled_channel_drop::<ShmFactory>().await;
    }

    #[tokio::test]
    async fn session_cancel_control_frame() {
        rapace_testkit::run_session_cancel_control_frame::<ShmFactory>().await;
    }

    #[tokio::test]
    async fn session_grant_credits_control_frame() {
        rapace_testkit::run_session_grant_credits_control_frame::<ShmFactory>().await;
    }

    #[tokio::test]
    async fn session_deadline_check() {
        rapace_testkit::run_session_deadline_check::<ShmFactory>().await;
    }

    // Streaming tests

    #[tokio::test]
    async fn server_streaming_happy_path() {
        rapace_testkit::run_server_streaming_happy_path::<ShmFactory>().await;
    }

    #[tokio::test]
    async fn client_streaming_happy_path() {
        rapace_testkit::run_client_streaming_happy_path::<ShmFactory>().await;
    }

    #[tokio::test]
    async fn bidirectional_streaming() {
        rapace_testkit::run_bidirectional_streaming::<ShmFactory>().await;
    }

    #[tokio::test]
    async fn streaming_cancellation() {
        rapace_testkit::run_streaming_cancellation::<ShmFactory>().await;
    }

    // Macro-generated streaming tests

    #[tokio::test]
    async fn macro_server_streaming() {
        rapace_testkit::run_macro_server_streaming::<ShmFactory>().await;
    }

    // Large blob tests

    #[tokio::test]
    async fn large_blob_echo() {
        rapace_testkit::run_large_blob_echo::<ShmFactory>().await;
    }

    #[tokio::test]
    async fn large_blob_transform() {
        rapace_testkit::run_large_blob_transform::<ShmFactory>().await;
    }

    #[tokio::test]
    async fn large_blob_checksum() {
        rapace_testkit::run_large_blob_checksum::<ShmFactory>().await;
    }
}
