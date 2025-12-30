//! Hub allocator: Per-class Treiber stack allocator with extent management.
//!
//! Ported from `rapace-transport-shm` (hub transport).

use std::sync::atomic::AtomicU32;
use std::sync::atomic::Ordering;

use super::hub_layout::{
    ExtentHeader, FREE_LIST_END, HUB_SIZE_CLASSES, HubSlotError, HubSlotMeta, NO_OWNER,
    NUM_SIZE_CLASSES, SizeClassHeader, SlotState, decode_global_index, encode_global_index,
    pack_free_head, unpack_free_head,
};
use shm_primitives::futex_signal;

/// A view into the hub's allocator state.
///
/// This provides safe access to allocation operations. The actual memory
/// is in the SHM segment; this struct holds pointers into it.
pub struct HubAllocator {
    size_classes: [*mut SizeClassHeader; NUM_SIZE_CLASSES],
    base_addr: *mut u8,
}

unsafe impl Send for HubAllocator {}
unsafe impl Sync for HubAllocator {}

impl HubAllocator {
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

    #[inline]
    fn class_header(&self, class: usize) -> &SizeClassHeader {
        debug_assert!(class < NUM_SIZE_CLASSES);
        unsafe { &*self.size_classes[class] }
    }

    pub fn slot_available_futex(&self, class: usize) -> &AtomicU32 {
        &self.class_header(class).slot_available_futex
    }

    #[inline]
    unsafe fn extent_header(&self, offset: u64) -> &ExtentHeader {
        let ptr = unsafe { self.base_addr.add(offset as usize) as *const ExtentHeader };
        unsafe { &*ptr }
    }

    unsafe fn slot_meta(&self, class: usize, global_index: u32) -> &HubSlotMeta {
        let header = self.class_header(class);
        let extent_slot_shift = header.extent_slot_shift;
        let (extent_id, slot_in_extent) = decode_global_index(global_index, extent_slot_shift);

        let extent_offset = header.extent_offsets[extent_id as usize].load(Ordering::Acquire);
        if extent_offset == 0 {
            panic!("invalid extent offset for class {class} extent {extent_id}");
        }

        let extent_header = unsafe { self.extent_header(extent_offset) };
        let meta_offset = extent_header.meta_offset as usize;
        let meta_base = unsafe { self.base_addr.add(extent_offset as usize + meta_offset) };
        let meta_ptr =
            unsafe { meta_base.add(slot_in_extent as usize * std::mem::size_of::<HubSlotMeta>()) };

        unsafe { &*(meta_ptr as *const HubSlotMeta) }
    }

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

    pub fn find_class_for_size(&self, size: usize) -> Option<usize> {
        for (i, (slot_size, _)) in HUB_SIZE_CLASSES.iter().enumerate() {
            if *slot_size as usize >= size {
                return Some(i);
            }
        }
        None
    }

    pub fn slot_size(&self, class: usize) -> u32 {
        self.class_header(class).slot_size
    }

    fn alloc_from_class(&self, class: usize, owner_peer: u32) -> Result<(u32, u32), HubSlotError> {
        let header = self.class_header(class);

        loop {
            let old_head = header.free_head.load(Ordering::Acquire);
            let (global_index, tag) = unpack_free_head(old_head);

            if global_index == FREE_LIST_END {
                return Err(HubSlotError::NoFreeSlots);
            }

            let meta = unsafe { self.slot_meta(class, global_index) };
            let next = meta.next_free.load(Ordering::Acquire);
            let new_head = pack_free_head(next, tag.wrapping_add(1));

            if header
                .free_head
                .compare_exchange_weak(old_head, new_head, Ordering::AcqRel, Ordering::Acquire)
                .is_ok()
            {
                // Mark allocated + set owner
                meta.state
                    .store(SlotState::Allocated as u32, Ordering::Release);
                meta.owner_peer.store(owner_peer, Ordering::Release);
                let slot_generation = meta.generation.load(Ordering::Acquire);
                return Ok((global_index, slot_generation));
            }
        }
    }

    /// Allocate a slot for `len` bytes, returning (class, global_index, generation).
    pub fn alloc(&self, len: usize, owner_peer: u32) -> Result<(u16, u32, u32), HubSlotError> {
        let class = self
            .find_class_for_size(len)
            .ok_or_else(|| HubSlotError::PayloadTooLarge {
                len,
                max: HUB_SIZE_CLASSES[NUM_SIZE_CLASSES - 1].0 as usize,
            })?;

        for c in class..NUM_SIZE_CLASSES {
            if let Ok((idx, slot_generation)) = self.alloc_from_class(c, owner_peer) {
                return Ok((c as u16, idx, slot_generation));
            }
        }

        Err(HubSlotError::NoFreeSlots)
    }

    pub fn mark_in_flight(
        &self,
        class: u16,
        global_index: u32,
        generation: u32,
    ) -> Result<(), HubSlotError> {
        let class = class as usize;
        if class >= NUM_SIZE_CLASSES {
            return Err(HubSlotError::InvalidSizeClass);
        }

        let meta = unsafe { self.slot_meta(class, global_index) };
        let slot_generation = meta.generation.load(Ordering::Acquire);
        if slot_generation != generation {
            return Err(HubSlotError::StaleGeneration);
        }

        let state = meta.state.load(Ordering::Acquire);
        if SlotState::from_u32(state) != Some(SlotState::Allocated) {
            return Err(HubSlotError::InvalidState);
        }

        meta.state
            .store(SlotState::InFlight as u32, Ordering::Release);
        Ok(())
    }

    pub fn free(&self, class: u16, global_index: u32, generation: u32) -> Result<(), HubSlotError> {
        let class = class as usize;
        if class >= NUM_SIZE_CLASSES {
            return Err(HubSlotError::InvalidSizeClass);
        }

        let header = self.class_header(class);
        let meta = unsafe { self.slot_meta(class, global_index) };
        let slot_generation = meta.generation.load(Ordering::Acquire);
        if slot_generation != generation {
            return Err(HubSlotError::StaleGeneration);
        }

        let state = meta.state.load(Ordering::Acquire);
        if SlotState::from_u32(state) != Some(SlotState::InFlight) {
            return Err(HubSlotError::InvalidState);
        }

        meta.owner_peer.store(NO_OWNER, Ordering::Release);
        meta.generation.fetch_add(1, Ordering::Release);
        meta.state.store(SlotState::Free as u32, Ordering::Release);

        loop {
            let old_head = header.free_head.load(Ordering::Acquire);
            let (old_index, tag) = unpack_free_head(old_head);
            meta.next_free.store(old_index, Ordering::Release);
            let new_head = pack_free_head(global_index, tag.wrapping_add(1));

            if header
                .free_head
                .compare_exchange_weak(old_head, new_head, Ordering::AcqRel, Ordering::Acquire)
                .is_ok()
            {
                break;
            }
        }

        // Wake allocators waiting for slots.
        // We signal both this class (for selective waiting) and class 0 as a global
        // "some slot freed" futex to keep wait logic simple and robust.
        futex_signal(&header.slot_available_futex);
        if class != 0 {
            futex_signal(&self.class_header(0).slot_available_futex);
        }

        Ok(())
    }

    /// Reclaim all slots owned by a dead peer.
    pub fn reclaim_peer_slots(&self, peer_id: u32) {
        for class in 0..NUM_SIZE_CLASSES {
            let header = self.class_header(class);
            let extent_slot_shift = header.extent_slot_shift;

            for extent_id in 0..super::hub_layout::MAX_EXTENTS_PER_CLASS as u32 {
                let extent_offset =
                    header.extent_offsets[extent_id as usize].load(Ordering::Acquire);
                if extent_offset == 0 {
                    continue;
                }

                let extent_header = unsafe { self.extent_header(extent_offset) };
                let slot_count = extent_header.slot_count;

                for slot_in_extent in 0..slot_count {
                    let global_index =
                        encode_global_index(extent_id, slot_in_extent, extent_slot_shift);
                    let meta = unsafe { self.slot_meta(class, global_index) };
                    if meta.owner_peer.load(Ordering::Acquire) != peer_id {
                        continue;
                    }

                    // Force to in-flight to satisfy free() state machine if needed.
                    let state = meta.state.load(Ordering::Acquire);
                    if SlotState::from_u32(state) == Some(SlotState::Free) {
                        continue;
                    }

                    meta.state
                        .store(SlotState::InFlight as u32, Ordering::Release);
                    let slot_generation = meta.generation.load(Ordering::Acquire);
                    let _ = self.free(class as u16, global_index, slot_generation);
                }
            }
        }
    }
}

/// Initialize all slots in an extent and link them into the free list.
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

    let extent_header = unsafe { allocator.extent_header(extent_offset) };
    let slot_count = extent_header.slot_count;
    if slot_count == 0 {
        return;
    }

    for i in 0..slot_count {
        let global_index = encode_global_index(extent_id, i, extent_slot_shift);
        let meta = unsafe { allocator.slot_meta(class, global_index) };

        let next_in_extent = if i + 1 < slot_count {
            encode_global_index(extent_id, i + 1, extent_slot_shift)
        } else {
            FREE_LIST_END
        };

        meta.generation.store(0, Ordering::Relaxed);
        meta.state.store(SlotState::Free as u32, Ordering::Relaxed);
        meta.next_free.store(next_in_extent, Ordering::Relaxed);
        meta.owner_peer.store(NO_OWNER, Ordering::Relaxed);
    }

    let first_global = encode_global_index(extent_id, 0, extent_slot_shift);
    let last_global = encode_global_index(extent_id, slot_count - 1, extent_slot_shift);

    loop {
        let old_head = header.free_head.load(Ordering::Acquire);
        let (old_index, tag) = unpack_free_head(old_head);

        let last_meta = unsafe { allocator.slot_meta(class, last_global) };
        last_meta.next_free.store(old_index, Ordering::Release);

        let new_head = pack_free_head(first_global, tag.wrapping_add(1));
        if header
            .free_head
            .compare_exchange_weak(old_head, new_head, Ordering::AcqRel, Ordering::Acquire)
            .is_ok()
        {
            break;
        }
    }

    header.total_slots.fetch_add(slot_count, Ordering::AcqRel);
}
