//! Bitmap-based fixed-size slot pools (shm-spec).
//!
//! Slots are allocated by clearing a bit in a shared bitmap and freed by
//! setting it again. Each slot begins with a 4-byte generation counter
//! used for ABA detection.

use core::sync::atomic::{AtomicU32, AtomicU64, Ordering};

use shm_primitives::{Region, SlotHandle};

use crate::layout::SegmentConfig;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum FreeError {
    GenerationMismatch,
    InvalidIndex,
}

pub(crate) struct SlotPool {
    region: Region,
    pool_offset: u64,
    slot_size: u32,
    slots_per_guest: u32,
    header_size: usize,
    bitmap_words: usize,
}

impl SlotPool {
    pub(crate) fn new(region: Region, pool_offset: u64, config: &SegmentConfig) -> Self {
        let bitmap_words = (config.slots_per_guest as usize).div_ceil(64);
        let bitmap_bytes = bitmap_words * 8;
        let header_size = align_up_usize(bitmap_bytes, 64);

        Self {
            region,
            pool_offset,
            slot_size: config.slot_size,
            slots_per_guest: config.slots_per_guest,
            header_size,
            bitmap_words,
        }
    }

    /// Update the region after a resize/remap.
    ///
    /// Call this after the underlying MmapRegion has been resized.
    pub(crate) fn update_region(&mut self, region: Region) {
        self.region = region;
    }

    fn bitmap_ptr(&self) -> *mut AtomicU64 {
        self.region.offset(self.pool_offset as usize) as *mut AtomicU64
    }

    fn slot_base_offset(&self, index: u32) -> Option<usize> {
        if index >= self.slots_per_guest {
            return None;
        }
        Some(
            self.pool_offset as usize + self.header_size + index as usize * self.slot_size as usize,
        )
    }

    fn generation_ptr(&self, index: u32) -> Option<*mut AtomicU32> {
        Some(self.region.offset(self.slot_base_offset(index)?) as *mut AtomicU32)
    }

    pub(crate) fn is_free(&self, index: u32) -> bool {
        if index >= self.slots_per_guest {
            return false;
        }
        let word = (index / 64) as usize;
        let bit = (index % 64) as u64;
        let mask = 1u64 << bit;
        let ptr = unsafe { &*self.bitmap_ptr().add(word) };
        (ptr.load(Ordering::Acquire) & mask) != 0
    }

    /// Count how many slots are currently allocated (not free).
    pub(crate) fn allocated_count(&self) -> u32 {
        let mut free_count = 0u32;
        for word_index in 0..self.bitmap_words {
            let word_ptr = unsafe { &*self.bitmap_ptr().add(word_index) };
            let word = word_ptr.load(Ordering::Acquire);
            free_count += word.count_ones();
        }
        // Excess bits in the last word are cleared to 0 during init,
        // so count_ones already excludes them. No adjustment needed.
        self.slots_per_guest.saturating_sub(free_count)
    }

    /// Get total number of slots in this pool.
    pub(crate) fn total_slots(&self) -> u32 {
        self.slots_per_guest
    }

    pub(crate) fn is_reclaimed(&self, handle: SlotHandle) -> bool {
        let Some(gen_ptr) = self.generation_ptr(handle.index) else {
            return true;
        };
        let current_gen = unsafe { &*gen_ptr }.load(Ordering::Acquire);
        if current_gen != handle.generation {
            return true;
        }
        self.is_free(handle.index)
    }

    /// Try to allocate a slot from the pool.
    ///
    /// Returns `None` if no slots are available.
    pub(crate) fn try_alloc(&self) -> Option<SlotHandle> {
        // shm[impl shm.slot.allocate]
        for word_index in 0..self.bitmap_words {
            let word_ptr = unsafe { &*self.bitmap_ptr().add(word_index) };
            let mut current = word_ptr.load(Ordering::Acquire);

            while current != 0 {
                let bit = current.trailing_zeros();
                let slot_index = (word_index as u32) * 64 + bit;
                if slot_index >= self.slots_per_guest {
                    break;
                }

                let mask = 1u64 << bit;
                let next = current & !mask;

                match word_ptr.compare_exchange_weak(
                    current,
                    next,
                    Ordering::AcqRel,
                    Ordering::Acquire,
                ) {
                    Ok(_) => {
                        // Increment generation counter (slot is now allocated).
                        let gen_ptr = self.generation_ptr(slot_index)?;
                        let generation = unsafe { &*gen_ptr }
                            .fetch_add(1, Ordering::AcqRel)
                            .wrapping_add(1);
                        let handle = SlotHandle {
                            index: slot_index,
                            generation,
                        };
                        trace!(
                            slot_index,
                            generation,
                            allocated = self.allocated_count(),
                            total = self.slots_per_guest,
                            "slot allocated"
                        );
                        return Some(handle);
                    }
                    Err(actual) => current = actual,
                }
            }
        }

        None
    }

    pub(crate) fn payload_ptr(&self, index: u32, payload_offset: u32) -> Option<*mut u8> {
        // shm[impl shm.slot.payload-offset]
        let base = self.slot_base_offset(index)?;
        Some(self.region.offset(base + 4 + payload_offset as usize))
    }

    pub(crate) fn generation(&self, index: u32) -> Option<u32> {
        Some(unsafe { &*self.generation_ptr(index)? }.load(Ordering::Acquire))
    }

    pub(crate) fn free(&self, handle: SlotHandle) -> Result<(), FreeError> {
        // shm[impl shm.slot.free]
        if handle.index >= self.slots_per_guest {
            return Err(FreeError::InvalidIndex);
        }

        let Some(gen_ptr) = self.generation_ptr(handle.index) else {
            return Err(FreeError::InvalidIndex);
        };
        let current_gen = unsafe { &*gen_ptr }.load(Ordering::Acquire);
        if current_gen != handle.generation {
            return Err(FreeError::GenerationMismatch);
        }

        let word = (handle.index / 64) as usize;
        let bit = (handle.index % 64) as u64;
        let mask = 1u64 << bit;
        let word_ptr = unsafe { &*self.bitmap_ptr().add(word) };
        let _ = word_ptr.fetch_or(mask, Ordering::Release);
        trace!(
            slot_index = handle.index,
            generation = handle.generation,
            allocated = self.allocated_count(),
            total = self.slots_per_guest,
            "slot freed"
        );
        Ok(())
    }

    /// Initialize a slot pool in a freshly-created segment.
    ///
    /// # Safety
    ///
    /// Caller must ensure exclusive access (no concurrent users).
    pub(crate) unsafe fn init(region: &Region, pool_offset: u64, config: &SegmentConfig) {
        let pool = Self::new(*region, pool_offset, config);

        // Initialize bitmap: all slots free (all bits set to 1).
        let bitmap_ptr = pool.bitmap_ptr();
        for i in 0..pool.bitmap_words {
            unsafe { bitmap_ptr.add(i).write(AtomicU64::new(u64::MAX)) };
        }

        // Clear any excess bits in the last word.
        let excess = (config.slots_per_guest as u64) % 64;
        if excess != 0 {
            let last_word = unsafe { &*bitmap_ptr.add(pool.bitmap_words - 1) };
            last_word.store((1u64 << excess) - 1, Ordering::Release);
        }

        // Initialize slot generations to 0.
        for i in 0..config.slots_per_guest {
            let base = pool.slot_base_offset(i).expect("slot_base_offset");
            let gen_ptr = region.offset(base) as *mut AtomicU32;
            unsafe { gen_ptr.write(AtomicU32::new(0)) };
        }
    }

    /// Reset a pool's free bitmap (all slots free), preserving generations.
    ///
    /// # Safety
    ///
    /// Caller must ensure the owning peer is dead/detached and will not
    /// concurrently allocate/free from this pool.
    pub(crate) unsafe fn reset_free_bitmap(&self) {
        let bitmap_ptr = self.bitmap_ptr();
        for i in 0..self.bitmap_words {
            unsafe { &*bitmap_ptr.add(i) }.store(u64::MAX, Ordering::Release);
        }

        let excess = (self.slots_per_guest as u64) % 64;
        if excess != 0 {
            let last_word = unsafe { &*bitmap_ptr.add(self.bitmap_words - 1) };
            last_word.store((1u64 << excess) - 1, Ordering::Release);
        }
    }
}

#[inline]
const fn align_up_usize(value: usize, align: usize) -> usize {
    (value + (align - 1)) & !(align - 1)
}
