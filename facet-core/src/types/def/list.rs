use crate::ptr::{PtrConst, PtrMut, PtrUninit};

use super::{IterVTable, Shape};

/// Fields for list types
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
#[repr(C)]
#[non_exhaustive]
pub struct ListDef<'shape> {
    /// vtable for interacting with the list
    pub vtable: &'shape ListVTable,
    /// shape of the items in the list
    pub t: fn() -> &'shape Shape<'shape>,
}

impl<'shape> ListDef<'shape> {
    /// Returns a builder for ListDef
    pub const fn builder() -> ListDefBuilder<'shape> {
        ListDefBuilder::new()
    }

    /// Returns the shape of the items in the list
    pub fn t(&self) -> &'shape Shape<'shape> {
        (self.t)()
    }
}

/// Builder for ListDef
pub struct ListDefBuilder<'shape> {
    vtable: Option<&'shape ListVTable>,
    t: Option<fn() -> &'shape Shape<'shape>>,
}

impl<'shape> ListDefBuilder<'shape> {
    /// Creates a new ListDefBuilder
    #[allow(clippy::new_without_default)]
    pub const fn new() -> Self {
        Self {
            vtable: None,
            t: None,
        }
    }

    /// Sets the vtable for the ListDef
    pub const fn vtable(mut self, vtable: &'shape ListVTable) -> Self {
        self.vtable = Some(vtable);
        self
    }

    /// Sets the item shape for the ListDef
    pub const fn t(mut self, t: fn() -> &'shape Shape<'shape>) -> Self {
        self.t = Some(t);
        self
    }

    /// Builds the ListDef
    pub const fn build(self) -> ListDef<'shape> {
        ListDef {
            vtable: self.vtable.unwrap(),
            t: self.t.unwrap(),
        }
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
#[derive(Clone, Copy, Debug, Hash, PartialEq, Eq)]
#[repr(C)]
#[non_exhaustive]
pub struct ListVTable {
    /// cf. [`ListInitInPlaceWithCapacityFn`].
    /// Unbuildable lists exist, like arrays.
    pub init_in_place_with_capacity: Option<ListInitInPlaceWithCapacityFn>,

    /// cf. [`ListPushFn`]
    pub push: ListPushFn,

    /// cf. [`ListLenFn`]
    pub len: ListLenFn,

    /// cf. [`ListGetFn`]
    pub get: ListGetFn,

    /// cf. [`ListGetMutFn`]
    pub get_mut: ListGetMutFn,

    /// cf. [`ListAsPtrFn`]
    /// Only available for types that can be accessed as a contiguous array
    pub as_ptr: Option<ListAsPtrFn>,

    /// cf. [`ListAsMutPtrFn`]
    /// Only available for types that can be accessed as a contiguous array
    pub as_mut_ptr: Option<ListAsMutPtrFn>,

    /// Virtual table for list iterator operations
    pub iter_vtable: IterVTable<PtrConst<'static>>,
}

impl ListVTable {
    /// Returns a builder for ListVTable
    pub const fn builder() -> ListVTableBuilder {
        ListVTableBuilder::new()
    }
}

/// Builds a [`ListVTable`]
pub struct ListVTableBuilder {
    init_in_place_with_capacity: Option<ListInitInPlaceWithCapacityFn>,
    push: Option<ListPushFn>,
    len: Option<ListLenFn>,
    get: Option<ListGetFn>,
    get_mut: Option<ListGetMutFn>,
    as_ptr: Option<ListAsPtrFn>,
    as_mut_ptr: Option<ListAsMutPtrFn>,
    iter_vtable: Option<IterVTable<PtrConst<'static>>>,
}

impl ListVTableBuilder {
    /// Creates a new [`ListVTableBuilder`] with all fields set to `None`.
    #[allow(clippy::new_without_default)]
    pub const fn new() -> Self {
        Self {
            init_in_place_with_capacity: None,
            push: None,
            len: None,
            get: None,
            get_mut: None,
            as_ptr: None,
            as_mut_ptr: None,
            iter_vtable: None,
        }
    }

    /// Sets the init_in_place_with_capacity field
    pub const fn init_in_place_with_capacity(mut self, f: ListInitInPlaceWithCapacityFn) -> Self {
        self.init_in_place_with_capacity = Some(f);
        self
    }

    /// Sets the push field
    pub const fn push(mut self, f: ListPushFn) -> Self {
        self.push = Some(f);
        self
    }

    /// Sets the len field
    pub const fn len(mut self, f: ListLenFn) -> Self {
        self.len = Some(f);
        self
    }

    /// Sets the get field
    pub const fn get(mut self, f: ListGetFn) -> Self {
        self.get = Some(f);
        self
    }

    /// Sets the get_mut field
    pub const fn get_mut(mut self, f: ListGetMutFn) -> Self {
        self.get_mut = Some(f);
        self
    }

    /// Sets the as_ptr field
    pub const fn as_ptr(mut self, f: ListAsPtrFn) -> Self {
        self.as_ptr = Some(f);
        self
    }

    /// Sets the as_mut_ptr field
    pub const fn as_mut_ptr(mut self, f: ListAsMutPtrFn) -> Self {
        self.as_mut_ptr = Some(f);
        self
    }

    /// Sets the iter_vtable field
    pub const fn iter_vtable(mut self, vtable: IterVTable<PtrConst<'static>>) -> Self {
        self.iter_vtable = Some(vtable);
        self
    }

    /// Builds the [`ListVTable`] from the current state of the builder.
    ///
    /// # Panics
    ///
    /// This method will panic if any of the required fields are `None`.
    pub const fn build(self) -> ListVTable {
        ListVTable {
            init_in_place_with_capacity: self.init_in_place_with_capacity,
            push: self.push.unwrap(),
            len: self.len.unwrap(),
            get: self.get.unwrap(),
            get_mut: self.get_mut.unwrap(),
            as_ptr: self.as_ptr,
            as_mut_ptr: self.as_mut_ptr,
            iter_vtable: self.iter_vtable.unwrap(),
        }
    }
}
