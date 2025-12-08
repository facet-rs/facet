// src/alloc.rs

use std::sync::atomic::{AtomicU64, Ordering};
use std::ptr::NonNull;
use crate::types::{SlotIndex, Generation, ByteLen};
use crate::layout::{SlotMeta, SlotState};

/// Error when allocation fails
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AllocError;

/// Data segment for payload allocation.
pub struct DataSegment {
    /// Pointer to slot metadata array
    meta_base: NonNull<SlotMeta>,
    /// Pointer to slot data array
    data_base: NonNull<u8>,
    /// Size of each slot in bytes
    slot_size: u32,
    /// Number of slots
    slot_count: u32,
    /// Free list head: low 32 bits = index, high 32 bits = generation (for ABA safety)
    free_head: AtomicU64,
}

// Safety: DataSegment uses atomics for all shared state
unsafe impl Send for DataSegment {}
unsafe impl Sync for DataSegment {}

/// A slot allocated for outbound data. Owns the slot until committed or dropped.
///
/// If dropped without committing, the slot is freed automatically.
pub struct OutboundSlot<'seg> {
    seg: &'seg DataSegment,
    slot: SlotIndex,
    gen: Generation,
    committed: bool,
}

/// A committed slot ready to be referenced in a descriptor.
///
/// Does NOT free on drop—ownership transfers to receiver via the ring.
#[derive(Debug, Clone, Copy)]
pub struct CommittedSlot {
    slot: SlotIndex,
    gen: Generation,
    len: u32,
}

/// An inbound payload received from peer. Frees the slot on drop.
pub struct InboundPayload<'seg> {
    seg: &'seg DataSegment,
    slot: Option<(SlotIndex, Generation)>, // None for inline payloads
}

impl DataSegment {
    /// Create a new DataSegment from raw pointers.
    ///
    /// # Safety
    /// - `meta_base` must point to valid SlotMeta array of length `slot_count`
    /// - `data_base` must point to valid memory of size `slot_size * slot_count`
    /// - Memory must outlive the DataSegment
    /// - Caller must ensure exclusive access during initialization
    pub unsafe fn from_raw(
        meta_base: NonNull<SlotMeta>,
        data_base: NonNull<u8>,
        slot_size: u32,
        slot_count: u32,
    ) -> Self {
        DataSegment {
            meta_base,
            data_base,
            slot_size,
            slot_count,
            free_head: AtomicU64::new(0), // Will be initialized by init_free_list
        }
    }

    /// Initialize the free list. Call once after creating the segment.
    ///
    /// # Safety
    /// Must be called exactly once, before any allocations.
    pub unsafe fn init_free_list(&self) {
        // Initialize all slots as free, chaining them together
        for i in 0..self.slot_count {
            let meta = self.slot_meta_mut(SlotIndex::new(i));
            (*meta).generation.store(0, Ordering::Relaxed);
            (*meta).state.store(SlotState::Free as u32, Ordering::Relaxed);
        }

        // Set free head to slot 0 (all slots available)
        // Encode: low 32 = index, high 32 = generation
        self.free_head.store(0, Ordering::Release);
    }

    /// Allocate a slot for outbound payload.
    pub fn alloc(&self) -> Result<OutboundSlot<'_>, AllocError> {
        // Try to find a free slot, with a reasonable limit to prevent infinite loops
        for _ in 0..self.slot_count * 2 {
            let head = self.free_head.load(Ordering::Acquire);
            let idx = (head & 0xFFFF_FFFF) as u32;

            if idx >= self.slot_count {
                return Err(AllocError);
            }

            let meta = unsafe { &*self.slot_meta(SlotIndex::new(idx)) };
            let state = meta.state.load(Ordering::Acquire);

            if state != SlotState::Free as u32 {
                // Slot not actually free, try to update head to next free slot
                if let Some(next_idx) = self.find_next_free(idx + 1) {
                    let new_head = ((head >> 32).wrapping_add(1) << 32) | (next_idx as u64);
                    // Try to update head, but don't worry if it fails - just retry
                    let _ = self.free_head.compare_exchange_weak(
                        head,
                        new_head,
                        Ordering::AcqRel,
                        Ordering::Relaxed,
                    );
                    continue;
                } else {
                    // No free slots available
                    return Err(AllocError);
                }
            }

            // Try to claim this slot
            let new_gen = meta.generation.load(Ordering::Acquire).wrapping_add(1);

            // Calculate next free slot (simple linear scan for now)
            let next_idx = self.find_next_free(idx + 1).unwrap_or(self.slot_count);
            let new_head = ((head >> 32).wrapping_add(1) << 32) | (next_idx as u64);

            if self.free_head.compare_exchange_weak(
                head,
                new_head,
                Ordering::AcqRel,
                Ordering::Relaxed,
            ).is_ok() {
                // Successfully claimed the slot
                meta.generation.store(new_gen, Ordering::Release);
                meta.state.store(SlotState::Allocated as u32, Ordering::Release);

                return Ok(OutboundSlot {
                    seg: self,
                    slot: SlotIndex::new(idx),
                    gen: Generation::new(new_gen),
                    committed: false,
                });
            }
            // CAS failed, retry
        }

        // Exceeded retry limit
        Err(AllocError)
    }

    /// Find the next free slot starting from `start`
    fn find_next_free(&self, start: u32) -> Option<u32> {
        for i in start..self.slot_count {
            let meta = unsafe { &*self.slot_meta(SlotIndex::new(i)) };
            if meta.state.load(Ordering::Relaxed) == SlotState::Free as u32 {
                return Some(i);
            }
        }
        // Wrap around
        for i in 0..start.min(self.slot_count) {
            let meta = unsafe { &*self.slot_meta(SlotIndex::new(i)) };
            if meta.state.load(Ordering::Relaxed) == SlotState::Free as u32 {
                return Some(i);
            }
        }
        None
    }

    /// Free a slot (called by receiver after processing).
    pub fn free(&self, slot: SlotIndex, expected_gen: Generation) {
        let meta = unsafe { &*self.slot_meta(slot) };

        // Verify generation to prevent double-free or stale free
        let current_gen = meta.generation.load(Ordering::Acquire);
        if current_gen != expected_gen.get() {
            // Stale generation, ignore
            return;
        }

        // Mark as free
        meta.state.store(SlotState::Free as u32, Ordering::Release);

        // Update free list head
        loop {
            let head = self.free_head.load(Ordering::Acquire);
            let head_idx = (head & 0xFFFF_FFFF) as u32;

            // If this slot comes before current head, make it the new head
            if slot.get() < head_idx || head_idx >= self.slot_count {
                let new_head = ((head >> 32).wrapping_add(1) << 32) | (slot.get() as u64);
                if self.free_head.compare_exchange_weak(
                    head,
                    new_head,
                    Ordering::AcqRel,
                    Ordering::Relaxed,
                ).is_ok() {
                    break;
                }
            } else {
                break; // Head is already at a lower index
            }
        }
    }

    /// Get the slot count
    pub fn slot_count(&self) -> u32 {
        self.slot_count
    }

    /// Get the slot size
    pub fn slot_size(&self) -> u32 {
        self.slot_size
    }

    /// Get generation for a slot
    pub fn generation(&self, slot: SlotIndex) -> Generation {
        let meta = unsafe { &*self.slot_meta(slot) };
        Generation::new(meta.generation.load(Ordering::Acquire))
    }

    /// Get data slice for a slot (used by receiver)
    pub fn slot_data(&self, slot: SlotIndex, offset: u32, len: u32) -> &[u8] {
        assert!(slot.get() < self.slot_count);
        assert!(offset.saturating_add(len) <= self.slot_size);

        unsafe {
            let base = self.data_base.as_ptr()
                .add((slot.get() as usize) * (self.slot_size as usize))
                .add(offset as usize);
            std::slice::from_raw_parts(base, len as usize)
        }
    }

    /// Get mutable data slice for a slot (used by sender)
    pub fn slot_data_mut(&self, slot: SlotIndex) -> &mut [u8] {
        assert!(slot.get() < self.slot_count);

        unsafe {
            let base = self.data_base.as_ptr()
                .add((slot.get() as usize) * (self.slot_size as usize));
            std::slice::from_raw_parts_mut(base, self.slot_size as usize)
        }
    }

    fn slot_meta(&self, slot: SlotIndex) -> *const SlotMeta {
        assert!(slot.get() < self.slot_count);
        unsafe { self.meta_base.as_ptr().add(slot.get() as usize) }
    }

    unsafe fn slot_meta_mut(&self, slot: SlotIndex) -> *mut SlotMeta {
        assert!(slot.get() < self.slot_count);
        self.meta_base.as_ptr().add(slot.get() as usize)
    }
}

impl<'seg> OutboundSlot<'seg> {
    /// Get mutable access to the payload buffer.
    pub fn as_mut_bytes(&mut self) -> &mut [u8] {
        self.seg.slot_data_mut(self.slot)
    }

    /// Get the slot index
    pub fn slot(&self) -> SlotIndex {
        self.slot
    }

    /// Get the generation
    pub fn generation(&self) -> Generation {
        self.gen
    }

    /// Commit the slot with the actual payload length.
    ///
    /// After this, the slot will NOT be freed on drop—ownership
    /// transfers to the receiver when the descriptor is enqueued.
    pub fn commit(mut self, len: ByteLen) -> CommittedSlot {
        self.committed = true;

        // Transition to InFlight
        let meta = unsafe { &*self.seg.slot_meta(self.slot) };
        meta.state.store(SlotState::InFlight as u32, Ordering::Release);

        CommittedSlot {
            slot: self.slot,
            gen: self.gen,
            len: len.get(),
        }
    }
}

impl Drop for OutboundSlot<'_> {
    fn drop(&mut self) {
        if !self.committed {
            // Slot was allocated but not used—free it
            self.seg.free(self.slot, self.gen);
        }
    }
}

impl CommittedSlot {
    /// Get descriptor fields for this slot.
    pub fn to_descriptor_fields(&self) -> (u32, u32, u32, u32) {
        (self.slot.get(), self.gen.get(), 0, self.len)
    }

    /// Get the slot index
    pub fn slot(&self) -> SlotIndex {
        self.slot
    }

    /// Get the generation
    pub fn generation(&self) -> Generation {
        self.gen
    }

    /// Get the committed length
    pub fn len(&self) -> u32 {
        self.len
    }
}

impl<'seg> InboundPayload<'seg> {
    /// Create an inbound payload for a slot-based payload
    pub fn from_slot(seg: &'seg DataSegment, slot: SlotIndex, gen: Generation) -> Self {
        InboundPayload {
            seg,
            slot: Some((slot, gen)),
        }
    }

    /// Create an inbound payload for an inline payload (no slot to free)
    pub fn inline(seg: &'seg DataSegment) -> Self {
        InboundPayload {
            seg,
            slot: None,
        }
    }

    /// Check if this is an inline payload
    pub fn is_inline(&self) -> bool {
        self.slot.is_none()
    }
}

impl Drop for InboundPayload<'_> {
    fn drop(&mut self) {
        if let Some((slot, gen)) = self.slot {
            self.seg.free(slot, gen);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::alloc::{alloc_zeroed, dealloc, Layout};

    struct TestSegment {
        meta_ptr: *mut u8,
        data_ptr: *mut u8,
        meta_layout: Layout,
        data_layout: Layout,
        slot_count: u32,
        slot_size: u32,
    }

    impl TestSegment {
        fn new(slot_count: u32, slot_size: u32) -> Self {
            let meta_layout = Layout::array::<SlotMeta>(slot_count as usize).unwrap();
            let data_layout = Layout::from_size_align(
                (slot_count * slot_size) as usize,
                64,
            ).unwrap();

            let meta_ptr = unsafe { alloc_zeroed(meta_layout) };
            let data_ptr = unsafe { alloc_zeroed(data_layout) };

            TestSegment {
                meta_ptr,
                data_ptr,
                meta_layout,
                data_layout,
                slot_count,
                slot_size,
            }
        }

        fn as_segment(&self) -> DataSegment {
            unsafe {
                let seg = DataSegment::from_raw(
                    NonNull::new(self.meta_ptr as *mut SlotMeta).unwrap(),
                    NonNull::new(self.data_ptr).unwrap(),
                    self.slot_size,
                    self.slot_count,
                );
                seg.init_free_list();
                seg
            }
        }
    }

    impl Drop for TestSegment {
        fn drop(&mut self) {
            unsafe {
                dealloc(self.meta_ptr, self.meta_layout);
                dealloc(self.data_ptr, self.data_layout);
            }
        }
    }

    #[test]
    fn alloc_and_commit() {
        let test = TestSegment::new(4, 1024);
        let seg = test.as_segment();

        let mut slot = seg.alloc().unwrap();
        let buf = slot.as_mut_bytes();
        buf[0..5].copy_from_slice(b"hello");

        let committed = slot.commit(ByteLen::new(5, 1024).unwrap());
        assert_eq!(committed.len(), 5);
    }

    #[test]
    fn alloc_drop_without_commit_frees() {
        let test = TestSegment::new(2, 1024);
        let seg = test.as_segment();

        // Allocate and drop without committing
        {
            let _slot = seg.alloc().unwrap();
            // Dropped here, should be freed
        }

        // Should be able to allocate again
        let _slot1 = seg.alloc().unwrap();
        let _slot2 = seg.alloc().unwrap();
    }

    #[test]
    fn inbound_payload_frees_on_drop() {
        let test = TestSegment::new(2, 1024);
        let seg = test.as_segment();

        let slot = seg.alloc().unwrap();
        let committed = slot.commit(ByteLen::new(10, 1024).unwrap());
        let slot_idx = committed.slot();
        let gen = committed.generation();

        // Simulate receiver getting the payload
        {
            let _payload = InboundPayload::from_slot(&seg, slot_idx, gen);
            // Dropped here, should free the slot
        }

        // Slot should be free now - can allocate again
        let _new_slot = seg.alloc().unwrap();
    }

    #[test]
    fn exhaust_slots() {
        let test = TestSegment::new(2, 1024);
        let seg = test.as_segment();

        let _s1 = seg.alloc().unwrap();
        let _s2 = seg.alloc().unwrap();

        // Third allocation should fail
        assert_eq!(seg.alloc().err(), Some(AllocError));
    }

    #[test]
    fn generation_increments() {
        let test = TestSegment::new(1, 1024);
        let seg = test.as_segment();

        let slot1 = seg.alloc().unwrap();
        let gen1 = slot1.generation();
        let committed = slot1.commit(ByteLen::new(1, 1024).unwrap());

        // Free it
        {
            let _payload = InboundPayload::from_slot(&seg, committed.slot(), committed.generation());
        }

        // Allocate again - generation should have incremented
        let slot2 = seg.alloc().unwrap();
        let gen2 = slot2.generation();

        assert!(gen2.get() > gen1.get());
    }
}
