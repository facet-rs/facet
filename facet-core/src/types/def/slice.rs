use crate::{PtrMut, ptr::PtrConst};

use super::Shape;

/// Fields for slice types
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
#[repr(C)]
#[non_exhaustive]
pub struct SliceDef<'shape> {
    /// vtable for interacting with the slice
    pub vtable: &'shape SliceVTable,
    /// shape of the items in the slice
    pub t: &'shape Shape<'shape>,
}

impl<'shape> SliceDef<'shape> {
    /// Returns a builder for SliceDef
    pub const fn builder() -> SliceDefBuilder<'shape> {
        SliceDefBuilder::new()
    }

    /// Returns the shape of the items in the slice
    pub const fn t(&self) -> &'shape Shape<'shape> {
        self.t
    }
}

/// Builder for SliceDef
pub struct SliceDefBuilder<'shape> {
    vtable: Option<&'shape SliceVTable>,
    t: Option<&'shape Shape<'shape>>,
}

impl<'shape> SliceDefBuilder<'shape> {
    /// Creates a new SliceDefBuilder
    #[allow(clippy::new_without_default)]
    pub const fn new() -> Self {
        Self {
            vtable: None,
            t: None,
        }
    }

    /// Sets the vtable for the SliceDef
    pub const fn vtable(mut self, vtable: &'shape SliceVTable) -> Self {
        self.vtable = Some(vtable);
        self
    }

    /// Sets the item shape for the SliceDef
    pub const fn t(mut self, t: &'shape Shape<'shape>) -> Self {
        self.t = Some(t);
        self
    }

    /// Builds the SliceDef
    pub const fn build(self) -> SliceDef<'shape> {
        SliceDef {
            vtable: self.vtable.unwrap(),
            t: self.t.unwrap(),
        }
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
#[derive(Clone, Copy, Debug, Hash, PartialEq, Eq)]
#[repr(C)]
#[non_exhaustive]
pub struct SliceVTable {
    /// Number of items in the slice
    pub len: SliceLenFn,
    /// Get pointer to the data buffer of the slice.
    pub as_ptr: SliceAsPtrFn,
    /// Get mutable pointer to the data buffer of the slice.
    pub as_mut_ptr: SliceAsMutPtrFn,
}

impl SliceVTable {
    /// Returns a builder for SliceVTable
    pub const fn builder() -> SliceVTableBuilder {
        SliceVTableBuilder::new()
    }
}

/// Builds a [`SliceVTable`]
pub struct SliceVTableBuilder {
    as_ptr: Option<SliceAsPtrFn>,
    as_mut_ptr: Option<SliceAsMutPtrFn>,
    len: Option<SliceLenFn>,
}

impl SliceVTableBuilder {
    /// Creates a new [`SliceVTableBuilder`] with all fields set to `None`.
    #[allow(clippy::new_without_default)]
    pub const fn new() -> Self {
        Self {
            len: None,
            as_ptr: None,
            as_mut_ptr: None,
        }
    }

    /// Sets the `len` field
    pub const fn len(mut self, f: SliceLenFn) -> Self {
        self.len = Some(f);
        self
    }

    /// Sets the as_ptr field
    pub const fn as_ptr(mut self, f: SliceAsPtrFn) -> Self {
        self.as_ptr = Some(f);
        self
    }

    /// Sets the as_mut_ptr field
    pub const fn as_mut_ptr(mut self, f: SliceAsMutPtrFn) -> Self {
        self.as_mut_ptr = Some(f);
        self
    }

    /// Builds the [`SliceVTable`] from the current state of the builder.
    ///
    /// # Panics
    ///
    /// This method will panic if any of the required fields are `None`.
    pub const fn build(self) -> SliceVTable {
        SliceVTable {
            len: self.len.unwrap(),
            as_ptr: self.as_ptr.unwrap(),
            as_mut_ptr: self.as_mut_ptr.unwrap(),
        }
    }
}
