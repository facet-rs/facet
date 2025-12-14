//! Hub allocator: Per-class Treiber stack allocator with extent management.
//!
//! This module implements the MPMC lock-free allocator for the hub's shared
//! slot pool. Each size class has its own free list (Treiber stack) and
//! extent directory.
//!
//! # Allocation Strategy
//!
//! 1. Find smallest size class that fits the requested size
//! 2. Pop from that class's free list (O(1) with CAS)
//! 3. If empty, fall back to larger classes
//! 4. If all classes exhausted, return error (or wait on futex)
//!
//! # Free List Structure
//!
//! Each size class uses a tagged Treiber stack:
//! - `free_head: AtomicU64` = (tag << 32) | global_index
//! - Tag increments on each push/pop for ABA safety
//! - `next_free` stored in SlotMeta (not in slot data) for MPMC safety

use std::sync::atomic::Ordering;

use crate::hub_layout::{
    ExtentHeader, FREE_LIST_END, HUB_SIZE_CLASSES, HubSlotError, HubSlotMeta, NO_OWNER,
    NUM_SIZE_CLASSES, SizeClassHeader, SlotState, decode_global_index, encode_global_index,
    pack_free_head, unpack_free_head,
};

/// A view into the hub's allocator state.
///
/// This provides safe access to allocation operations. The actual memory
/// is in the SHM segment; this struct holds pointers into it.
pub struct HubAllocator {
    /// Pointers to size class headers.
    size_classes: [*mut SizeClassHeader; NUM_SIZE_CLASSES],
    /// Base address of the SHM mapping (for offset calculations).
    base_addr: *mut u8,
}

// SAFETY: HubAllocator is Send + Sync because it points to shared memory
// that is synchronized via atomics.
unsafe impl Send for HubAllocator {}
unsafe impl Sync for HubAllocator {}

impl HubAllocator {
    /// Create an allocator view from raw pointers.
    ///
    /// # Safety
    ///
    /// - `size_classes` must point to valid, initialized `SizeClassHeader` structs.
    /// - `base_addr` must be the base of the SHM mapping.
    /// - The memory must remain valid for the lifetime of this allocator.
    pub unsafe fn from_raw(
        size_classes: [*mut SizeClassHeader; NUM_SIZE_CLASSES],
        base_addr: *mut u8,
    ) -> Self {
        Self {
            size_classes,
            base_addr,
        }
    }

    /// Get a size class header.
    #[inline]
    fn class_header(&self, class: usize) -> &SizeClassHeader {
        debug_assert!(class < NUM_SIZE_CLASSES);
        // SAFETY: Caller guaranteed valid pointers in from_raw.
        unsafe { &*self.size_classes[class] }
    }

    /// Get the extent header at a given offset.
    ///
    /// # Safety
    ///
    /// Offset must be valid and point to an initialized ExtentHeader.
    #[inline]
    unsafe fn extent_header(&self, offset: u64) -> &ExtentHeader {
        let ptr = unsafe { self.base_addr.add(offset as usize) as *const ExtentHeader };
        unsafe { &*ptr }
    }

    /// Get the slot metadata for a global index in a class.
    ///
    /// # Safety
    ///
    /// Class and global_index must be valid.
    unsafe fn slot_meta(&self, class: usize, global_index: u32) -> &HubSlotMeta {
        let header = self.class_header(class);
        let extent_slot_shift = header.extent_slot_shift;
        let (extent_id, slot_in_extent) = decode_global_index(global_index, extent_slot_shift);

        let extent_offset = header.extent_offsets[extent_id as usize].load(Ordering::Acquire);
        if extent_offset == 0 {
            panic!(
                "Invalid extent offset for class {} extent {}",
                class, extent_id
            );
        }

        let extent_header = unsafe { self.extent_header(extent_offset) };
        let meta_offset = extent_header.meta_offset as usize;
        let meta_base = unsafe { self.base_addr.add(extent_offset as usize + meta_offset) };
        let meta_ptr =
            unsafe { meta_base.add(slot_in_extent as usize * std::mem::size_of::<HubSlotMeta>()) };

        unsafe { &*(meta_ptr as *const HubSlotMeta) }
    }

    /// Get the slot data pointer for a global index in a class.
    ///
    /// # Safety
    ///
    /// Class and global_index must be valid, caller must own the slot.
    pub unsafe fn slot_data_ptr(&self, class: usize, global_index: u32) -> *mut u8 {
        let header = self.class_header(class);
        let extent_slot_shift = header.extent_slot_shift;
        let slot_size = header.slot_size as usize;
        let (extent_id, slot_in_extent) = decode_global_index(global_index, extent_slot_shift);

        let extent_offset = header.extent_offsets[extent_id as usize].load(Ordering::Acquire);
        let extent_header = unsafe { self.extent_header(extent_offset) };
        let data_offset = extent_header.data_offset as usize;
        let data_base = unsafe { self.base_addr.add(extent_offset as usize + data_offset) };

        unsafe { data_base.add(slot_in_extent as usize * slot_size) }
    }

    /// Find the smallest size class that can fit the given payload size.
    pub fn find_class_for_size(&self, size: usize) -> Option<usize> {
        for (i, (slot_size, _)) in HUB_SIZE_CLASSES.iter().enumerate() {
            if *slot_size as usize >= size {
                return Some(i);
            }
        }
        None
    }

    /// Get the slot size for a class.
    pub fn slot_size(&self, class: usize) -> u32 {
        self.class_header(class).slot_size
    }

    /// Allocate a slot from the given size class.
    ///
    /// Returns (global_index, generation) on success.
    fn alloc_from_class(&self, class: usize, owner_peer: u32) -> Result<(u32, u32), HubSlotError> {
        let header = self.class_header(class);

        loop {
            // Load current head
            let old_head = header.free_head.load(Ordering::Acquire);
            let (global_index, tag) = unpack_free_head(old_head);

            // Check if list is empty
            if global_index == FREE_LIST_END {
                return Err(HubSlotError::NoFreeSlots);
            }

            // Read next pointer from slot meta
            // SAFETY: global_index came from free list, should be valid
            let meta = unsafe { self.slot_meta(class, global_index) };
            let next = meta.next_free.load(Ordering::Acquire);

            // Try to CAS head to next, incrementing tag for ABA safety
            let new_head = pack_free_head(next, tag.wrapping_add(1));

            if header
                .free_head
                .compare_exchange_weak(old_head, new_head, Ordering::AcqRel, Ordering::Acquire)
                .is_ok()
            {
                // Successfully popped. Transition Free -> Allocated.
                let result = meta.state.compare_exchange(
                    SlotState::Free as u32,
                    SlotState::Allocated as u32,
                    Ordering::AcqRel,
                    Ordering::Acquire,
                );

                if result.is_err() {
                    // Slot was not Free - shouldn't happen if free list is consistent.
                    // Push it back and try again.
                    self.push_to_free_list(class, global_index);
                    continue;
                }

                // Set owner
                meta.owner_peer.store(owner_peer, Ordering::Release);

                // Bump generation
                let generation = meta.generation.fetch_add(1, Ordering::AcqRel) + 1;

                return Ok((global_index, generation));
            }
            // CAS failed, retry
        }
    }

    /// Push a slot onto a class's free list.
    fn push_to_free_list(&self, class: usize, global_index: u32) {
        let header = self.class_header(class);

        loop {
            let old_head = header.free_head.load(Ordering::Acquire);
            let (old_index, tag) = unpack_free_head(old_head);

            // Store the old head as our next pointer
            // SAFETY: global_index should be valid
            let meta = unsafe { self.slot_meta(class, global_index) };
            meta.next_free.store(old_index, Ordering::Release);

            // Try to CAS head to point to us
            let new_head = pack_free_head(global_index, tag.wrapping_add(1));

            if header
                .free_head
                .compare_exchange_weak(old_head, new_head, Ordering::AcqRel, Ordering::Acquire)
                .is_ok()
            {
                return;
            }
            // CAS failed, retry
        }
    }

    /// Allocate a slot for the given payload size.
    ///
    /// Returns (class, global_index, generation) on success.
    pub fn alloc(&self, size: usize, owner_peer: u32) -> Result<(u8, u32, u32), HubSlotError> {
        // Find smallest class that fits
        let start_class = self
            .find_class_for_size(size)
            .ok_or(HubSlotError::PayloadTooLarge {
                len: size,
                max: HUB_SIZE_CLASSES[NUM_SIZE_CLASSES - 1].0 as usize,
            })?;

        // Try to allocate from this class
        if let Ok((global_index, generation)) = self.alloc_from_class(start_class, owner_peer) {
            return Ok((start_class as u8, global_index, generation));
        }

        // Fall back to larger classes
        for class in (start_class + 1)..NUM_SIZE_CLASSES {
            if let Ok((global_index, generation)) = self.alloc_from_class(class, owner_peer) {
                return Ok((class as u8, global_index, generation));
            }
        }

        Err(HubSlotError::NoFreeSlots)
    }

    /// Mark a slot as in-flight (after enqueuing descriptor).
    pub fn mark_in_flight(
        &self,
        class: u8,
        global_index: u32,
        expected_gen: u32,
    ) -> Result<(), HubSlotError> {
        if class as usize >= NUM_SIZE_CLASSES {
            return Err(HubSlotError::InvalidSizeClass);
        }

        // SAFETY: class validated above
        let meta = unsafe { self.slot_meta(class as usize, global_index) };

        // Verify generation
        if meta.generation.load(Ordering::Acquire) != expected_gen {
            return Err(HubSlotError::StaleGeneration);
        }

        // Transition Allocated -> InFlight
        meta.state
            .compare_exchange(
                SlotState::Allocated as u32,
                SlotState::InFlight as u32,
                Ordering::AcqRel,
                Ordering::Acquire,
            )
            .map_err(|_| HubSlotError::InvalidState)?;

        Ok(())
    }

    /// Free a slot (receiver side, after processing).
    pub fn free(
        &self,
        class: u8,
        global_index: u32,
        expected_gen: u32,
    ) -> Result<(), HubSlotError> {
        if class as usize >= NUM_SIZE_CLASSES {
            return Err(HubSlotError::InvalidSizeClass);
        }

        // SAFETY: class validated above
        let meta = unsafe { self.slot_meta(class as usize, global_index) };

        // Verify generation
        if meta.generation.load(Ordering::Acquire) != expected_gen {
            return Err(HubSlotError::StaleGeneration);
        }

        // Transition InFlight -> Free
        meta.state
            .compare_exchange(
                SlotState::InFlight as u32,
                SlotState::Free as u32,
                Ordering::AcqRel,
                Ordering::Acquire,
            )
            .map_err(|_| HubSlotError::InvalidState)?;

        // Clear owner
        meta.owner_peer.store(NO_OWNER, Ordering::Release);

        // Push back onto free list
        self.push_to_free_list(class as usize, global_index);

        // Signal anyone waiting for slots
        let header = self.class_header(class as usize);
        crate::futex::futex_signal(&header.slot_available);

        Ok(())
    }

    /// Free a slot that's still in Allocated state (never sent).
    pub fn free_allocated(
        &self,
        class: u8,
        global_index: u32,
        expected_gen: u32,
    ) -> Result<(), HubSlotError> {
        if class as usize >= NUM_SIZE_CLASSES {
            return Err(HubSlotError::InvalidSizeClass);
        }

        // SAFETY: class validated above
        let meta = unsafe { self.slot_meta(class as usize, global_index) };

        // Verify generation
        if meta.generation.load(Ordering::Acquire) != expected_gen {
            return Err(HubSlotError::StaleGeneration);
        }

        // Transition Allocated -> Free
        meta.state
            .compare_exchange(
                SlotState::Allocated as u32,
                SlotState::Free as u32,
                Ordering::AcqRel,
                Ordering::Acquire,
            )
            .map_err(|_| HubSlotError::InvalidState)?;

        // Clear owner
        meta.owner_peer.store(NO_OWNER, Ordering::Release);

        // Push back onto free list
        self.push_to_free_list(class as usize, global_index);

        // Signal anyone waiting for slots
        let header = self.class_header(class as usize);
        crate::futex::futex_signal(&header.slot_available);

        Ok(())
    }

    /// Force-free all slots owned by a dead peer.
    ///
    /// This is called during crash cleanup. It:
    /// 1. Scans all slots in all extents
    /// 2. For slots owned by the dead peer: force state to Free, bump generation
    /// 3. Pushes freed slots back onto the free list
    pub fn reclaim_peer_slots(&self, dead_peer_id: u32) {
        for class in 0..NUM_SIZE_CLASSES {
            let header = self.class_header(class);
            let extent_slot_shift = header.extent_slot_shift;
            let extent_count = header.extent_count as usize;

            for extent_id in 0..extent_count {
                let extent_offset = header.extent_offsets[extent_id].load(Ordering::Acquire);
                if extent_offset == 0 {
                    continue;
                }

                // SAFETY: extent_offset is valid (we're iterating known extents)
                let extent_header = unsafe { self.extent_header(extent_offset) };
                let slot_count = extent_header.slot_count;

                for slot_in_extent in 0..slot_count {
                    let global_index =
                        encode_global_index(extent_id as u32, slot_in_extent, extent_slot_shift);

                    // SAFETY: we're iterating valid indices
                    let meta = unsafe { self.slot_meta(class, global_index) };

                    // Check if owned by dead peer
                    if meta.owner_peer.load(Ordering::Acquire) != dead_peer_id {
                        continue;
                    }

                    // Force to Free state
                    meta.state.store(SlotState::Free as u32, Ordering::Release);

                    // Bump generation to invalidate any in-flight descriptors
                    meta.generation.fetch_add(1, Ordering::AcqRel);

                    // Clear owner
                    meta.owner_peer.store(NO_OWNER, Ordering::Release);

                    // Push onto free list
                    self.push_to_free_list(class, global_index);
                }
            }

            // Signal that slots are available
            crate::futex::futex_signal(&header.slot_available);
        }
    }

    /// Get the slot_available futex for a size class.
    pub fn slot_available_futex(&self, class: usize) -> &std::sync::atomic::AtomicU32 {
        &self.class_header(class).slot_available
    }

    /// Get slot status for all size classes (for diagnostics).
    ///
    /// Returns a summary of slot states across all classes.
    pub fn slot_status(&self) -> HubSlotStatus {
        let mut status = HubSlotStatus::default();

        for (class, class_out) in status.classes.iter_mut().enumerate() {
            let header = self.class_header(class);
            let extent_slot_shift = header.extent_slot_shift;
            let extent_count = header.extent_count as usize;
            let slot_size = header.slot_size;

            let mut class_status = SizeClassStatus {
                slot_size,
                total: 0,
                free: 0,
                allocated: 0,
                in_flight: 0,
            };

            for extent_id in 0..extent_count {
                let extent_offset = header.extent_offsets[extent_id].load(Ordering::Acquire);
                if extent_offset == 0 {
                    continue;
                }

                // SAFETY: extent_offset is valid (we're iterating known extents)
                let extent_header = unsafe { self.extent_header(extent_offset) };
                let slot_count = extent_header.slot_count;
                class_status.total += slot_count;

                for slot_in_extent in 0..slot_count {
                    let global_index =
                        encode_global_index(extent_id as u32, slot_in_extent, extent_slot_shift);

                    // SAFETY: we're iterating valid indices
                    let meta = unsafe { self.slot_meta(class, global_index) };
                    let state = meta.state.load(Ordering::Acquire);

                    match SlotState::from_u32(state) {
                        Some(SlotState::Free) => class_status.free += 1,
                        Some(SlotState::Allocated) => class_status.allocated += 1,
                        Some(SlotState::InFlight) => class_status.in_flight += 1,
                        None => {} // Unknown state
                    }
                }
            }

            *class_out = class_status;
            status.total += class_status.total;
            status.free += class_status.free;
            status.allocated += class_status.allocated;
            status.in_flight += class_status.in_flight;
        }

        status
    }
}

/// Status of a single size class.
#[derive(Debug, Clone, Copy, Default)]
pub struct SizeClassStatus {
    /// Slot size in bytes.
    pub slot_size: u32,
    /// Total slots in this class.
    pub total: u32,
    /// Free slots.
    pub free: u32,
    /// Allocated (being written).
    pub allocated: u32,
    /// In-flight (enqueued, waiting for receiver).
    pub in_flight: u32,
}

/// Status of all slots in the hub allocator.
#[derive(Debug, Clone, Default)]
pub struct HubSlotStatus {
    /// Per-class status.
    pub classes: [SizeClassStatus; NUM_SIZE_CLASSES],
    /// Total slots across all classes.
    pub total: u32,
    /// Total free slots.
    pub free: u32,
    /// Total allocated slots.
    pub allocated: u32,
    /// Total in-flight slots.
    pub in_flight: u32,
}

impl std::fmt::Display for HubSlotStatus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        writeln!(
            f,
            "HubAllocator slots: {} total, {} free, {} allocated, {} in_flight",
            self.total, self.free, self.allocated, self.in_flight
        )?;
        for (i, class) in self.classes.iter().enumerate() {
            if class.total > 0 {
                writeln!(
                    f,
                    "  class[{}] ({:>7}B): {:>3} total, {:>3} free, {:>3} alloc, {:>3} in_flight",
                    i, class.slot_size, class.total, class.free, class.allocated, class.in_flight
                )?;
            }
        }
        Ok(())
    }
}

/// Initialize all slots in an extent and link them into the free list.
///
/// This is called during extent creation. It:
/// 1. Initializes all SlotMeta entries
/// 2. Bulk-pushes all slots as a chain onto the free list (one CAS for the whole chain)
///
/// # Safety
///
/// - `extent_offset` must point to a valid, mapped extent
/// - `allocator` must be valid
/// - Must only be called during extent initialization (before concurrent access)
pub unsafe fn init_extent_free_list(
    allocator: &HubAllocator,
    class: usize,
    extent_id: u32,
    extent_offset: u64,
) {
    let header = allocator.class_header(class);
    let extent_slot_shift = header.extent_slot_shift;

    // SAFETY: caller guarantees valid extent
    let extent_header = unsafe { allocator.extent_header(extent_offset) };
    let slot_count = extent_header.slot_count;
    let _base_global_index = extent_header.base_global_index;

    if slot_count == 0 {
        return;
    }

    // Initialize all slot metas and link them together
    // We'll create a chain: slot[0] -> slot[1] -> ... -> slot[n-1] -> (old_head)
    // Then CAS the chain head to slot[0]

    // First pass: initialize metas and link them (except last slot)
    for i in 0..slot_count {
        let global_index = encode_global_index(extent_id, i, extent_slot_shift);
        let meta = unsafe { allocator.slot_meta(class, global_index) };

        // Next slot in this extent, or FREE_LIST_END if last
        let next_in_extent = if i + 1 < slot_count {
            encode_global_index(extent_id, i + 1, extent_slot_shift)
        } else {
            FREE_LIST_END // Will be updated when we link to existing chain
        };

        meta.generation.store(0, Ordering::Relaxed);
        meta.state.store(SlotState::Free as u32, Ordering::Relaxed);
        meta.next_free.store(next_in_extent, Ordering::Relaxed);
        meta.owner_peer.store(NO_OWNER, Ordering::Relaxed);
    }

    // Now bulk-push the entire chain onto the free list
    // The chain is: first_slot -> ... -> last_slot -> (need to link to old head)
    let first_global = encode_global_index(extent_id, 0, extent_slot_shift);
    let last_global = encode_global_index(extent_id, slot_count - 1, extent_slot_shift);

    loop {
        let old_head = header.free_head.load(Ordering::Acquire);
        let (old_index, tag) = unpack_free_head(old_head);

        // Link last slot to old head
        let last_meta = unsafe { allocator.slot_meta(class, last_global) };
        last_meta.next_free.store(old_index, Ordering::Release);

        // Try to CAS head to first slot
        let new_head = pack_free_head(first_global, tag.wrapping_add(1));

        if header
            .free_head
            .compare_exchange_weak(old_head, new_head, Ordering::AcqRel, Ordering::Acquire)
            .is_ok()
        {
            break;
        }
        // CAS failed, retry
    }

    // Update total slots count
    header.total_slots.fetch_add(slot_count, Ordering::AcqRel);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_find_class_for_size() {
        // Test that sizes map to correct classes
        // Class 0: 1KB, Class 1: 16KB, Class 2: 256KB, Class 3: 4MB, Class 4: 16MB

        // Create a mock allocator (we just need the logic, not real memory)
        // For this test, we can use the constants directly

        assert!(HUB_SIZE_CLASSES[0].0 >= 1024);

        // 500 bytes should fit in class 0 (1KB)
        for (i, (slot_size, _)) in HUB_SIZE_CLASSES.iter().enumerate() {
            if *slot_size as usize >= 500 {
                assert_eq!(i, 0);
                break;
            }
        }

        // 2KB should fit in class 1 (16KB) since class 0 is 1KB
        for (i, (slot_size, _)) in HUB_SIZE_CLASSES.iter().enumerate() {
            if *slot_size as usize >= 2048 {
                assert_eq!(i, 1);
                break;
            }
        }

        // 10MB should fit in class 4 (16MB)
        let ten_mb = 10 * 1024 * 1024;
        for (i, (slot_size, _)) in HUB_SIZE_CLASSES.iter().enumerate() {
            if *slot_size as usize >= ten_mb {
                assert_eq!(i, 4);
                break;
            }
        }
    }
}
