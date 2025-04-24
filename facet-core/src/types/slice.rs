use crate::ptr::PtrConst;

use super::Shape;

/// Fields for slice types
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
#[repr(C)]
#[non_exhaustive]
pub struct SliceDef {
    /// vtable for interacting with the slice
    pub vtable: &'static SliceVTable,

    /// shape of the items in the slice
    pub t: &'static Shape,
}

impl SliceDef {
    /// Returns a builder for SliceDef
    pub const fn builder() -> SliceDefBuilder {
        SliceDefBuilder::new()
    }
}

/// Builder for SliceDef
pub struct SliceDefBuilder {
    vtable: Option<&'static SliceVTable>,
    t: Option<&'static Shape>,
}

impl SliceDefBuilder {
    /// Creates a new SliceDefBuilder
    #[allow(clippy::new_without_default)]
    pub const fn new() -> Self {
        Self {
            vtable: None,
            t: None,
        }
    }

    /// Sets the vtable for the SliceDef
    pub const fn vtable(mut self, vtable: &'static SliceVTable) -> Self {
        self.vtable = Some(vtable);
        self
    }

    /// Sets the item shape for the SliceDef
    pub const fn t(mut self, t: &'static Shape) -> Self {
        self.t = Some(t);
        self
    }

    /// Builds the SliceDef
    pub const fn build(self) -> SliceDef {
        SliceDef {
            vtable: self.vtable.unwrap(),
            t: self.t.unwrap(),
        }
    }
}

/// Get pointer to the item at the given index. Panics if out of bounds.
///
/// # Safety
///
/// - The `slice` parameter must point to aligned, initialized memory of the correct type.
/// - The index must be in bounds.
pub type SliceGetItemPtrFn = unsafe fn(slice: PtrConst, index: usize) -> PtrConst;

/// Virtual table for a slice-like type (like `Vec<T>`,
/// but also `HashSet<T>`, etc.)
#[derive(Clone, Copy, Debug, Hash, PartialEq, Eq)]
#[repr(C)]
#[non_exhaustive]
pub struct SliceVTable {
    /// Get pointer to the item at the given index. Undefined behavior if out of bounds.
    pub get_item_ptr: SliceGetItemPtrFn,
}

impl SliceVTable {
    /// Returns a builder for SliceVTable
    pub const fn builder() -> SliceVTableBuilder {
        SliceVTableBuilder::new()
    }
}

/// Builds a [`SliceVTable`]
pub struct SliceVTableBuilder {
    get_item_ptr: Option<SliceGetItemPtrFn>,
}

impl SliceVTableBuilder {
    /// Creates a new [`SliceVTableBuilder`] with all fields set to `None`.
    #[allow(clippy::new_without_default)]
    pub const fn new() -> Self {
        Self { get_item_ptr: None }
    }

    /// Sets the get_item_ptr field
    pub const fn get_item_ptr(mut self, f: SliceGetItemPtrFn) -> Self {
        self.get_item_ptr = Some(f);
        self
    }

    /// Builds the [`SliceVTable`] from the current state of the builder.
    ///
    /// # Panics
    ///
    /// This method will panic if any of the required fields are `None`.
    pub const fn build(self) -> SliceVTable {
        SliceVTable {
            get_item_ptr: self.get_item_ptr.unwrap(),
        }
    }
}
