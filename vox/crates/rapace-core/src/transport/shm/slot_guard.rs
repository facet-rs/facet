use std::sync::Arc;

use tokio::sync::Notify;

use super::layout::{DataSegment, SlotError};
use super::session::ShmSession;
use super::transport::ShmMetrics;

/// Guard for a shared-memory payload slot.
///
/// Keeps the underlying SHM mapping alive and frees the slot back to the free
/// list on drop.
pub struct SlotGuard {
    #[allow(dead_code)]
    session: Arc<ShmSession>,
    data_segment: DataSegment,
    slot: u32,
    generation: u32,
    offset: u32,
    len: u32,
    slot_freed_notify: Option<Arc<Notify>>,
    metrics: Option<Arc<ShmMetrics>>,
}

impl std::fmt::Debug for SlotGuard {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("SlotGuard")
            .field("slot", &self.slot)
            .field("generation", &self.generation)
            .field("offset", &self.offset)
            .field("len", &self.len)
            .finish_non_exhaustive()
    }
}

impl SlotGuard {
    pub(crate) fn new(
        session: Arc<ShmSession>,
        slot: u32,
        generation: u32,
        offset: u32,
        len: u32,
        slot_freed_notify: Option<Arc<Notify>>,
        metrics: Option<Arc<ShmMetrics>>,
    ) -> Self {
        let data_segment = session.data_segment();
        Self {
            session,
            data_segment,
            slot,
            generation,
            offset,
            len,
            slot_freed_notify,
            metrics,
        }
    }

    fn read_slice(&self) -> Result<&[u8], SlotError> {
        // SAFETY: The slot was dequeued from the recv ring and remains InFlight
        // until this guard is dropped, at which point we free it. The session's
        // SHM mapping outlives this borrow via Arc.
        unsafe {
            self.data_segment
                .read_slot(self.slot, self.generation, self.offset, self.len)
        }
    }
}

impl AsRef<[u8]> for SlotGuard {
    fn as_ref(&self) -> &[u8] {
        self.read_slice()
            .expect("SHM SlotGuard slice must be valid")
    }
}

impl Drop for SlotGuard {
    fn drop(&mut self) {
        if self.data_segment.free(self.slot, self.generation).is_ok() {
            if let Some(metrics) = self.metrics.as_ref() {
                metrics.record_slot_free();
            }
            if let Some(notify) = self.slot_freed_notify.as_ref() {
                notify.notify_waiters();
            }
        }
    }
}
