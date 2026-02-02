//! Backend abstraction for memory operations.
//!
//! Two implementations:
//! - `RealBackend`: Actual memory operations (for production)
//! - `VerifiedBackend`: State tracking with assertions (for Kani)

use crate::shape::ShapeDesc;

/// Maximum number of allocations tracked by VerifiedBackend.
pub const MAX_ALLOCS: usize = 8;

/// Maximum number of slots tracked by VerifiedBackend.
pub const MAX_SLOTS: usize = 32;

/// State of a memory slot.
///
/// ```text
/// Unallocated  --alloc-->  Allocated  --init-->  Initialized
///                               ^                     |
///                               |----drop_in_place----|
///                               |
/// Unallocated  <--dealloc-------+
/// ```
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SlotState {
    /// Not allocated.
    Unallocated,
    /// Allocated but not initialized.
    Allocated,
    /// Allocated and initialized.
    Initialized,
}

/// Backend for memory operations.
///
/// # Safety
///
/// All methods are unsafe because they have preconditions that cannot be
/// checked at compile time.
pub trait Backend {
    /// Handle to an allocation.
    type Alloc: Copy;

    /// Handle to a slot within an allocation.
    type Slot: Copy;

    /// Allocate memory for a shape.
    ///
    /// # Safety
    /// - Caller must eventually call `dealloc`
    unsafe fn alloc(&mut self, shape: ShapeDesc) -> Self::Alloc;

    /// Deallocate memory.
    ///
    /// # Safety
    /// - `alloc` must be a live allocation
    /// - All slots must be in `Allocated` state (not `Initialized`)
    unsafe fn dealloc(&mut self, alloc: Self::Alloc);

    /// Get a slot handle for a field within an allocation.
    ///
    /// # Safety
    /// - `alloc` must be a live allocation
    /// - `field_idx` must be within bounds for the shape
    unsafe fn slot(&self, alloc: Self::Alloc, field_idx: usize) -> Self::Slot;

    /// Mark a slot as initialized.
    ///
    /// # Safety
    /// - `slot` must be valid
    /// - Slot must be in `Allocated` state
    /// - Memory must actually be initialized
    unsafe fn mark_init(&mut self, slot: Self::Slot);

    /// Mark a slot as uninitialized (after drop).
    ///
    /// # Safety
    /// - `slot` must be valid
    /// - Slot must be in `Initialized` state
    /// - `drop_in_place` must have been called
    unsafe fn mark_uninit(&mut self, slot: Self::Slot);

    /// Check if a slot is initialized.
    ///
    /// # Safety
    /// - `slot` must be valid
    unsafe fn is_init(&self, slot: Self::Slot) -> bool;
}

/// Verified backend that tracks state for Kani proofs.
#[derive(Debug)]
pub struct VerifiedBackend {
    /// State of each slot.
    slots: [SlotState; MAX_SLOTS],
    /// Next slot index to allocate.
    next_slot: usize,
    /// Allocations: (start_slot, slot_count), or None if freed.
    allocs: [Option<(u16, u16)>; MAX_ALLOCS],
    /// Next allocation index.
    next_alloc: usize,
}

impl VerifiedBackend {
    pub fn new() -> Self {
        Self {
            slots: [SlotState::Unallocated; MAX_SLOTS],
            next_slot: 0,
            allocs: [None; MAX_ALLOCS],
            next_alloc: 0,
        }
    }
}

impl Default for VerifiedBackend {
    fn default() -> Self {
        Self::new()
    }
}

impl Backend for VerifiedBackend {
    type Alloc = u8;
    type Slot = u16;

    unsafe fn alloc(&mut self, shape: ShapeDesc) -> Self::Alloc {
        let slot_count = shape.slot_count();

        assert!(self.next_alloc < MAX_ALLOCS, "too many allocations");
        assert!(self.next_slot + slot_count <= MAX_SLOTS, "too many slots");

        let alloc_id = self.next_alloc;
        let start_slot = self.next_slot;

        // Mark slots as allocated
        for i in 0..slot_count {
            self.slots[start_slot + i] = SlotState::Allocated;
        }

        self.allocs[alloc_id] = Some((start_slot as u16, slot_count as u16));
        self.next_alloc += 1;
        self.next_slot += slot_count;

        alloc_id as u8
    }

    unsafe fn dealloc(&mut self, alloc: Self::Alloc) {
        let alloc_idx = alloc as usize;
        let (start, count) = self.allocs[alloc_idx].expect("dealloc: already freed");

        // Verify all slots are Allocated (not Initialized)
        for i in 0..(count as usize) {
            let slot_idx = (start as usize) + i;
            assert!(
                self.slots[slot_idx] == SlotState::Allocated,
                "dealloc: slot {} is {:?}, expected Allocated",
                slot_idx,
                self.slots[slot_idx]
            );
            self.slots[slot_idx] = SlotState::Unallocated;
        }

        self.allocs[alloc_idx] = None;
    }

    unsafe fn slot(&self, alloc: Self::Alloc, field_idx: usize) -> Self::Slot {
        let alloc_idx = alloc as usize;
        let (start, count) = self.allocs[alloc_idx].expect("slot: allocation not live");

        assert!(
            field_idx < count as usize,
            "slot: field_idx {} out of bounds (count {})",
            field_idx,
            count
        );

        (start as usize + field_idx) as u16
    }

    unsafe fn mark_init(&mut self, slot: Self::Slot) {
        let idx = slot as usize;
        assert!(
            self.slots[idx] == SlotState::Allocated,
            "mark_init: slot {} is {:?}, expected Allocated",
            idx,
            self.slots[idx]
        );
        self.slots[idx] = SlotState::Initialized;
    }

    unsafe fn mark_uninit(&mut self, slot: Self::Slot) {
        let idx = slot as usize;
        assert!(
            self.slots[idx] == SlotState::Initialized,
            "mark_uninit: slot {} is {:?}, expected Initialized",
            idx,
            self.slots[idx]
        );
        self.slots[idx] = SlotState::Allocated;
    }

    unsafe fn is_init(&self, slot: Self::Slot) -> bool {
        self.slots[slot as usize] == SlotState::Initialized
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn alloc_scalar() {
        let mut backend = VerifiedBackend::new();
        let alloc = unsafe { backend.alloc(ShapeDesc::Scalar) };
        let slot = unsafe { backend.slot(alloc, 0) };

        assert!(!unsafe { backend.is_init(slot) });

        unsafe { backend.mark_init(slot) };
        assert!(unsafe { backend.is_init(slot) });

        unsafe { backend.mark_uninit(slot) };
        assert!(!unsafe { backend.is_init(slot) });

        unsafe { backend.dealloc(alloc) };
    }

    #[test]
    fn alloc_struct() {
        let mut backend = VerifiedBackend::new();
        let alloc = unsafe { backend.alloc(ShapeDesc::Struct { field_count: 3 }) };

        // Init fields out of order
        let slot1 = unsafe { backend.slot(alloc, 1) };
        let slot0 = unsafe { backend.slot(alloc, 0) };
        let slot2 = unsafe { backend.slot(alloc, 2) };

        unsafe { backend.mark_init(slot1) };
        unsafe { backend.mark_init(slot2) };
        unsafe { backend.mark_init(slot0) };

        // Uninit all
        unsafe { backend.mark_uninit(slot0) };
        unsafe { backend.mark_uninit(slot1) };
        unsafe { backend.mark_uninit(slot2) };

        unsafe { backend.dealloc(alloc) };
    }

    #[test]
    #[should_panic(expected = "mark_init")]
    fn double_init_panics() {
        let mut backend = VerifiedBackend::new();
        let alloc = unsafe { backend.alloc(ShapeDesc::Scalar) };
        let slot = unsafe { backend.slot(alloc, 0) };

        unsafe { backend.mark_init(slot) };
        unsafe { backend.mark_init(slot) }; // panic
    }

    #[test]
    #[should_panic(expected = "mark_uninit")]
    fn uninit_without_init_panics() {
        let mut backend = VerifiedBackend::new();
        let alloc = unsafe { backend.alloc(ShapeDesc::Scalar) };
        let slot = unsafe { backend.slot(alloc, 0) };

        unsafe { backend.mark_uninit(slot) }; // panic
    }

    #[test]
    #[should_panic(expected = "dealloc")]
    fn dealloc_while_init_panics() {
        let mut backend = VerifiedBackend::new();
        let alloc = unsafe { backend.alloc(ShapeDesc::Scalar) };
        let slot = unsafe { backend.slot(alloc, 0) };

        unsafe { backend.mark_init(slot) };
        unsafe { backend.dealloc(alloc) }; // panic
    }
}

#[cfg(kani)]
mod kani_proofs {
    use super::*;

    /// Verify that valid operation sequences don't panic.
    #[kani::proof]
    #[kani::unwind(10)]
    fn valid_scalar_lifecycle() {
        let mut backend = VerifiedBackend::new();
        let alloc = unsafe { backend.alloc(ShapeDesc::Scalar) };
        let slot = unsafe { backend.slot(alloc, 0) };

        // Must init before uninit
        unsafe { backend.mark_init(slot) };
        unsafe { backend.mark_uninit(slot) };

        // Must uninit before dealloc
        unsafe { backend.dealloc(alloc) };
    }

    /// Verify struct field operations.
    #[kani::proof]
    #[kani::unwind(10)]
    fn valid_struct_lifecycle() {
        let field_count: u8 = kani::any();
        kani::assume(field_count > 0 && field_count <= 4);

        let mut backend = VerifiedBackend::new();
        let shape = ShapeDesc::Struct { field_count };
        let alloc = unsafe { backend.alloc(shape) };

        // Init all fields
        for i in 0..(field_count as usize) {
            let slot = unsafe { backend.slot(alloc, i) };
            unsafe { backend.mark_init(slot) };
        }

        // Uninit all fields
        for i in 0..(field_count as usize) {
            let slot = unsafe { backend.slot(alloc, i) };
            unsafe { backend.mark_uninit(slot) };
        }

        unsafe { backend.dealloc(alloc) };
    }
}
