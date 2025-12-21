use core::mem::{align_of, size_of};

use crate::region::Region;
use crate::slot::{SlotMeta, SlotState};
use crate::sync::{AtomicU32, AtomicU64, Ordering, spin_loop};

/// Sentinel value indicating end of free list.
pub const FREE_LIST_END: u32 = u32::MAX;

/// Slab header (64 bytes, cache-line aligned).
#[repr(C, align(64))]
pub struct TreiberSlabHeader {
    pub slot_size: u32,
    pub slot_count: u32,
    pub max_frame_size: u32,
    _pad: u32,

    /// Free list head: index (low 32 bits) + tag (high 32 bits).
    pub free_head: AtomicU64,

    /// Slot-availability futex word (unused by this crate, but reserved for parity).
    pub slot_available: AtomicU32,

    _pad2: [u8; 36],
}

#[cfg(not(loom))]
const _: () = assert!(core::mem::size_of::<TreiberSlabHeader>() == 64);

impl TreiberSlabHeader {
    pub fn init(&mut self, slot_size: u32, slot_count: u32) {
        self.slot_size = slot_size;
        self.slot_count = slot_count;
        self.max_frame_size = slot_size;
        self._pad = 0;
        self.free_head = AtomicU64::new(pack_free_head(FREE_LIST_END, 0));
        self.slot_available = AtomicU32::new(0);
        self._pad2 = [0; 36];
    }
}

/// Handle to an allocated slot.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SlotHandle {
    pub index: u32,
    pub generation: u32,
}

/// Result of an allocation attempt.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AllocResult {
    Ok(SlotHandle),
    WouldBlock,
}

/// Errors returned by slot transitions.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SlotError {
    InvalidIndex,
    GenerationMismatch {
        expected: u32,
        actual: u32,
    },
    InvalidState {
        expected: SlotState,
        actual: SlotState,
    },
}

pub type FreeError = SlotError;

/// A lock-free slab allocator backed by a region.
///
/// This is a convenience wrapper around `TreiberSlabRaw` that manages
/// memory through a `Region`. All operations delegate to the raw implementation.
pub struct TreiberSlab {
    /// We hold the region to keep the backing memory alive.
    /// All operations go through `inner` which holds raw pointers into this region.
    #[allow(dead_code)]
    region: Region,
    inner: TreiberSlabRaw,
}

unsafe impl Send for TreiberSlab {}
unsafe impl Sync for TreiberSlab {}

impl TreiberSlab {
    /// Initialize a new slab at `header_offset` in the region.
    ///
    /// # Safety
    ///
    /// The region must be writable and exclusively owned during initialization.
    pub unsafe fn init(
        region: Region,
        header_offset: usize,
        slot_count: u32,
        slot_size: u32,
    ) -> Self {
        assert!(slot_count > 0, "slot_count must be > 0");
        assert!(
            slot_size >= size_of::<u32>() as u32,
            "slot_size must be >= 4"
        );
        assert!(
            header_offset.is_multiple_of(64),
            "header_offset must be 64-byte aligned"
        );

        let meta_offset = align_up(
            header_offset + size_of::<TreiberSlabHeader>(),
            align_of::<SlotMeta>(),
        );
        let data_offset = align_up(
            meta_offset + (slot_count as usize * size_of::<SlotMeta>()),
            align_of::<u32>(),
        );
        let required = data_offset + (slot_count as usize * slot_size as usize);
        assert!(required <= region.len(), "region too small for slab");

        // Get raw pointers
        let header_ptr = region.offset(header_offset) as *mut TreiberSlabHeader;
        let slot_meta_ptr = region.offset(meta_offset) as *mut SlotMeta;
        let slot_data_ptr = region.offset(data_offset);

        // Initialize header
        unsafe { (*header_ptr).init(slot_size, slot_count) };

        // Initialize slot metadata
        for i in 0..slot_count {
            let meta = unsafe { &mut *slot_meta_ptr.add(i as usize) };
            meta.init();
        }

        // Create inner raw slab
        let inner = unsafe { TreiberSlabRaw::from_raw(header_ptr, slot_meta_ptr, slot_data_ptr) };

        // Initialize free list by linking all slots together
        unsafe { inner.init_free_list() };

        Self { region, inner }
    }

    /// Attach to an existing slab.
    ///
    /// # Safety
    ///
    /// The region must contain a valid, initialized slab header at `header_offset`.
    pub unsafe fn attach(region: Region, header_offset: usize) -> Result<Self, &'static str> {
        assert!(
            header_offset.is_multiple_of(64),
            "header_offset must be 64-byte aligned"
        );

        let header_ptr = region.offset(header_offset) as *mut TreiberSlabHeader;
        let header = unsafe { &*header_ptr };

        if header.slot_count == 0 {
            return Err("slot_count must be > 0");
        }
        if header.slot_size < size_of::<u32>() as u32 {
            return Err("slot_size must be >= 4");
        }

        let meta_offset = align_up(
            header_offset + size_of::<TreiberSlabHeader>(),
            align_of::<SlotMeta>(),
        );
        let data_offset = align_up(
            meta_offset + (header.slot_count as usize * size_of::<SlotMeta>()),
            align_of::<u32>(),
        );
        let required = data_offset + (header.slot_count as usize * header.slot_size as usize);
        if required > region.len() {
            return Err("region too small for slab");
        }

        let slot_meta_ptr = region.offset(meta_offset) as *mut SlotMeta;
        let slot_data_ptr = region.offset(data_offset);

        let inner = unsafe { TreiberSlabRaw::from_raw(header_ptr, slot_meta_ptr, slot_data_ptr) };

        Ok(Self { region, inner })
    }

    /// Get a reference to the inner raw slab.
    #[inline]
    pub fn inner(&self) -> &TreiberSlabRaw {
        &self.inner
    }

    /// Try to allocate a slot.
    ///
    /// Delegates to `TreiberSlabRaw::try_alloc`.
    pub fn try_alloc(&self) -> AllocResult {
        self.inner.try_alloc()
    }

    /// Mark a slot as in-flight (after enqueue).
    ///
    /// Delegates to `TreiberSlabRaw::mark_in_flight`.
    pub fn mark_in_flight(&self, handle: SlotHandle) -> Result<(), SlotError> {
        self.inner.mark_in_flight(handle)
    }

    /// Free an in-flight slot and push it to the free list.
    ///
    /// Delegates to `TreiberSlabRaw::free`.
    pub fn free(&self, handle: SlotHandle) -> Result<(), SlotError> {
        self.inner.free(handle)
    }

    /// Free a slot that is still Allocated (never sent).
    ///
    /// Delegates to `TreiberSlabRaw::free_allocated`.
    pub fn free_allocated(&self, handle: SlotHandle) -> Result<(), SlotError> {
        self.inner.free_allocated(handle)
    }

    /// Return a pointer to the slot data.
    ///
    /// # Safety
    ///
    /// The handle must be valid and the slot must be allocated.
    pub unsafe fn slot_data_ptr(&self, handle: SlotHandle) -> *mut u8 {
        unsafe { self.inner.slot_data_ptr(handle) }
    }

    /// Returns the slot size in bytes.
    #[inline]
    pub fn slot_size(&self) -> u32 {
        self.inner.slot_size()
    }

    /// Returns the total number of slots.
    #[inline]
    pub fn slot_count(&self) -> u32 {
        self.inner.slot_count()
    }

    /// Approximate number of free slots.
    pub fn free_count_approx(&self) -> u32 {
        self.inner.free_count_approx()
    }
}

#[inline]
fn pack_free_head(index: u32, tag: u32) -> u64 {
    ((tag as u64) << 32) | (index as u64)
}

#[inline]
fn unpack_free_head(packed: u64) -> (u32, u32) {
    let index = packed as u32;
    let tag = (packed >> 32) as u32;
    (index, tag)
}

#[inline]
const fn align_up(value: usize, align: usize) -> usize {
    (value + (align - 1)) & !(align - 1)
}

// =============================================================================
// TreiberSlabRaw - Raw pointer version for rapace-core compatibility
// =============================================================================

/// A lock-free slab allocator operating on raw pointers.
///
/// This is the "raw" API that matches rapace-core's `DataSegment` interface:
/// - Constructed from raw pointers to header, slot metadata, and slot data
/// - Does not own the memory or manage offsets
///
/// Use this when you need to integrate with existing SHM layouts or when
/// the `Region` abstraction doesn't fit your use case.
pub struct TreiberSlabRaw {
    header: *mut TreiberSlabHeader,
    slot_meta: *mut SlotMeta,
    slot_data: *mut u8,
}

unsafe impl Send for TreiberSlabRaw {}
unsafe impl Sync for TreiberSlabRaw {}

impl TreiberSlabRaw {
    /// Create a slab view from raw pointers.
    ///
    /// # Safety
    ///
    /// - `header` must point to a valid, initialized `TreiberSlabHeader`
    /// - `slot_meta` must point to `header.slot_count` initialized `SlotMeta` entries
    /// - `slot_data` must point to `header.slot_count * header.slot_size` bytes
    /// - The memory must remain valid for the lifetime of this slab
    /// - All pointers must be properly aligned
    #[inline]
    pub unsafe fn from_raw(
        header: *mut TreiberSlabHeader,
        slot_meta: *mut SlotMeta,
        slot_data: *mut u8,
    ) -> Self {
        Self {
            header,
            slot_meta,
            slot_data,
        }
    }

    #[inline]
    fn header(&self) -> &TreiberSlabHeader {
        unsafe { &*self.header }
    }

    #[inline]
    unsafe fn meta(&self, index: u32) -> &SlotMeta {
        unsafe { &*self.slot_meta.add(index as usize) }
    }

    #[inline]
    unsafe fn data_ptr(&self, index: u32) -> *mut u8 {
        let slot_size = self.header().slot_size as usize;
        unsafe { self.slot_data.add(index as usize * slot_size) }
    }

    #[inline]
    unsafe fn read_next_free(&self, index: u32) -> u32 {
        let ptr = unsafe { self.data_ptr(index) as *const u32 };
        unsafe { core::ptr::read_volatile(ptr) }
    }

    #[inline]
    unsafe fn write_next_free(&self, index: u32, next: u32) {
        let ptr = unsafe { self.data_ptr(index) as *mut u32 };
        unsafe { core::ptr::write_volatile(ptr, next) };
    }

    /// Initialize the free list by linking all slots together.
    ///
    /// # Safety
    ///
    /// Must only be called during initialization, before any concurrent access.
    pub unsafe fn init_free_list(&self) {
        let slot_count = self.header().slot_count;
        if slot_count == 0 {
            return;
        }

        for i in 0..slot_count - 1 {
            unsafe { self.write_next_free(i, i + 1) };
        }
        unsafe { self.write_next_free(slot_count - 1, FREE_LIST_END) };

        let header = unsafe { &mut *self.header };
        header
            .free_head
            .store(pack_free_head(0, 0), Ordering::Release);
    }

    /// Try to allocate a slot.
    pub fn try_alloc(&self) -> AllocResult {
        let header = self.header();

        loop {
            let old_head = header.free_head.load(Ordering::Acquire);
            let (index, tag) = unpack_free_head(old_head);

            if index == FREE_LIST_END {
                return AllocResult::WouldBlock;
            }

            let next = unsafe { self.read_next_free(index) };
            let new_head = pack_free_head(next, tag.wrapping_add(1));

            match header.free_head.compare_exchange_weak(
                old_head,
                new_head,
                Ordering::AcqRel,
                Ordering::Acquire,
            ) {
                Ok(_) => {
                    let meta = unsafe { self.meta(index) };
                    let result = meta.state.compare_exchange(
                        SlotState::Free as u32,
                        SlotState::Allocated as u32,
                        Ordering::AcqRel,
                        Ordering::Acquire,
                    );

                    if result.is_err() {
                        // The slot was popped from the free list but its state wasn't Free.
                        // This should never happen - it indicates an invariant violation.
                        // Only push it back if it's actually in Free state to avoid corrupting
                        // data that another thread may be using.
                        let current_state = meta.state.load(Ordering::Acquire);
                        if current_state == SlotState::Free as u32 {
                            self.push_to_free_list(index);
                        }
                        // If not Free, the slot is leaked - but that's better than data corruption.
                        // In debug builds, this would indicate a serious bug.
                        debug_assert_eq!(
                            current_state,
                            SlotState::Free as u32,
                            "slot popped from free list had unexpected state"
                        );
                        spin_loop();
                        continue;
                    }

                    let generation = meta.generation.fetch_add(1, Ordering::AcqRel) + 1;
                    return AllocResult::Ok(SlotHandle { index, generation });
                }
                Err(_) => {
                    spin_loop();
                    continue;
                }
            }
        }
    }

    /// Mark a slot as in-flight (after enqueue).
    pub fn mark_in_flight(&self, handle: SlotHandle) -> Result<(), SlotError> {
        if handle.index >= self.header().slot_count {
            return Err(SlotError::InvalidIndex);
        }

        let meta = unsafe { self.meta(handle.index) };
        let actual = meta.generation.load(Ordering::Acquire);
        if actual != handle.generation {
            return Err(SlotError::GenerationMismatch {
                expected: handle.generation,
                actual,
            });
        }

        let result = meta.state.compare_exchange(
            SlotState::Allocated as u32,
            SlotState::InFlight as u32,
            Ordering::AcqRel,
            Ordering::Acquire,
        );

        result
            .map(|_| ())
            .map_err(|actual| SlotError::InvalidState {
                expected: SlotState::Allocated,
                actual: SlotState::from_u32(actual).unwrap_or(SlotState::Free),
            })
    }

    /// Free an in-flight slot and push it to the free list.
    pub fn free(&self, handle: SlotHandle) -> Result<(), SlotError> {
        if handle.index >= self.header().slot_count {
            return Err(SlotError::InvalidIndex);
        }

        let meta = unsafe { self.meta(handle.index) };
        let actual = meta.generation.load(Ordering::Acquire);
        if actual != handle.generation {
            return Err(SlotError::GenerationMismatch {
                expected: handle.generation,
                actual,
            });
        }

        let result = meta.state.compare_exchange(
            SlotState::InFlight as u32,
            SlotState::Free as u32,
            Ordering::AcqRel,
            Ordering::Acquire,
        );

        if result.is_ok() {
            self.push_to_free_list(handle.index);
            Ok(())
        } else {
            Err(SlotError::InvalidState {
                expected: SlotState::InFlight,
                actual: SlotState::from_u32(result.err().unwrap()).unwrap_or(SlotState::Free),
            })
        }
    }

    /// Free a slot that is still Allocated (never sent).
    pub fn free_allocated(&self, handle: SlotHandle) -> Result<(), SlotError> {
        if handle.index >= self.header().slot_count {
            return Err(SlotError::InvalidIndex);
        }

        let meta = unsafe { self.meta(handle.index) };
        let actual = meta.generation.load(Ordering::Acquire);
        if actual != handle.generation {
            return Err(SlotError::GenerationMismatch {
                expected: handle.generation,
                actual,
            });
        }

        let result = meta.state.compare_exchange(
            SlotState::Allocated as u32,
            SlotState::Free as u32,
            Ordering::AcqRel,
            Ordering::Acquire,
        );

        if result.is_ok() {
            self.push_to_free_list(handle.index);
            Ok(())
        } else {
            Err(SlotError::InvalidState {
                expected: SlotState::Allocated,
                actual: SlotState::from_u32(result.err().unwrap()).unwrap_or(SlotState::Free),
            })
        }
    }

    /// Return a pointer to the slot data.
    ///
    /// # Safety
    ///
    /// The handle must be valid and the slot must be allocated.
    #[inline]
    pub unsafe fn slot_data_ptr(&self, handle: SlotHandle) -> *mut u8 {
        unsafe { self.data_ptr(handle.index) }
    }

    /// Returns the slot size in bytes.
    #[inline]
    pub fn slot_size(&self) -> u32 {
        self.header().slot_size
    }

    /// Returns the total number of slots.
    #[inline]
    pub fn slot_count(&self) -> u32 {
        self.header().slot_count
    }

    /// Approximate number of free slots.
    pub fn free_count_approx(&self) -> u32 {
        let slot_count = self.header().slot_count;
        let mut free_list_len = 0u32;
        let mut current = {
            let (index, _tag) = unpack_free_head(self.header().free_head.load(Ordering::Acquire));
            index
        };

        while current != FREE_LIST_END && free_list_len < slot_count {
            free_list_len += 1;
            if current < slot_count {
                current = unsafe { self.read_next_free(current) };
            } else {
                break;
            }
        }

        free_list_len
    }

    fn push_to_free_list(&self, index: u32) {
        let header = self.header();

        loop {
            let old_head = header.free_head.load(Ordering::Acquire);
            let (old_index, tag) = unpack_free_head(old_head);

            unsafe { self.write_next_free(index, old_index) };

            let new_head = pack_free_head(index, tag.wrapping_add(1));

            if header
                .free_head
                .compare_exchange_weak(old_head, new_head, Ordering::AcqRel, Ordering::Acquire)
                .is_ok()
            {
                return;
            }
        }
    }
}
