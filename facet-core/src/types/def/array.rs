use crate::{PtrMut, ptr::PtrConst};

use super::Shape;

/// Fields for array types
#[derive(Clone, Copy, Debug)]
#[repr(C)]
pub struct ArrayDef {
    /// vtable for interacting with the array
    pub vtable: &'static ArrayVTable,

    /// shape of the items in the array
    pub t: &'static Shape,

    /// The length of the array
    pub n: usize,
}

impl ArrayDef {
    /// Construct an `ArrayDef` from its vtable, element shape, and length.
    pub const fn new(vtable: &'static ArrayVTable, t: &'static Shape, n: usize) -> Self {
        Self { vtable, t, n }
    }

    /// Returns the shape of the items in the array
    pub const fn t(&self) -> &'static Shape {
        self.t
    }
}

/// Get pointer to the data buffer of the array.
///
/// # Safety
///
/// The `array` parameter must point to aligned, initialized memory of the correct type.
pub type ArrayAsPtrFn = unsafe fn(array: PtrConst) -> PtrConst;

/// Get mutable pointer to the data buffer of the array.
///
/// # Safety
///
/// The `array` parameter must point to aligned, initialized memory of the correct type.
pub type ArrayAsMutPtrFn = unsafe fn(array: PtrMut) -> PtrMut;

/// Virtual table for an array
#[derive(Clone, Copy, Debug)]
#[repr(C)]
pub struct ArrayVTable {
    /// cf. [`ArrayAsPtrFn`]
    pub as_ptr: ArrayAsPtrFn,

    /// cf. [`ArrayAsMutPtrFn`]
    pub as_mut_ptr: ArrayAsMutPtrFn,
}

impl ArrayVTable {
    /// Const ctor for array vtable.
    pub const fn new(as_ptr: ArrayAsPtrFn, as_mut_ptr: ArrayAsMutPtrFn) -> Self {
        Self { as_ptr, as_mut_ptr }
    }
}
