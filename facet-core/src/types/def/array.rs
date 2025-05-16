use crate::{PtrMut, ptr::PtrConst};

use super::Shape;

/// Fields for array types
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
#[repr(C)]
#[non_exhaustive]
pub struct ArrayDef<'shape> {
    /// vtable for interacting with the array
    pub vtable: &'shape ArrayVTable,

    /// shape of the items in the array
    pub t: &'shape Shape<'shape>,

    /// The length of the array
    pub n: usize,
}

impl<'shape> ArrayDef<'shape> {
    /// Returns a builder for ArrayDef
    pub const fn builder() -> ArrayDefBuilder<'shape> {
        ArrayDefBuilder::new()
    }

    /// Returns the shape of the items in the array
    pub fn t(&self) -> &'shape Shape<'shape> {
        self.t
    }
}

/// Builder for ArrayDef
pub struct ArrayDefBuilder<'shape> {
    vtable: Option<&'shape ArrayVTable>,
    t: Option<&'shape Shape<'shape>>,
    n: Option<usize>,
}

impl<'shape> ArrayDefBuilder<'shape> {
    /// Creates a new ArrayDefBuilder
    #[allow(clippy::new_without_default)]
    pub const fn new() -> Self {
        Self {
            vtable: None,
            t: None,
            n: None,
        }
    }

    /// Sets the vtable for the ArrayDef
    pub const fn vtable(mut self, vtable: &'shape ArrayVTable) -> Self {
        self.vtable = Some(vtable);
        self
    }

    /// Sets the item shape for the ArrayDef
    pub const fn t(mut self, t: &'shape Shape<'shape>) -> Self {
        self.t = Some(t);
        self
    }

    /// Sets the length for the ArrayDef (added method)
    pub const fn n(mut self, n: usize) -> Self {
        self.n = Some(n);
        self
    }

    /// Builds the ArrayDef
    pub const fn build(self) -> ArrayDef<'shape> {
        ArrayDef {
            vtable: self.vtable.unwrap(),
            t: self.t.unwrap(),
            n: self.n.unwrap(),
        }
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
#[derive(Clone, Copy, Debug, Hash, PartialEq, Eq)]
#[repr(C)]
#[non_exhaustive]
pub struct ArrayVTable {
    /// cf. [`ArrayAsPtrFn`]
    pub as_ptr: ArrayAsPtrFn,

    /// cf. [`ArrayAsMutPtrFn`]
    pub as_mut_ptr: ArrayAsMutPtrFn,
}

impl ArrayVTable {
    /// Returns a builder for ListVTable
    pub const fn builder() -> ArrayVTableBuilder {
        ArrayVTableBuilder::new()
    }
}

/// Builds a [`ArrayVTable`]
pub struct ArrayVTableBuilder {
    as_ptr_fn: Option<ArrayAsPtrFn>,
    as_mut_ptr_fn: Option<ArrayAsMutPtrFn>,
}

impl ArrayVTableBuilder {
    /// Creates a new [`ArrayVTableBuilder`] with all fields set to `None`.
    #[allow(clippy::new_without_default)]
    pub const fn new() -> Self {
        Self {
            as_ptr_fn: None,
            as_mut_ptr_fn: None,
        }
    }

    /// Sets the as_ptr field
    pub const fn as_ptr(mut self, f: ArrayAsPtrFn) -> Self {
        self.as_ptr_fn = Some(f);
        self
    }

    /// Sets the as_mut_ptr field
    pub const fn as_mut_ptr(mut self, f: ArrayAsMutPtrFn) -> Self {
        self.as_mut_ptr_fn = Some(f);
        self
    }

    /// Builds the [`ArrayVTable`] from the current state of the builder.
    ///
    /// # Panics
    ///
    /// This method will panic if any of the required fields are `None`.
    pub const fn build(self) -> ArrayVTable {
        ArrayVTable {
            as_ptr: self.as_ptr_fn.unwrap(),
            as_mut_ptr: self.as_mut_ptr_fn.unwrap(),
        }
    }
}
