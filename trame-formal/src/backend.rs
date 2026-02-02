//! Backend abstraction for memory operations.
//!
//! Two implementations:
//! - `RealBackend`: Actual memory operations (for production)
//! - `VerifiedBackend`: State tracking with assertions (for Kani)
//!
//! The key insight: we have ONE implementation of business logic that uses
//! the Backend trait. For verification, we swap in VerifiedBackend which
//! tracks state and asserts valid transitions. For production, RealBackend
//! performs actual memory operations with zero overhead.

use crate::dyn_shape::{DynShape, IShape, IStructType};

/// Maximum number of allocations tracked by VerifiedBackend.
pub const MAX_ALLOCS: usize = 8;

/// Maximum number of slots tracked by VerifiedBackend.
pub const MAX_SLOTS: usize = 32;

/// State of a memory slot.
///
/// The valid state transitions are:
///
/// ```text
/// Unallocated  --alloc-->  Allocated  --init-->  Initialized
///                               ^                     |
///                               |----drop_in_place----|
///                               |
/// Unallocated  <--dealloc-------+
/// ```
///
/// Key invariants:
/// - Cannot init an Unallocated slot (no memory!)
/// - Cannot init an already Initialized slot (double-init!)
/// - Cannot drop an Unallocated or Allocated slot (nothing to drop!)
/// - Cannot dealloc while any slot is Initialized (leak the drop!)
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SlotState {
    /// Not allocated - no memory exists for this slot.
    Unallocated,
    /// Allocated but not initialized - memory exists but contains garbage.
    Allocated,
    /// Allocated and initialized - memory contains a valid value.
    Initialized,
}

/// Backend for memory operations, generic over shape type.
///
/// # Safety
///
/// All methods are unsafe because they have preconditions that cannot be
/// checked at compile time. The VerifiedBackend asserts these preconditions
/// for Kani proofs; RealBackend assumes they hold (undefined behavior otherwise).
pub trait Backend<S: IShape> {
    /// Handle to an allocation (a contiguous region of slots).
    type Alloc: Copy;

    /// Handle to a single slot within an allocation.
    type Slot: Copy;

    /// Allocate memory for a shape.
    ///
    /// # Safety
    /// - Caller must eventually call `dealloc` to avoid leaks
    /// - All slots start in `Allocated` state
    unsafe fn alloc(&mut self, shape: S) -> Self::Alloc;

    /// Deallocate memory.
    ///
    /// # Safety
    /// - `alloc` must be a live allocation (not already freed)
    /// - All slots must be in `Allocated` state (not `Initialized` - drop first!)
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
    /// - Slot must be in `Allocated` state (not Unallocated or Initialized)
    /// - Memory must actually be initialized before calling this
    unsafe fn mark_init(&mut self, slot: Self::Slot);

    /// Mark a slot as uninitialized (after drop_in_place).
    ///
    /// # Safety
    /// - `slot` must be valid
    /// - Slot must be in `Initialized` state
    /// - `drop_in_place` must have been called on the memory
    unsafe fn mark_uninit(&mut self, slot: Self::Slot);

    /// Check if a slot is initialized.
    ///
    /// # Safety
    /// - `slot` must be valid
    unsafe fn is_init(&self, slot: Self::Slot) -> bool;
}

/// Verified backend that tracks state for Kani proofs.
///
/// This backend doesn't do any real memory operations - it just tracks
/// the abstract state of slots and asserts that all transitions are valid.
#[derive(Debug)]
pub struct VerifiedBackend {
    /// State of each slot.
    slots: [SlotState; MAX_SLOTS],
    /// Next slot index to allocate from.
    next_slot: usize,
    /// Allocations: Some((start_slot, slot_count)) if live, None if freed.
    allocs: [Option<(u16, u16)>; MAX_ALLOCS],
    /// Next allocation index.
    next_alloc: usize,
}

impl VerifiedBackend {
    /// Create a new verified backend with all slots unallocated.
    pub fn new() -> Self {
        Self {
            slots: [SlotState::Unallocated; MAX_SLOTS],
            next_slot: 0,
            allocs: [None; MAX_ALLOCS],
            next_alloc: 0,
        }
    }

    /// Check that all allocations have been freed (for leak detection).
    #[cfg(test)]
    pub fn assert_no_leaks(&self) {
        for (i, alloc) in self.allocs.iter().enumerate() {
            assert!(alloc.is_none(), "allocation {} not freed: {:?}", i, alloc);
        }
    }
}

impl Default for VerifiedBackend {
    fn default() -> Self {
        Self::new()
    }
}

/// Helper to get slot count from a shape.
fn slot_count<S: IShape>(shape: S) -> usize {
    if let Some(st) = shape.as_struct() {
        st.field_count()
    } else {
        1 // Scalars have 1 slot
    }
}

impl Backend<DynShape> for VerifiedBackend {
    type Alloc = u8;
    type Slot = u16;

    unsafe fn alloc(&mut self, shape: DynShape) -> Self::Alloc {
        let slots = slot_count(shape);

        assert!(self.next_alloc < MAX_ALLOCS, "too many allocations");
        assert!(self.next_slot + slots <= MAX_SLOTS, "too many slots");

        let alloc_id = self.next_alloc;
        let start_slot = self.next_slot;

        // Mark slots as allocated
        for i in 0..slots {
            assert!(
                self.slots[start_slot + i] == SlotState::Unallocated,
                "slot {} already allocated",
                start_slot + i
            );
            self.slots[start_slot + i] = SlotState::Allocated;
        }

        self.allocs[alloc_id] = Some((start_slot as u16, slots as u16));
        self.next_alloc += 1;
        self.next_slot += slots;

        alloc_id as u8
    }

    unsafe fn dealloc(&mut self, alloc: Self::Alloc) {
        let alloc_idx = alloc as usize;
        let (start, count) = self.allocs[alloc_idx].expect("dealloc: already freed");

        // Verify all slots are Allocated (not Initialized - must drop first!)
        for i in 0..(count as usize) {
            let slot_idx = (start as usize) + i;
            assert!(
                self.slots[slot_idx] == SlotState::Allocated,
                "dealloc: slot {} is {:?}, expected Allocated (did you forget to drop?)",
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
    use crate::dyn_shape::DynField;
    use core::alloc::Layout;

    #[test]
    fn scalar_lifecycle() {
        let mut backend = VerifiedBackend::new();
        let shape = DynShape::scalar(Layout::new::<u32>());
        let alloc = unsafe { backend.alloc(shape) };
        let slot = unsafe { backend.slot(alloc, 0) };

        assert!(!unsafe { backend.is_init(slot) });
        unsafe { backend.mark_init(slot) };
        assert!(unsafe { backend.is_init(slot) });
        unsafe { backend.mark_uninit(slot) };
        unsafe { backend.dealloc(alloc) };
        backend.assert_no_leaks();
    }

    #[test]
    fn struct_lifecycle() {
        let mut backend = VerifiedBackend::new();
        let fields = [
            DynField::new(0, Layout::new::<u32>()),
            DynField::new(4, Layout::new::<u32>()),
        ];
        let shape = DynShape::struct_with_fields(&fields);
        let alloc = unsafe { backend.alloc(shape) };

        let slot0 = unsafe { backend.slot(alloc, 0) };
        let slot1 = unsafe { backend.slot(alloc, 1) };

        unsafe { backend.mark_init(slot0) };
        unsafe { backend.mark_init(slot1) };
        unsafe { backend.mark_uninit(slot0) };
        unsafe { backend.mark_uninit(slot1) };
        unsafe { backend.dealloc(alloc) };
        backend.assert_no_leaks();
    }

    #[test]
    #[should_panic(expected = "mark_init")]
    fn double_init_panics() {
        let mut backend = VerifiedBackend::new();
        let shape = DynShape::scalar(Layout::new::<u32>());
        let alloc = unsafe { backend.alloc(shape) };
        let slot = unsafe { backend.slot(alloc, 0) };

        unsafe { backend.mark_init(slot) };
        unsafe { backend.mark_init(slot) };
    }

    #[test]
    #[should_panic(expected = "dealloc")]
    fn dealloc_while_init_panics() {
        let mut backend = VerifiedBackend::new();
        let shape = DynShape::scalar(Layout::new::<u32>());
        let alloc = unsafe { backend.alloc(shape) };
        let slot = unsafe { backend.slot(alloc, 0) };

        unsafe { backend.mark_init(slot) };
        unsafe { backend.dealloc(alloc) };
    }
}

#[cfg(kani)]
mod kani_proofs {
    use super::*;
    use crate::dyn_shape::{DynDef, DynField, DynStructType, MAX_FIELDS};
    use core::alloc::Layout;

    #[kani::proof]
    #[kani::unwind(10)]
    fn scalar_lifecycle() {
        let mut backend = VerifiedBackend::new();
        let shape = DynShape::scalar(Layout::from_size_align(4, 4).unwrap());
        let alloc = unsafe { backend.alloc(shape) };
        let slot = unsafe { backend.slot(alloc, 0) };

        unsafe { backend.mark_init(slot) };
        unsafe { backend.mark_uninit(slot) };
        unsafe { backend.dealloc(alloc) };
    }

    #[kani::proof]
    #[kani::unwind(10)]
    fn struct_lifecycle() {
        let field_count: u8 = kani::any();
        kani::assume(field_count > 0 && field_count <= 4);

        let mut fields = [DynField::new(0, Layout::new::<()>()); MAX_FIELDS];
        for i in 0..(field_count as usize) {
            fields[i] = DynField::new(i * 4, Layout::from_size_align(4, 1).unwrap());
        }

        let shape = DynShape {
            layout: Layout::from_size_align((field_count as usize) * 4, 1).unwrap(),
            def: DynDef::Struct(DynStructType {
                field_count,
                fields,
            }),
        };

        let mut backend = VerifiedBackend::new();
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
