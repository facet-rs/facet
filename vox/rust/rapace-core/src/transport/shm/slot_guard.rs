//! Slot guard for zero-copy SHM payload access.
//!
//! Note: The hub transport currently copies data out immediately on receive,
//! so this SlotGuard is not actively used. It's kept for API compatibility
//! and potential future zero-copy optimizations.

use std::sync::Arc;

use super::hub_alloc::HubAllocator;
use super::hub_layout::HubSlotError;
use super::transport::ShmMetrics;

/// Guard for a shared-memory payload slot.
///
/// Keeps the underlying SHM mapping alive and frees the slot back to the
/// allocator on drop. This enables zero-copy access to payload data.
pub struct SlotGuard {
    /// The allocator that owns this slot.
    allocator: Arc<HubAllocator>,
    /// Size class of the slot.
    class: u16,
    /// Global index within the size class.
    global_index: u32,
    /// Generation for ABA protection.
    generation: u32,
    /// Pointer to the start of the payload data within the slot.
    data_ptr: *const u8,
    /// Length of the payload in bytes.
    len: u32,
    /// Optional metrics for tracking slot frees.
    metrics: Option<Arc<ShmMetrics>>,
}

// SAFETY: The data pointer points into a shared memory region that is kept alive
// by the HubPeer/HubHost that owns the allocator. The slot remains allocated
// (InFlight state) until this guard is dropped.
unsafe impl Send for SlotGuard {}
unsafe impl Sync for SlotGuard {}

impl std::fmt::Debug for SlotGuard {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("SlotGuard")
            .field("class", &self.class)
            .field("global_index", &self.global_index)
            .field("generation", &self.generation)
            .field("len", &self.len)
            .finish_non_exhaustive()
    }
}

impl SlotGuard {
    /// Create a new slot guard.
    ///
    /// # Safety
    ///
    /// - The slot must be in InFlight state.
    /// - `data_ptr` must point to valid memory within the slot.
    /// - `len` must not exceed the slot's data region.
    /// - The allocator must remain valid for the lifetime of this guard.
    #[allow(dead_code)]
    pub(crate) unsafe fn new(
        allocator: Arc<HubAllocator>,
        class: u16,
        global_index: u32,
        generation: u32,
        data_ptr: *const u8,
        len: u32,
        metrics: Option<Arc<ShmMetrics>>,
    ) -> Self {
        Self {
            allocator,
            class,
            global_index,
            generation,
            data_ptr,
            len,
            metrics,
        }
    }

    /// Get the length of the payload in bytes.
    #[inline]
    pub fn len(&self) -> usize {
        self.len as usize
    }

    /// Returns true if the payload is empty.
    #[inline]
    pub fn is_empty(&self) -> bool {
        self.len == 0
    }

    /// Borrow the payload as a byte slice.
    #[inline]
    fn as_slice(&self) -> &[u8] {
        // SAFETY: The slot is in InFlight state and won't be freed until we drop.
        // The data_ptr and len were validated at construction time.
        unsafe { std::slice::from_raw_parts(self.data_ptr, self.len as usize) }
    }

    /// Free the slot, returning any error that occurs.
    ///
    /// This is called automatically on drop, but can be called explicitly
    /// to handle errors.
    pub fn free(self) -> Result<(), HubSlotError> {
        let result = self
            .allocator
            .free(self.class, self.global_index, self.generation);
        if result.is_ok()
            && let Some(ref metrics) = self.metrics
        {
            metrics.record_slot_free();
        }
        // Prevent Drop from running (we already freed)
        std::mem::forget(self);
        result
    }
}

impl AsRef<[u8]> for SlotGuard {
    #[inline]
    fn as_ref(&self) -> &[u8] {
        self.as_slice()
    }
}

impl std::ops::Deref for SlotGuard {
    type Target = [u8];

    #[inline]
    fn deref(&self) -> &Self::Target {
        self.as_slice()
    }
}

impl Drop for SlotGuard {
    fn drop(&mut self) {
        if let Err(e) = self
            .allocator
            .free(self.class, self.global_index, self.generation)
        {
            tracing::warn!(
                class = self.class,
                global_index = self.global_index,
                generation = self.generation,
                error = %e,
                "SlotGuard: failed to free slot on drop"
            );
        } else if let Some(ref metrics) = self.metrics {
            metrics.record_slot_free();
        }
    }
}
