use super::{PtrConst, PtrMut, PtrUninit};

use super::{IterVTable, Shape};

/// Fields for set types
#[derive(Clone, Copy, Debug)]
#[repr(C)]
pub struct SetDef {
    /// vtable for interacting with the set
    pub vtable: &'static SetVTable,
    /// shape of the values in the set
    pub t: &'static Shape,
}

impl SetDef {
    /// Construct a `SetDef` from its vtable and element shape.
    pub const fn new(vtable: &'static SetVTable, t: &'static Shape) -> Self {
        Self { vtable, t }
    }

    /// Returns the shape of the items in the set
    pub const fn t(&self) -> &'static Shape {
        self.t
    }
}

/// Initialize a set in place with a given capacity
///
/// # Safety
///
/// The `set` parameter must point to uninitialized memory of sufficient size.
/// The function must properly initialize the memory.
pub type SetInitInPlaceWithCapacityFn = unsafe fn(set: PtrUninit, capacity: usize) -> PtrMut;

/// Insert a value in the set if not already contained, returning true
/// if the value wasn't present before
///
/// # Safety
///
/// The `set` parameter must point to aligned, initialized memory of the correct type.
/// `value` is moved out of (with [`core::ptr::read`]) — it should be deallocated afterwards (e.g.
/// with [`core::mem::forget`]) but NOT dropped.
pub type SetInsertFn = unsafe fn(set: PtrMut, value: PtrMut) -> bool;

/// Get the number of values in the set
///
/// # Safety
///
/// The `set` parameter must point to aligned, initialized memory of the correct type.
pub type SetLenFn = unsafe fn(set: PtrConst) -> usize;

/// Check if the set contains a value
///
/// # Safety
///
/// The `set` parameter must point to aligned, initialized memory of the correct type.
pub type SetContainsFn = unsafe fn(set: PtrConst, value: PtrConst) -> bool;

vtable_def! {
    /// Virtual table for a `Set<T>`
    #[derive(Clone, Copy, Debug)]
    #[repr(C)]
    pub struct SetVTable + SetVTableBuilder {
        /// cf. [`SetInitInPlaceWithCapacityFn`]
        pub init_in_place_with_capacity: SetInitInPlaceWithCapacityFn,

        /// cf. [`SetInsertFn`]
        pub insert: SetInsertFn,

        /// cf. [`SetLenFn`]
        pub len: SetLenFn,

        /// cf. [`SetContainsFn`]
        pub contains: SetContainsFn,

        /// Virtual table for set iterator operations
        pub iter_vtable: IterVTable<PtrConst>,
    }
}
