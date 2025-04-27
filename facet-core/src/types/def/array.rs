use crate::ptr::PtrConst;

use super::Shape;

/// Fields for array types
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
#[repr(C)]
#[non_exhaustive]
pub struct ArrayDef {
    /// vtable for interacting with the array
    pub vtable: &'static ArrayVTable,

    /// shape of the items in the array
    pub t: &'static Shape,

    /// The length of the array
    pub n: usize,
}

impl ArrayDef {
    /// Returns a builder for ArrayDef
    pub const fn builder() -> ArrayDefBuilder {
        ArrayDefBuilder::new()
    }

    /// Returns the shape of the items in the array
    pub fn t(&self) -> &'static Shape {
        self.t
    }
}

/// Builder for ArrayDef
pub struct ArrayDefBuilder {
    vtable: Option<&'static ArrayVTable>,
    t: Option<&'static Shape>,
    n: Option<usize>,
}

impl ArrayDefBuilder {
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
    pub const fn vtable(mut self, vtable: &'static ArrayVTable) -> Self {
        self.vtable = Some(vtable);
        self
    }

    /// Sets the item shape for the ArrayDef
    pub const fn t(mut self, t: &'static Shape) -> Self {
        self.t = Some(t);
        self
    }

    /// Sets the length for the ArrayDef (added method)
    pub const fn n(mut self, n: usize) -> Self {
        self.n = Some(n);
        self
    }

    /// Builds the ArrayDef
    pub const fn build(self) -> ArrayDef {
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

/// Virtual table for an array
#[derive(Clone, Copy, Debug, Hash, PartialEq, Eq)]
#[repr(C)]
#[non_exhaustive]
pub struct ArrayVTable {
    /// cf. [`ArrayAsPtrFn`]
    pub as_ptr: ArrayAsPtrFn,
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
}

impl ArrayVTableBuilder {
    /// Creates a new [`ArrayVTableBuilder`] with all fields set to `None`.
    #[allow(clippy::new_without_default)]
    pub const fn new() -> Self {
        Self { as_ptr_fn: None }
    }

    /// Sets the as_ptr field
    pub const fn as_ptr(mut self, f: ArrayAsPtrFn) -> Self {
        self.as_ptr_fn = Some(f);
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
        }
    }
}
