//! Variable-size slot pools with multiple size classes.
//!
//! This module implements shared variable-size slot pools as specified in
//! `docs/content/shm-spec/_index.md`. Unlike fixed-size per-guest pools,
//! variable-size pools are shared across all guests with per-slot ownership
//! tracking for crash recovery.

use core::sync::atomic::{AtomicU32, AtomicU64, Ordering};

use shm_primitives::{Region, SlotState, VarSlotMeta};

use crate::layout::{MAX_EXTENTS_PER_CLASS, SizeClass};

/// Sentinel value indicating end of free list.
pub const FREE_LIST_END: u64 = u64::MAX;

/// Header for a single size class with extent support (64 bytes, cache-line aligned).
///
/// Each size class can have up to MAX_EXTENTS_PER_CLASS extents:
/// - Extent 0: The initial inline extent (in the main pool region)
/// - Extents 1..N: Additional extents appended to the segment via growth
///
/// shm[impl shm.varslot.freelist]
/// shm[impl shm.varslot.extents]
#[repr(C, align(64))]
pub struct SizeClassHeader {
    /// Size of each slot in this class.
    pub slot_size: u32,
    /// Number of slots per extent (same for all extents in this class).
    pub slots_per_extent: u32,
    /// Number of extents currently allocated (1 = initial only, 2-3 = grown).
    pub extent_count: AtomicU32,
    /// Padding for alignment.
    pub _pad: u32,
    /// Free list heads for each extent.
    /// Each is packed (index in upper 32 bits, generation in lower 32 bits).
    /// Uses `FREE_LIST_END` as sentinel for empty list.
    pub free_heads: [AtomicU64; MAX_EXTENTS_PER_CLASS],
    /// Offsets to extents 1 and 2 (extent 0 is at the class's inline position).
    /// Only valid for indices < extent_count - 1.
    pub extent_offsets: [AtomicU64; MAX_EXTENTS_PER_CLASS - 1],
    /// Reserved for future use.
    pub _reserved: [u8; 8],
}

// 4 + 4 + 4 + 4 + 24 + 16 + 8 = 64 bytes
const _: () = assert!(core::mem::size_of::<SizeClassHeader>() == 64);

impl SizeClassHeader {
    /// Pack a slot index and generation into a free list head value.
    #[inline]
    pub fn pack(index: u32, generation: u32) -> u64 {
        ((index as u64) << 32) | (generation as u64)
    }

    /// Unpack a free list head value into (index, generation).
    #[inline]
    pub fn unpack(packed: u64) -> (u32, u32) {
        let index = (packed >> 32) as u32;
        let generation = packed as u32;
        (index, generation)
    }

    /// Initialize a size class header (extent 0 only).
    pub fn init(&mut self, slot_size: u32, slots_per_extent: u32) {
        self.slot_size = slot_size;
        self.slots_per_extent = slots_per_extent;
        self.extent_count = AtomicU32::new(1); // Start with extent 0
        self._pad = 0;
        for free_head in &self.free_heads {
            free_head.store(FREE_LIST_END, Ordering::Relaxed);
        }
        for offset in &self.extent_offsets {
            offset.store(0, Ordering::Relaxed);
        }
        self._reserved = [0; 8];
    }

    /// Get the free list head for a specific extent.
    #[inline]
    pub fn free_head(&self, extent_idx: usize) -> &AtomicU64 {
        &self.free_heads[extent_idx]
    }

    /// Get the number of currently allocated extents.
    #[inline]
    pub fn extent_count(&self) -> u32 {
        self.extent_count.load(Ordering::Acquire)
    }

    /// Get the offset to an extent (extent 0 returns None as it's inline).
    #[inline]
    pub fn extent_offset(&self, extent_idx: usize) -> Option<u64> {
        if extent_idx == 0 {
            None // Extent 0 is inline
        } else if extent_idx < MAX_EXTENTS_PER_CLASS {
            Some(self.extent_offsets[extent_idx - 1].load(Ordering::Acquire))
        } else {
            None
        }
    }
}

/// Handle to an allocated variable-size slot.
///
/// Encodes the size class index, extent index, slot index, and generation.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct VarSlotHandle {
    /// Size class index (0-255).
    pub class_idx: u8,
    /// Extent index within the size class (0-2).
    pub extent_idx: u8,
    /// Slot index within the extent.
    pub slot_idx: u32,
    /// Generation counter for ABA detection.
    pub generation: u32,
}

impl VarSlotHandle {
    /// Sentinel value for inline payloads (no slot allocated).
    pub const INLINE: Self = Self {
        class_idx: 0xFF,
        extent_idx: 0xFF,
        slot_idx: 0x003FFFFF,
        generation: 0,
    };

    /// Check if this is the inline sentinel.
    #[inline]
    pub fn is_inline(&self) -> bool {
        self.class_idx == 0xFF && self.extent_idx == 0xFF && self.slot_idx == 0x003FFFFF
    }

    /// Pack into a u32 for MsgDesc.payload_slot.
    ///
    /// Format:
    /// - Bits 31-24: class_idx (8 bits)
    /// - Bits 23-22: extent_idx (2 bits, 0-3)
    /// - Bits 21-0: slot_idx (22 bits, max ~4M slots per extent)
    #[inline]
    pub fn pack_slot(&self) -> u32 {
        ((self.class_idx as u32) << 24)
            | ((self.extent_idx as u32 & 0x3) << 22)
            | (self.slot_idx & 0x003FFFFF)
    }

    /// Unpack from MsgDesc.payload_slot and payload_generation.
    #[inline]
    pub fn from_packed(payload_slot: u32, payload_generation: u32) -> Self {
        Self {
            class_idx: (payload_slot >> 24) as u8,
            extent_idx: ((payload_slot >> 22) & 0x3) as u8,
            slot_idx: payload_slot & 0x003FFFFF,
            generation: payload_generation,
        }
    }
}

/// Errors from freeing a variable slot.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VarFreeError {
    /// The generation doesn't match (double-free or stale handle).
    GenerationMismatch { expected: u32, actual: u32 },
    /// The slot index is out of range.
    InvalidIndex,
    /// The class index is out of range.
    InvalidClass,
    /// The slot is not in the expected state.
    InvalidState {
        expected: SlotState,
        actual: SlotState,
    },
}

/// Variable-size slot pool with multiple size classes and extent support.
///
/// Each size class can have up to MAX_EXTENTS_PER_CLASS extents:
/// - Extent 0: Inline in the main pool region (offsets computed at construction)
/// - Extents 1-2: Appended to segment via growth (offsets stored in SizeClassHeader)
///
/// shm[impl shm.varslot.shared]
/// shm[impl shm.varslot.extents]
pub struct VarSlotPool {
    region: Region,
    /// Offset to the first size class header.
    base_offset: u64,
    /// Size class configurations.
    classes: Vec<SizeClass>,
    /// Computed offsets to each class's extent 0 metadata array.
    extent0_meta_offsets: Vec<u64>,
    /// Computed offsets to each class's extent 0 data array.
    extent0_data_offsets: Vec<u64>,
}

impl VarSlotPool {
    /// Create a new VarSlotPool view.
    ///
    /// This does not initialize the pool - use `init` for that.
    pub fn new(region: Region, base_offset: u64, classes: Vec<SizeClass>) -> Self {
        let mut extent0_meta_offsets = Vec::with_capacity(classes.len());
        let mut extent0_data_offsets = Vec::with_capacity(classes.len());

        // Headers are at the start
        let headers_size = classes.len() as u64 * 64;
        let mut offset = base_offset + headers_size;

        for class in &classes {
            // Align metadata array
            offset = align_up(offset, 16); // VarSlotMeta is 16 bytes
            extent0_meta_offsets.push(offset);
            offset += class.count as u64 * 16; // VarSlotMeta size

            // Align data array
            offset = align_up(offset, 64);
            extent0_data_offsets.push(offset);
            offset += class.count as u64 * class.slot_size as u64;
        }

        Self {
            region,
            base_offset,
            classes,
            extent0_meta_offsets,
            extent0_data_offsets,
        }
    }

    /// Update the region after a resize/remap.
    ///
    /// Call this after the underlying MmapRegion has been resized.
    pub fn update_region(&mut self, region: Region) {
        self.region = region;
    }

    /// Calculate the total size needed for a variable slot pool (extent 0 only).
    pub fn calculate_size(classes: &[SizeClass]) -> u64 {
        let headers_size = classes.len() as u64 * 64;
        let mut size = headers_size;

        for class in classes {
            // Align metadata array
            size = align_up(size, 16);
            size += class.count as u64 * 16; // VarSlotMeta size

            // Align data array
            size = align_up(size, 64);
            size += class.count as u64 * class.slot_size as u64;
        }

        align_up(size, 64)
    }

    /// Initialize the pool (call once during segment creation).
    ///
    /// This initializes extent 0 for all size classes.
    ///
    /// # Safety
    ///
    /// Caller must ensure exclusive access during initialization.
    pub unsafe fn init(&mut self) {
        // Initialize size class headers
        for i in 0..self.classes.len() {
            let slot_size = self.classes[i].slot_size;
            let count = self.classes[i].count;
            let header = self.class_header_mut(i);
            header.init(slot_size, count);
        }

        // Initialize extent 0 slot metadata and build free lists
        for class_idx in 0..self.classes.len() {
            // SAFETY: We have exclusive access during init (caller's requirement)
            unsafe { self.init_extent_slots(class_idx, 0) };
        }
    }

    /// Initialize slots for a specific extent and build its free list.
    ///
    /// # Safety
    ///
    /// Caller must ensure exclusive access to the extent during initialization.
    pub unsafe fn init_extent_slots(&mut self, class_idx: usize, extent_idx: usize) {
        let class = &self.classes[class_idx];
        let slot_count = class.count;

        // Initialize all slot metadata
        for slot_idx in 0..slot_count {
            if let Some(meta) = self.slot_meta_mut_ext(class_idx, extent_idx, slot_idx) {
                meta.init();
            }
        }

        // Build free list by linking slots together
        // Link 0 -> 1 -> 2 -> ... -> (n-1) -> END
        for slot_idx in 0..slot_count {
            if let Some(meta) = self.slot_meta_mut_ext(class_idx, extent_idx, slot_idx) {
                if slot_idx + 1 < slot_count {
                    meta.next_free.store(slot_idx + 1, Ordering::Release);
                } else {
                    meta.next_free.store(u32::MAX, Ordering::Release);
                }
            }
        }

        // Set free list head for this extent
        let header = self.class_header_mut(class_idx);
        if slot_count > 0 {
            header.free_heads[extent_idx].store(SizeClassHeader::pack(0, 0), Ordering::Release);
        }
    }

    fn class_header_ptr(&self, class_idx: usize) -> *mut SizeClassHeader {
        let offset = self.base_offset as usize + class_idx * 64;
        self.region.offset(offset) as *mut SizeClassHeader
    }

    fn class_header(&self, class_idx: usize) -> &SizeClassHeader {
        unsafe { &*self.class_header_ptr(class_idx) }
    }

    fn class_header_mut(&mut self, class_idx: usize) -> &mut SizeClassHeader {
        unsafe { &mut *self.class_header_ptr(class_idx) }
    }

    /// Get the base offset for an extent's data within the segment.
    /// Used by grow_size_class() when initializing new extents.
    #[allow(dead_code)]
    fn extent_base_offset(&self, class_idx: usize, extent_idx: usize) -> Option<u64> {
        if extent_idx == 0 {
            // Extent 0 is inline - we have precomputed offsets
            Some(self.extent0_meta_offsets[class_idx] - 64) // Approximate base
        } else {
            // Extents 1+ have their offset stored in the header
            let header = self.class_header(class_idx);
            let offset = header.extent_offsets[extent_idx - 1].load(Ordering::Acquire);
            if offset == 0 {
                None // Extent not allocated
            } else {
                Some(offset)
            }
        }
    }

    /// Get a slot's metadata for any extent.
    fn slot_meta_ext(
        &self,
        class_idx: usize,
        extent_idx: usize,
        slot_idx: u32,
    ) -> Option<&VarSlotMeta> {
        let class = &self.classes[class_idx];
        if slot_idx >= class.count {
            return None;
        }

        if extent_idx == 0 {
            // Extent 0: use precomputed offsets
            let offset = self.extent0_meta_offsets[class_idx] as usize + slot_idx as usize * 16;
            Some(unsafe { &*(self.region.offset(offset) as *const VarSlotMeta) })
        } else if extent_idx < MAX_EXTENTS_PER_CLASS {
            // Extents 1+: look up offset from header
            let header = self.class_header(class_idx);
            let extent_offset = header.extent_offsets[extent_idx - 1].load(Ordering::Acquire);
            if extent_offset == 0 {
                return None; // Extent not allocated
            }
            // Extent layout: ExtentHeader (64) + metadata array
            let meta_offset = extent_offset as usize + 64 + slot_idx as usize * 16;
            Some(unsafe { &*(self.region.offset(meta_offset) as *const VarSlotMeta) })
        } else {
            None
        }
    }

    /// Get a mutable slot's metadata for any extent.
    fn slot_meta_mut_ext(
        &mut self,
        class_idx: usize,
        extent_idx: usize,
        slot_idx: u32,
    ) -> Option<&mut VarSlotMeta> {
        let class = &self.classes[class_idx];
        if slot_idx >= class.count {
            return None;
        }

        if extent_idx == 0 {
            let offset = self.extent0_meta_offsets[class_idx] as usize + slot_idx as usize * 16;
            Some(unsafe { &mut *(self.region.offset(offset) as *mut VarSlotMeta) })
        } else if extent_idx < MAX_EXTENTS_PER_CLASS {
            let header = self.class_header(class_idx);
            let extent_offset = header.extent_offsets[extent_idx - 1].load(Ordering::Acquire);
            if extent_offset == 0 {
                return None;
            }
            let meta_offset = extent_offset as usize + 64 + slot_idx as usize * 16;
            Some(unsafe { &mut *(self.region.offset(meta_offset) as *mut VarSlotMeta) })
        } else {
            None
        }
    }

    /// Get a pointer to the slot's payload data area.
    pub fn payload_ptr(&self, handle: VarSlotHandle) -> Option<*mut u8> {
        if handle.class_idx as usize >= self.classes.len() {
            return None;
        }
        let class = &self.classes[handle.class_idx as usize];
        if handle.slot_idx >= class.count {
            return None;
        }
        let extent_idx = handle.extent_idx as usize;

        if extent_idx == 0 {
            // Extent 0: use precomputed offsets
            let offset = self.extent0_data_offsets[handle.class_idx as usize] as usize
                + handle.slot_idx as usize * class.slot_size as usize;
            Some(self.region.offset(offset))
        } else if extent_idx < MAX_EXTENTS_PER_CLASS {
            // Extents 1+: compute from extent offset
            let header = self.class_header(handle.class_idx as usize);
            let extent_offset = header.extent_offsets[extent_idx - 1].load(Ordering::Acquire);
            if extent_offset == 0 {
                return None;
            }
            // Extent layout: ExtentHeader (64) + metadata (count * 16, aligned to 64) + data
            let meta_size = class.count as usize * 16;
            let data_start = extent_offset as usize + 64 + align_up(meta_size as u64, 64) as usize;
            let offset = data_start + handle.slot_idx as usize * class.slot_size as usize;
            Some(self.region.offset(offset))
        } else {
            None
        }
    }

    /// Get the slot size for a given class.
    pub fn slot_size(&self, class_idx: u8) -> Option<u32> {
        self.classes.get(class_idx as usize).map(|c| c.slot_size)
    }

    /// Allocate a slot that can hold `size` bytes.
    ///
    /// shm[impl shm.varslot.selection]
    ///
    /// Finds the smallest size class that fits, with fallback to larger classes
    /// if the preferred class is exhausted.
    pub fn alloc(&self, size: u32, owner: u8) -> Option<VarSlotHandle> {
        // Find smallest class that fits
        for (class_idx, class) in self.classes.iter().enumerate() {
            if class.slot_size >= size
                && let Some(handle) = self.alloc_from_class(class_idx, owner)
            {
                return Some(handle);
            }
            // Class exhausted, try next larger
        }
        None // All classes exhausted
    }

    /// Allocate from a specific size class, trying all available extents.
    ///
    /// shm[impl shm.varslot.allocation]
    /// shm[impl shm.varslot.extents]
    pub fn alloc_from_class(&self, class_idx: usize, owner: u8) -> Option<VarSlotHandle> {
        if class_idx >= self.classes.len() {
            return None;
        }

        let header = self.class_header(class_idx);
        let extent_count = header.extent_count() as usize;

        // Try each extent in order
        for extent_idx in 0..extent_count {
            if let Some(handle) = self.alloc_from_extent(class_idx, extent_idx, owner) {
                return Some(handle);
            }
        }

        None // All extents exhausted
    }

    /// Allocate from a specific extent within a size class.
    fn alloc_from_extent(
        &self,
        class_idx: usize,
        extent_idx: usize,
        owner: u8,
    ) -> Option<VarSlotHandle> {
        let header = self.class_header(class_idx);
        let free_head = header.free_head(extent_idx);

        loop {
            let head = free_head.load(Ordering::Acquire);
            if head == FREE_LIST_END {
                return None; // This extent exhausted
            }

            let (index, tag) = SizeClassHeader::unpack(head);
            let meta = self.slot_meta_ext(class_idx, extent_idx, index)?;

            // Read next pointer before CAS
            let next = meta.next_free.load(Ordering::Acquire);
            let next_packed = if next == u32::MAX {
                FREE_LIST_END
            } else {
                SizeClassHeader::pack(next, tag.wrapping_add(1))
            };

            // Try to pop from free list
            match free_head.compare_exchange_weak(
                head,
                next_packed,
                Ordering::AcqRel,
                Ordering::Acquire,
            ) {
                Ok(_) => {
                    // Success! Initialize slot metadata
                    let new_gen = meta
                        .generation
                        .fetch_add(1, Ordering::AcqRel)
                        .wrapping_add(1);
                    meta.state
                        .store(SlotState::Allocated as u32, Ordering::Release);
                    meta.owner_peer.store(owner as u32, Ordering::Release);

                    return Some(VarSlotHandle {
                        class_idx: class_idx as u8,
                        extent_idx: extent_idx as u8,
                        slot_idx: index,
                        generation: new_gen,
                    });
                }
                Err(_) => continue, // Retry
            }
        }
    }

    /// Mark a slot as in-flight (after enqueue).
    pub fn mark_in_flight(&self, handle: VarSlotHandle) -> Result<(), VarFreeError> {
        if handle.class_idx as usize >= self.classes.len() {
            return Err(VarFreeError::InvalidClass);
        }
        let class = &self.classes[handle.class_idx as usize];
        if handle.slot_idx >= class.count {
            return Err(VarFreeError::InvalidIndex);
        }

        let meta = self
            .slot_meta_ext(
                handle.class_idx as usize,
                handle.extent_idx as usize,
                handle.slot_idx,
            )
            .ok_or(VarFreeError::InvalidIndex)?;

        // Verify generation
        let actual_gen = meta.generation.load(Ordering::Acquire);
        if actual_gen != handle.generation {
            return Err(VarFreeError::GenerationMismatch {
                expected: handle.generation,
                actual: actual_gen,
            });
        }

        // Transition Allocated -> InFlight
        match meta.state.compare_exchange(
            SlotState::Allocated as u32,
            SlotState::InFlight as u32,
            Ordering::AcqRel,
            Ordering::Acquire,
        ) {
            Ok(_) => Ok(()),
            Err(actual) => Err(VarFreeError::InvalidState {
                expected: SlotState::Allocated,
                actual: SlotState::from_u32(actual).unwrap_or(SlotState::Free),
            }),
        }
    }

    /// Free an in-flight slot back to its pool.
    ///
    /// shm[impl shm.varslot.freeing]
    pub fn free(&self, handle: VarSlotHandle) -> Result<(), VarFreeError> {
        if handle.class_idx as usize >= self.classes.len() {
            return Err(VarFreeError::InvalidClass);
        }
        let class = &self.classes[handle.class_idx as usize];
        if handle.slot_idx >= class.count {
            return Err(VarFreeError::InvalidIndex);
        }

        let meta = self
            .slot_meta_ext(
                handle.class_idx as usize,
                handle.extent_idx as usize,
                handle.slot_idx,
            )
            .ok_or(VarFreeError::InvalidIndex)?;

        // Verify generation (detect double-free)
        let actual_gen = meta.generation.load(Ordering::Acquire);
        if actual_gen != handle.generation {
            return Err(VarFreeError::GenerationMismatch {
                expected: handle.generation,
                actual: actual_gen,
            });
        }

        // Transition InFlight -> Free
        match meta.state.compare_exchange(
            SlotState::InFlight as u32,
            SlotState::Free as u32,
            Ordering::AcqRel,
            Ordering::Acquire,
        ) {
            Ok(_) => {}
            Err(actual) => {
                return Err(VarFreeError::InvalidState {
                    expected: SlotState::InFlight,
                    actual: SlotState::from_u32(actual).unwrap_or(SlotState::Free),
                });
            }
        }

        // Push to free list for this extent
        self.push_to_free_list(
            handle.class_idx as usize,
            handle.extent_idx as usize,
            handle.slot_idx,
        );
        Ok(())
    }

    /// Free an allocated (never sent) slot back to its pool.
    pub fn free_allocated(&self, handle: VarSlotHandle) -> Result<(), VarFreeError> {
        if handle.class_idx as usize >= self.classes.len() {
            return Err(VarFreeError::InvalidClass);
        }
        let class = &self.classes[handle.class_idx as usize];
        if handle.slot_idx >= class.count {
            return Err(VarFreeError::InvalidIndex);
        }

        let meta = self
            .slot_meta_ext(
                handle.class_idx as usize,
                handle.extent_idx as usize,
                handle.slot_idx,
            )
            .ok_or(VarFreeError::InvalidIndex)?;

        // Verify generation
        let actual_gen = meta.generation.load(Ordering::Acquire);
        if actual_gen != handle.generation {
            return Err(VarFreeError::GenerationMismatch {
                expected: handle.generation,
                actual: actual_gen,
            });
        }

        // Transition Allocated -> Free
        match meta.state.compare_exchange(
            SlotState::Allocated as u32,
            SlotState::Free as u32,
            Ordering::AcqRel,
            Ordering::Acquire,
        ) {
            Ok(_) => {}
            Err(actual) => {
                return Err(VarFreeError::InvalidState {
                    expected: SlotState::Allocated,
                    actual: SlotState::from_u32(actual).unwrap_or(SlotState::Free),
                });
            }
        }

        // Push to free list for this extent
        self.push_to_free_list(
            handle.class_idx as usize,
            handle.extent_idx as usize,
            handle.slot_idx,
        );
        Ok(())
    }

    fn push_to_free_list(&self, class_idx: usize, extent_idx: usize, slot_idx: u32) {
        let header = self.class_header(class_idx);
        let free_head = header.free_head(extent_idx);
        let Some(meta) = self.slot_meta_ext(class_idx, extent_idx, slot_idx) else {
            return; // Invalid extent/slot
        };

        loop {
            let head = free_head.load(Ordering::Acquire);
            let (head_idx, head_gen) = if head == FREE_LIST_END {
                (u32::MAX, 0u32)
            } else {
                SizeClassHeader::unpack(head)
            };

            // Set our next pointer
            meta.next_free.store(head_idx, Ordering::Release);

            // Try to become new head
            let new_head = SizeClassHeader::pack(slot_idx, head_gen.wrapping_add(1));

            match free_head.compare_exchange_weak(
                head,
                new_head,
                Ordering::AcqRel,
                Ordering::Acquire,
            ) {
                Ok(_) => return,
                Err(_) => continue, // Retry
            }
        }
    }

    /// Recover all slots owned by a crashed peer.
    ///
    /// This scans all extents in all size classes and frees any slots
    /// that were owned by the specified peer.
    /// Should be called when a peer crashes or disconnects unexpectedly.
    pub fn recover_peer(&self, peer_id: u8) {
        for class_idx in 0..self.classes.len() {
            let header = self.class_header(class_idx);
            let extent_count = header.extent_count() as usize;
            let class = &self.classes[class_idx];

            for extent_idx in 0..extent_count {
                for slot_idx in 0..class.count {
                    let Some(meta) = self.slot_meta_ext(class_idx, extent_idx, slot_idx) else {
                        continue;
                    };

                    let owner = meta.owner_peer.load(Ordering::Acquire);
                    let state = meta.state.load(Ordering::Acquire);

                    if owner == peer_id as u32 && state != SlotState::Free as u32 {
                        // Force transition to Free
                        meta.state.store(SlotState::Free as u32, Ordering::Release);

                        // Push to free list for this extent
                        self.push_to_free_list(class_idx, extent_idx, slot_idx);
                    }
                }
            }
        }
    }

    /// Get the current state of a slot.
    ///
    /// Returns `None` if the handle is invalid.
    pub fn slot_state(&self, handle: &VarSlotHandle) -> Option<SlotState> {
        let meta = self.slot_meta_ext(
            handle.class_idx as usize,
            handle.extent_idx as usize,
            handle.slot_idx,
        )?;
        Some(meta.state())
    }

    /// Check if a slot has been freed (state is Free or generation has advanced).
    pub fn is_slot_free(&self, handle: &VarSlotHandle) -> bool {
        let Some(meta) = self.slot_meta_ext(
            handle.class_idx as usize,
            handle.extent_idx as usize,
            handle.slot_idx,
        ) else {
            return true; // Invalid handle, treat as freed
        };
        meta.state() == SlotState::Free || meta.generation() != handle.generation
    }

    /// Get the number of size classes.
    pub fn class_count(&self) -> usize {
        self.classes.len()
    }

    /// Get the size classes.
    pub fn classes(&self) -> &[SizeClass] {
        &self.classes
    }

    /// Get the number of extents for a size class (for diagnostics).
    pub(crate) fn extent_count(&self, class_idx: usize) -> u32 {
        if class_idx >= self.classes.len() {
            return 0;
        }
        self.class_header(class_idx).extent_count()
    }

    /// Approximate count of free slots in a class (across all extents).
    pub fn free_count_approx(&self, class_idx: usize) -> u32 {
        if class_idx >= self.classes.len() {
            return 0;
        }

        let header = self.class_header(class_idx);
        let extent_count = header.extent_count() as usize;
        let slot_count = self.classes[class_idx].count;
        let mut total_count = 0u32;

        for extent_idx in 0..extent_count {
            let free_head = header.free_head(extent_idx);
            let mut count = 0u32;
            let mut current = free_head.load(Ordering::Acquire);

            while current != FREE_LIST_END && count < slot_count {
                let (index, _) = SizeClassHeader::unpack(current);
                if index < slot_count {
                    count += 1;
                    if let Some(meta) = self.slot_meta_ext(class_idx, extent_idx, index) {
                        let next = meta.next_free.load(Ordering::Acquire);
                        current = if next == u32::MAX {
                            FREE_LIST_END
                        } else {
                            SizeClassHeader::pack(next, 0)
                        };
                    } else {
                        break;
                    }
                } else {
                    break;
                }
            }
            total_count += count;
        }

        total_count
    }
}

#[inline]
const fn align_up(value: u64, align: u64) -> u64 {
    (value + (align - 1)) & !(align - 1)
}

#[cfg(test)]
mod tests {
    use super::*;
    use shm_primitives::HeapRegion;

    fn create_test_pool() -> (HeapRegion, VarSlotPool) {
        let classes = vec![
            SizeClass::new(64, 16),  // Small: 64 bytes × 16 slots
            SizeClass::new(256, 8),  // Medium: 256 bytes × 8 slots
            SizeClass::new(1024, 4), // Large: 1 KB × 4 slots
        ];

        let size = VarSlotPool::calculate_size(&classes);
        let region = HeapRegion::new_zeroed(size as usize);
        let mut pool = VarSlotPool::new(region.region(), 0, classes);

        unsafe { pool.init() };

        (region, pool)
    }

    #[test]
    fn test_size_class_header_pack_unpack() {
        let packed = SizeClassHeader::pack(42, 123);
        let (index, generation) = SizeClassHeader::unpack(packed);
        assert_eq!(index, 42);
        assert_eq!(generation, 123);

        let packed_max = SizeClassHeader::pack(u32::MAX, u32::MAX);
        let (index_max, generation_max) = SizeClassHeader::unpack(packed_max);
        assert_eq!(index_max, u32::MAX);
        assert_eq!(generation_max, u32::MAX);
    }

    #[test]
    fn test_var_slot_handle_pack() {
        let handle = VarSlotHandle {
            class_idx: 2,
            extent_idx: 1,
            slot_idx: 0x00123456,
            generation: 42,
        };
        let packed = handle.pack_slot();
        // class_idx=2 in bits 31-24, extent_idx=1 in bits 23-22, slot_idx in bits 21-0
        // 0x02 << 24 = 0x02000000
        // 0x01 << 22 = 0x00400000
        // 0x00123456 & 0x003FFFFF = 0x00123456
        // Result: 0x02400000 | 0x00123456 = 0x02523456
        assert_eq!(packed, 0x02523456);

        let unpacked = VarSlotHandle::from_packed(packed, 42);
        assert_eq!(unpacked.class_idx, 2);
        assert_eq!(unpacked.extent_idx, 1);
        assert_eq!(unpacked.slot_idx, 0x00123456);
        assert_eq!(unpacked.generation, 42);
    }

    #[test]
    fn test_alloc_smallest_fit() {
        let (_region, pool) = create_test_pool();

        // Small payload uses small class
        let small = pool.alloc(32, 0).unwrap();
        assert_eq!(small.class_idx, 0); // 64-byte class

        // Medium payload uses medium class
        let medium = pool.alloc(100, 0).unwrap();
        assert_eq!(medium.class_idx, 1); // 256-byte class

        // Large payload uses large class
        let large = pool.alloc(500, 0).unwrap();
        assert_eq!(large.class_idx, 2); // 1024-byte class
    }

    #[test]
    fn test_alloc_from_specific_class() {
        let (_region, pool) = create_test_pool();

        let handle = pool.alloc_from_class(1, 0).unwrap();
        assert_eq!(handle.class_idx, 1);
    }

    #[test]
    fn test_exhaustion_fallback() {
        let (_region, pool) = create_test_pool();

        // Exhaust small class (16 slots)
        let mut handles = Vec::new();
        for _ in 0..16 {
            handles.push(pool.alloc_from_class(0, 0).unwrap());
        }

        // Next small alloc should fail for this class
        assert!(pool.alloc_from_class(0, 0).is_none());

        // But alloc with size check should fall back to medium class
        let fallback = pool.alloc(32, 0).unwrap();
        assert_eq!(fallback.class_idx, 1); // Fell back to 256-byte class
    }

    #[test]
    fn test_free_and_realloc() {
        let (_region, pool) = create_test_pool();

        let handle1 = pool.alloc(32, 0).unwrap();
        pool.mark_in_flight(handle1).unwrap();
        pool.free(handle1).unwrap();

        // Should be able to allocate again
        let handle2 = pool.alloc(32, 0).unwrap();
        assert_eq!(handle2.class_idx, 0);
        // Generation should have increased
        assert!(handle2.generation > handle1.generation || handle2.slot_idx != handle1.slot_idx);
    }

    #[test]
    fn test_double_free_detected() {
        let (_region, pool) = create_test_pool();

        let handle = pool.alloc(32, 0).unwrap();
        pool.mark_in_flight(handle).unwrap();
        pool.free(handle).unwrap();

        // Second free should fail - the slot is now Free, not InFlight
        let result = pool.free(handle);
        assert!(matches!(
            result,
            Err(VarFreeError::InvalidState {
                expected: SlotState::InFlight,
                actual: SlotState::Free,
            })
        ));
    }

    #[test]
    fn test_owner_tracking() {
        let (_region, pool) = create_test_pool();

        let handle1 = pool.alloc(32, 1).unwrap(); // Owner: peer 1
        let handle2 = pool.alloc(32, 2).unwrap(); // Owner: peer 2

        let meta1 = pool
            .slot_meta_ext(
                handle1.class_idx as usize,
                handle1.extent_idx as usize,
                handle1.slot_idx,
            )
            .expect("meta1 should exist");
        let meta2 = pool
            .slot_meta_ext(
                handle2.class_idx as usize,
                handle2.extent_idx as usize,
                handle2.slot_idx,
            )
            .expect("meta2 should exist");

        assert_eq!(meta1.owner(), 1);
        assert_eq!(meta2.owner(), 2);
    }

    #[test]
    fn test_peer_recovery() {
        let (_region, pool) = create_test_pool();

        // Allocate slots for different peers
        let h1 = pool.alloc(32, 1).unwrap();
        let h2 = pool.alloc(32, 1).unwrap();
        let h3 = pool.alloc(32, 2).unwrap();

        // Mark as in-flight (simulating sent messages)
        pool.mark_in_flight(h1).unwrap();
        pool.mark_in_flight(h2).unwrap();
        pool.mark_in_flight(h3).unwrap();

        // Count free slots before recovery
        let free_before = pool.free_count_approx(0);

        // Recover peer 1's slots
        pool.recover_peer(1);

        // Peer 1's slots should now be free
        let free_after = pool.free_count_approx(0);
        assert_eq!(free_after, free_before + 2);

        // Peer 2's slot should still be in-flight (not recovered)
        let meta3 = pool
            .slot_meta_ext(h3.class_idx as usize, h3.extent_idx as usize, h3.slot_idx)
            .expect("meta3 should exist");
        assert_eq!(meta3.state(), SlotState::InFlight);
        assert_eq!(meta3.owner(), 2);
    }

    #[test]
    fn test_payload_ptr() {
        let (_region, pool) = create_test_pool();

        let handle = pool.alloc(32, 0).unwrap();
        let ptr = pool.payload_ptr(handle);
        assert!(ptr.is_some());

        // Invalid handles should return None
        let invalid = VarSlotHandle {
            class_idx: 255,
            extent_idx: 0,
            slot_idx: 0,
            generation: 0,
        };
        assert!(pool.payload_ptr(invalid).is_none());
    }
}
