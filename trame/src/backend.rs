//! Backend abstraction for memory operations.
//!
//! This module defines a trait that abstracts over the actual unsafe memory
//! operations (alloc, dealloc, init, drop). By implementing different backends,
//! we can:
//!
//! - **Real backend**: Actually perform the operations (production code)
//! - **Verified backend**: Track state and assert valid transitions (Kani proofs)
//!
//! The goal is ONE implementation of business logic that runs against either backend.

use std::alloc::Layout;

/// Backend for memory operations.
///
/// The associated types allow different representations:
/// - Real backend: pointers
/// - Verified backend: indices into tracking tables
///
/// # Safety
///
/// All methods are unsafe because they have preconditions that cannot be
/// checked at compile time. The caller must uphold the documented safety
/// requirements for each method.
pub trait Backend {
    /// Handle to an allocation (a contiguous region of memory).
    /// Real: `*mut u8`, Verified: index
    type Alloc: Copy;

    /// Handle to a slot (a single initializable cell within an allocation).
    /// Real: `*mut u8`, Verified: index
    type Slot: Copy;

    /// Allocate memory with the given layout.
    /// Transition: (nothing) -> Allocated
    ///
    /// # Safety
    ///
    /// - `layout` must have non-zero size, or caller must handle ZST case
    /// - Caller must eventually call `dealloc` with the same layout
    unsafe fn alloc(&mut self, layout: Layout) -> Self::Alloc;

    /// Deallocate memory.
    /// Transition: Allocated -> (nothing)
    ///
    /// # Safety
    ///
    /// - `alloc` must have been returned by a previous call to `alloc`
    /// - `layout` must be the same layout used in that `alloc` call
    /// - All slots in this allocation must be uninitialized (dropped)
    /// - `alloc` must not be used after this call
    unsafe fn dealloc(&mut self, alloc: Self::Alloc, layout: Layout);

    /// Get a slot handle for a location within an allocation.
    ///
    /// # Safety
    ///
    /// - `alloc` must be a live allocation
    /// - `offset` must be within the allocation's bounds
    unsafe fn slot(&mut self, alloc: Self::Alloc, offset: usize) -> Self::Slot;

    /// Mark a slot as initialized.
    /// Transition: Allocated -> Initialized
    ///
    /// # Safety
    ///
    /// - `slot` must be a valid slot handle
    /// - The memory at this slot must have actually been initialized
    /// - Slot must be in Allocated state (not already initialized)
    unsafe fn mark_init(&mut self, slot: Self::Slot);

    /// Mark a slot as uninitialized (after drop_in_place).
    /// Transition: Initialized -> Allocated
    ///
    /// # Safety
    ///
    /// - `slot` must be a valid slot handle
    /// - `drop_in_place` must have been called on the value at this slot
    /// - Slot must be in Initialized state
    unsafe fn mark_uninit(&mut self, slot: Self::Slot);

    /// Check if a slot is initialized.
    ///
    /// # Safety
    ///
    /// - `slot` must be a valid slot handle
    unsafe fn is_init(&self, slot: Self::Slot) -> bool;
}

/// Real backend that performs actual memory operations.
///
/// This is a zero-cost abstraction - all methods are no-ops or direct
/// pass-throughs, and should inline away completely.
pub struct RealBackend;

impl Backend for RealBackend {
    type Alloc = *mut u8;
    type Slot = *mut u8;

    #[inline]
    unsafe fn alloc(&mut self, layout: Layout) -> Self::Alloc {
        if layout.size() == 0 {
            // ZST: return dangling pointer with correct alignment
            layout.align() as *mut u8
        } else {
            // SAFETY: caller guarantees layout has non-zero size
            std::alloc::alloc(layout)
        }
    }

    #[inline]
    unsafe fn dealloc(&mut self, alloc: Self::Alloc, layout: Layout) {
        if layout.size() > 0 {
            // SAFETY: caller guarantees alloc was allocated with this layout
            std::alloc::dealloc(alloc, layout)
        }
    }

    #[inline]
    unsafe fn slot(&mut self, alloc: Self::Alloc, offset: usize) -> Self::Slot {
        // SAFETY: caller guarantees offset is within allocation
        alloc.add(offset)
    }

    #[inline]
    unsafe fn mark_init(&mut self, _slot: Self::Slot) {
        // No-op: we trust the caller in production
    }

    #[inline]
    unsafe fn mark_uninit(&mut self, _slot: Self::Slot) {
        // No-op: we trust the caller in production
    }

    #[inline]
    unsafe fn is_init(&self, _slot: Self::Slot) -> bool {
        // We don't track this in production - this method shouldn't
        // be called in contexts where we don't already know the answer
        true
    }
}

/// Verified backend that tracks state for Kani proofs.
///
/// Instead of actual memory operations, this tracks the state of each
/// slot and asserts that all transitions are valid.
#[cfg(any(kani, test))]
pub struct VerifiedBackend {
    /// State of each slot: None = unallocated, Some(false) = allocated, Some(true) = initialized
    slots: Vec<Option<bool>>,
    /// Which slots belong to which allocation: (start_slot, slot_count)
    allocs: Vec<Option<(u32, u32)>>,
}

#[cfg(any(kani, test))]
impl VerifiedBackend {
    pub fn new() -> Self {
        Self {
            slots: Vec::new(),
            allocs: Vec::new(),
        }
    }

    /// Check invariants - all initialized slots must belong to live allocations.
    #[allow(dead_code)]
    pub fn check_invariants(&self) {
        for (slot_idx, slot_state) in self.slots.iter().enumerate() {
            if let Some(initialized) = slot_state {
                // Slot is allocated - verify it belongs to a live allocation
                let belongs_to_alloc = self.allocs.iter().any(|a| {
                    if let Some((start, count)) = a {
                        let start = *start as usize;
                        let end = start + *count as usize;
                        slot_idx >= start && slot_idx < end
                    } else {
                        false
                    }
                });
                assert!(
                    belongs_to_alloc,
                    "slot {} is {:?} but doesn't belong to any allocation",
                    slot_idx,
                    if *initialized {
                        "initialized"
                    } else {
                        "allocated"
                    }
                );
            }
        }
    }
}

#[cfg(any(kani, test))]
impl Default for VerifiedBackend {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(any(kani, test))]
impl Backend for VerifiedBackend {
    type Alloc = u32;
    type Slot = u32;

    unsafe fn alloc(&mut self, _layout: Layout) -> Self::Alloc {
        // Find a free allocation slot or create a new one
        let alloc_id = self
            .allocs
            .iter()
            .position(|a| a.is_none())
            .unwrap_or_else(|| {
                self.allocs.push(None);
                self.allocs.len() - 1
            });

        // For now, each allocation gets one slot (we'll expand this)
        let slot_id = self.slots.len() as u32;
        self.slots.push(Some(false)); // allocated but not initialized

        self.allocs[alloc_id] = Some((slot_id, 1));
        alloc_id as u32
    }

    unsafe fn dealloc(&mut self, alloc: Self::Alloc, _layout: Layout) {
        let alloc_idx = alloc as usize;
        let (start, count) =
            self.allocs[alloc_idx].expect("dealloc: allocation already freed (double-free)");

        // Check all slots are uninitialized
        for i in start..(start + count) {
            let slot_state = self.slots[i as usize];
            assert!(
                slot_state == Some(false),
                "dealloc: slot {} is {:?}, expected allocated-but-uninitialized",
                i,
                slot_state
            );
        }

        // Mark slots as unallocated
        for i in start..(start + count) {
            self.slots[i as usize] = None;
        }

        // Free the allocation
        self.allocs[alloc_idx] = None;
    }

    unsafe fn slot(&mut self, alloc: Self::Alloc, offset: usize) -> Self::Slot {
        let alloc_idx = alloc as usize;
        let (start, count) = self.allocs[alloc_idx].expect("slot: allocation is not live");

        // For now, offset maps directly to slot index within allocation
        // This is simplified - real code would need to handle field offsets
        assert!(
            offset < count as usize,
            "slot: offset {} out of bounds (allocation has {} slots)",
            offset,
            count
        );

        start + offset as u32
    }

    unsafe fn mark_init(&mut self, slot: Self::Slot) {
        let slot_idx = slot as usize;
        let state = self.slots.get(slot_idx).copied().flatten();
        assert!(
            state == Some(false),
            "mark_init: slot {} is {:?}, expected allocated-but-uninitialized",
            slot_idx,
            state
        );
        self.slots[slot_idx] = Some(true);
    }

    unsafe fn mark_uninit(&mut self, slot: Self::Slot) {
        let slot_idx = slot as usize;
        let state = self.slots.get(slot_idx).copied().flatten();
        assert!(
            state == Some(true),
            "mark_uninit: slot {} is {:?}, expected initialized",
            slot_idx,
            state
        );
        self.slots[slot_idx] = Some(false);
    }

    unsafe fn is_init(&self, slot: Self::Slot) -> bool {
        let slot_idx = slot as usize;
        self.slots.get(slot_idx).copied().flatten().unwrap_or(false)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn real_backend_alloc_dealloc() {
        let mut backend = RealBackend;
        let layout = Layout::from_size_align(64, 8).unwrap();

        // SAFETY: layout is valid, we deallocate at the end
        let alloc = unsafe { backend.alloc(layout) };
        assert!(!alloc.is_null());

        // SAFETY: alloc is live, offset 0 is within bounds
        let slot = unsafe { backend.slot(alloc, 0) };
        // SAFETY: slot is valid (though we don't actually init memory in this test)
        unsafe { backend.mark_init(slot) };
        // SAFETY: slot is initialized
        unsafe { backend.mark_uninit(slot) };

        // SAFETY: alloc was allocated with this layout, all slots are uninit
        unsafe { backend.dealloc(alloc, layout) };
    }

    #[test]
    fn verified_backend_valid_transitions() {
        let mut backend = VerifiedBackend::new();
        let layout = Layout::from_size_align(64, 8).unwrap();

        // Alloc
        // SAFETY: valid layout
        let alloc = unsafe { backend.alloc(layout) };

        // Get slot
        // SAFETY: alloc is live, offset 0 is within bounds
        let slot = unsafe { backend.slot(alloc, 0) };
        // SAFETY: slot is valid
        assert!(unsafe { !backend.is_init(slot) });

        // Init
        // SAFETY: slot is valid and allocated but not initialized
        unsafe { backend.mark_init(slot) };
        // SAFETY: slot is valid
        assert!(unsafe { backend.is_init(slot) });

        // Uninit (drop)
        // SAFETY: slot is initialized
        unsafe { backend.mark_uninit(slot) };
        // SAFETY: slot is valid
        assert!(unsafe { !backend.is_init(slot) });

        // Dealloc
        // SAFETY: alloc was allocated with this layout, all slots are uninit
        unsafe { backend.dealloc(alloc, layout) };
    }

    #[test]
    #[should_panic(expected = "mark_init: slot")]
    fn verified_backend_double_init() {
        let mut backend = VerifiedBackend::new();
        let layout = Layout::from_size_align(64, 8).unwrap();

        // SAFETY: valid layout
        let alloc = unsafe { backend.alloc(layout) };
        // SAFETY: alloc is live
        let slot = unsafe { backend.slot(alloc, 0) };

        // SAFETY: slot is allocated
        unsafe { backend.mark_init(slot) };
        // This violates safety - slot is already init - should panic
        unsafe { backend.mark_init(slot) };
    }

    #[test]
    #[should_panic(expected = "mark_uninit: slot")]
    fn verified_backend_uninit_without_init() {
        let mut backend = VerifiedBackend::new();
        let layout = Layout::from_size_align(64, 8).unwrap();

        // SAFETY: valid layout
        let alloc = unsafe { backend.alloc(layout) };
        // SAFETY: alloc is live
        let slot = unsafe { backend.slot(alloc, 0) };

        // This violates safety - slot was never initialized - should panic
        unsafe { backend.mark_uninit(slot) };
    }

    #[test]
    #[should_panic(expected = "dealloc: slot")]
    fn verified_backend_dealloc_while_init() {
        let mut backend = VerifiedBackend::new();
        let layout = Layout::from_size_align(64, 8).unwrap();

        // SAFETY: valid layout
        let alloc = unsafe { backend.alloc(layout) };
        // SAFETY: alloc is live
        let slot = unsafe { backend.slot(alloc, 0) };

        // SAFETY: slot is allocated
        unsafe { backend.mark_init(slot) };
        // This violates safety - slot is still init - should panic
        unsafe { backend.dealloc(alloc, layout) };
    }

    #[test]
    #[should_panic(expected = "double-free")]
    fn verified_backend_double_free() {
        let mut backend = VerifiedBackend::new();
        let layout = Layout::from_size_align(64, 8).unwrap();

        // SAFETY: valid layout
        let alloc = unsafe { backend.alloc(layout) };
        // SAFETY: alloc is live, no slots init
        unsafe { backend.dealloc(alloc, layout) };
        // This violates safety - alloc already freed - should panic
        unsafe { backend.dealloc(alloc, layout) };
    }
}
