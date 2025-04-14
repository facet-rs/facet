use crate::opaque::OpaqueConst;

use super::Shape;

/// Fields for array types
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
#[repr(C)]
#[non_exhaustive]
pub struct ArrayDef {
    /// vtable for interacting with the array
    pub vtable: &'static ArrayVTable,

    /// shape of the items in the list
    pub t: fn() -> &'static Shape,

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
        (self.t)()
    }
}

/// Builder for ArrayDef
pub struct ArrayDefBuilder {
    vtable: Option<&'static ArrayVTable>,
    t: Option<fn() -> &'static Shape>,
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
    pub const fn t(mut self, t: fn() -> &'static Shape) -> Self {
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

/// Get pointer to the item at the given index. Panics if out of bounds.
///
/// # Safety
///
/// The `array` parameter must point to aligned, initialized memory of the correct type.
pub type ArrayGetItemPtrFn = unsafe fn(array: OpaqueConst, index: usize) -> OpaqueConst;

/// Virtual table for a list-like type (like `Vec<T>`,
/// but also `HashSet<T>`, etc.)
#[derive(Clone, Copy, Debug, Hash, PartialEq, Eq)]
#[repr(C)]
#[non_exhaustive]
pub struct ArrayVTable {
    /// cf. [`ArrayGetItemPtrFn`]
    pub get_item_ptr: ArrayGetItemPtrFn,
    // TODO: mutation
}

impl ArrayVTable {
    /// Returns a builder for ListVTable
    pub const fn builder() -> ArrayVTableBuilder {
        ArrayVTableBuilder::new()
    }
}

/// Builds a [`ArrayVTable`]
pub struct ArrayVTableBuilder {
    get_item_ptr: Option<ArrayGetItemPtrFn>,
}

impl ArrayVTableBuilder {
    /// Creates a new [`ArrayVTableBuilder`] with all fields set to `None`.
    #[allow(clippy::new_without_default)]
    pub const fn new() -> Self {
        Self { get_item_ptr: None }
    }

    /// Sets the get_item_ptr field
    pub const fn get_item_ptr(mut self, f: ArrayGetItemPtrFn) -> Self {
        self.get_item_ptr = Some(f);
        self
    }

    /// Builds the [`ArrayVTable`] from the current state of the builder.
    ///
    /// # Panics
    ///
    /// This method will panic if any of the required fields are `None`.
    pub const fn build(self) -> ArrayVTable {
        ArrayVTable {
            get_item_ptr: self.get_item_ptr.unwrap(),
        }
    }
}
