use super::{PtrConst, PtrMut};

use super::Shape;

/// Fields for slice types
#[derive(Clone, Copy, Debug)]
#[repr(C)]
pub struct SliceDef {
    /// vtable for interacting with the slice
    pub vtable: &'static SliceVTable,
    /// shape of the items in the slice
    pub t: &'static Shape,
}

impl SliceDef {
    /// Construct a `SliceDef` from its vtable and element shape.
    pub const fn new(vtable: &'static SliceVTable, t: &'static Shape) -> Self {
        Self { vtable, t }
    }

    /// Returns the shape of the items in the slice
    pub const fn t(&self) -> &'static Shape {
        self.t
    }
}

/// Get the number of items in the slice
///
/// # Safety
///
/// The `slice` parameter must point to aligned, initialized memory of the correct type.
pub type SliceLenFn = unsafe fn(slice: PtrConst) -> usize;

/// Get pointer to the data buffer of the slice
///
/// # Safety
///
/// The `slice` parameter must point to aligned, initialized memory of the correct type.
pub type SliceAsPtrFn = unsafe fn(slice: PtrConst) -> PtrConst;

/// Get mutable pointer to the data buffer of the slice
///
/// # Safety
///
/// The `slice` parameter must point to aligned, initialized memory of the correct type.
pub type SliceAsMutPtrFn = unsafe fn(slice: PtrMut) -> PtrMut;

/// Virtual table for a slice-like type (like `Vec<T>`,
/// but also `HashSet<T>`, etc.)
#[derive(Clone, Copy, Debug)]
#[repr(C)]
pub struct SliceVTable {
    /// Number of items in the slice
    pub len: SliceLenFn,
    /// Get pointer to the data buffer of the slice.
    pub as_ptr: SliceAsPtrFn,
    /// Get mutable pointer to the data buffer of the slice.
    pub as_mut_ptr: SliceAsMutPtrFn,
}
impl SliceVTable {
    /// Const ctor for slice vtable.
    pub const fn new(len: SliceLenFn, as_ptr: SliceAsPtrFn, as_mut_ptr: SliceAsMutPtrFn) -> Self {
        Self {
            len,
            as_ptr,
            as_mut_ptr,
        }
    }
}
