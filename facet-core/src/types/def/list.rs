use crate::ptr::{PtrConst, PtrMut, PtrUninit};

use super::{IterVTable, Shape};

/// Fields for list types
#[derive(Clone, Copy, Debug)]
#[repr(C)]
pub struct ListDef {
    /// vtable for interacting with the list
    pub vtable: &'static ListVTable,
    /// shape of the items in the list
    pub t: &'static Shape,
}

impl ListDef {
    /// Construct a `ListDef` from its vtable and element shape.
    pub const fn new(vtable: &'static ListVTable, t: &'static Shape) -> Self {
        Self { vtable, t }
    }

    /// Returns the shape of the items in the list
    pub const fn t(&self) -> &'static Shape {
        self.t
    }
}

/// Initialize a list in place with a given capacity
///
/// # Safety
///
/// The `list` parameter must point to uninitialized memory of sufficient size.
/// The function must properly initialize the memory.
pub type ListInitInPlaceWithCapacityFn =
    for<'mem> unsafe fn(list: PtrUninit<'mem>, capacity: usize) -> PtrMut<'mem>;

/// Push an item to the list
///
/// # Safety
///
/// The `list` parameter must point to aligned, initialized memory of the correct type.
/// `item` is moved out of (with [`core::ptr::read`]) — it should be deallocated afterwards (e.g.
/// with [`core::mem::forget`]) but NOT dropped.
pub type ListPushFn = unsafe fn(list: PtrMut, item: PtrMut);
// FIXME: this forces allocating item separately, copying it, and then dropping it — it's not great.

/// Get the number of items in the list
///
/// # Safety
///
/// The `list` parameter must point to aligned, initialized memory of the correct type.
pub type ListLenFn = unsafe fn(list: PtrConst) -> usize;

/// Get pointer to the element at `index` in the list, or `None` if the
/// index is out of bounds.
///
/// # Safety
///
/// The `list` parameter must point to aligned, initialized memory of the correct type.
pub type ListGetFn = unsafe fn(list: PtrConst, index: usize) -> Option<PtrConst>;

/// Get mutable pointer to the element at `index` in the list, or `None` if the
/// index is out of bounds.
///
/// # Safety
///
/// The `list` parameter must point to aligned, initialized memory of the correct type.
pub type ListGetMutFn = unsafe fn(list: PtrMut, index: usize) -> Option<PtrMut>;

/// Get pointer to the data buffer of the list.
///
/// # Safety
///
/// The `list` parameter must point to aligned, initialized memory of the correct type.
pub type ListAsPtrFn = unsafe fn(list: PtrConst) -> PtrConst;

/// Get mutable pointer to the data buffer of the list.
///
/// # Safety
///
/// The `list` parameter must point to aligned, initialized memory of the correct type.
pub type ListAsMutPtrFn = unsafe fn(list: PtrMut) -> PtrMut;

/// Virtual table for a list-like type (like `Vec<T>`)
#[derive(Clone, Copy, Debug)]
#[repr(C)]
pub struct ListVTable {
    /// cf. [`ListInitInPlaceWithCapacityFn`].
    /// Unbuildable lists exist, like arrays.
    pub init_in_place_with_capacity: Option<ListInitInPlaceWithCapacityFn>,

    /// cf. [`ListPushFn`]
    /// Only available for mutable lists
    pub push: Option<ListPushFn>,

    /// cf. [`ListLenFn`]
    pub len: ListLenFn,

    /// cf. [`ListGetFn`]
    pub get: ListGetFn,

    /// cf. [`ListGetMutFn`]
    /// Only available for mutable lists
    pub get_mut: Option<ListGetMutFn>,

    /// cf. [`ListAsPtrFn`]
    /// Only available for types that can be accessed as a contiguous array
    pub as_ptr: Option<ListAsPtrFn>,

    /// cf. [`ListAsMutPtrFn`]
    /// Only available for types that can be accessed as a contiguous array
    pub as_mut_ptr: Option<ListAsMutPtrFn>,

    /// Virtual table for list iterator operations
    pub iter_vtable: IterVTable<PtrConst<'static>>,
}

impl ListVTable {}
