use super::{PtrConst, PtrMut, PtrUninit};

use super::{IterVTable, Shape};

/// Fields for list types
#[derive(Clone, Copy, Debug)]
#[repr(C)]
pub struct ListDef {
    /// vtable for interacting with the list (can be type-erased/shared)
    pub vtable: &'static ListVTable,

    /// Per-type operations that must be monomorphized (optional).
    ///
    /// If `None`, uses the functions in `vtable`. If `Some`, these operations
    /// are used instead of the corresponding vtable functions, allowing the
    /// main vtable to be shared across all instantiations (e.g., one vtable
    /// for all `Vec<T>`).
    pub type_ops: Option<&'static ListTypeOps>,

    /// shape of the items in the list
    pub t: &'static Shape,
}

impl ListDef {
    /// Construct a `ListDef` from its vtable and element shape.
    pub const fn new(vtable: &'static ListVTable, t: &'static Shape) -> Self {
        Self {
            vtable,
            type_ops: None,
            t,
        }
    }

    /// Construct a `ListDef` with both shared vtable and per-T type operations.
    pub const fn with_type_ops(
        vtable: &'static ListVTable,
        type_ops: &'static ListTypeOps,
        t: &'static Shape,
    ) -> Self {
        Self {
            vtable,
            type_ops: Some(type_ops),
            t,
        }
    }

    /// Returns the shape of the items in the list
    pub const fn t(&self) -> &'static Shape {
        self.t
    }

    /// Returns the init_in_place_with_capacity function, checking type_ops first.
    #[inline]
    pub fn init_in_place_with_capacity(&self) -> Option<ListInitInPlaceWithCapacityFn> {
        self.type_ops
            .and_then(|ops| ops.init_in_place_with_capacity)
    }

    /// Returns the push function, checking type_ops first.
    #[inline]
    pub fn push(&self) -> Option<ListPushFn> {
        self.type_ops.and_then(|ops| ops.push)
    }

    /// Returns the iterator vtable, checking type_ops first.
    ///
    /// Returns `None` if no type_ops is set (no iterator support).
    #[inline]
    pub fn iter_vtable(&self) -> Option<&'static IterVTable<PtrConst>> {
        self.type_ops.map(|ops| &ops.iter_vtable)
    }
}

/// Initialize a list in place with a given capacity
///
/// # Safety
///
/// The `list` parameter must point to uninitialized memory of sufficient size.
/// The function must properly initialize the memory.
pub type ListInitInPlaceWithCapacityFn = unsafe fn(list: PtrUninit, capacity: usize) -> PtrMut;

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
/// The `shape` parameter is the list's shape, allowing type-erased implementations
/// to extract element size from `shape.type_params[0]`.
///
/// # Safety
///
/// The `list` parameter must point to aligned, initialized memory of the correct type.
pub type ListGetFn =
    unsafe fn(list: PtrConst, index: usize, shape: &'static Shape) -> Option<PtrConst>;

/// Get mutable pointer to the element at `index` in the list, or `None` if the
/// index is out of bounds.
///
/// The `shape` parameter is the list's shape, allowing type-erased implementations
/// to extract element size from `shape.type_params[0]`.
///
/// # Safety
///
/// The `list` parameter must point to aligned, initialized memory of the correct type.
pub type ListGetMutFn =
    unsafe fn(list: PtrMut, index: usize, shape: &'static Shape) -> Option<PtrMut>;

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

//////////////////////////////////////////////////////////////////////
// ListTypeOps - Per-type operations that must be monomorphized
//////////////////////////////////////////////////////////////////////

/// Per-type list operations that must be monomorphized.
///
/// These operations cannot be type-erased because they need to know
/// the concrete element type `T` at compile time:
/// - `init_in_place_with_capacity`: Needs to create `Vec<T>::with_capacity`
/// - `push`: Needs to call `Vec<T>::push`
/// - Iterator operations: Need the concrete iterator type
///
/// This struct is used alongside a shared `ListVTable` to separate:
/// - **Shareable operations** (in `ListVTable`): Can be type-erased using
///   runtime shape info (len, get, get_mut, as_ptr, as_mut_ptr)
/// - **Per-T operations** (in `ListTypeOps`): Must be monomorphized
///
/// # Example
///
/// ```ignore
/// // Shared vtable for ALL Vec<T> instantiations
/// static VEC_LIST_VTABLE: ListVTable = ListVTable { ... };
///
/// // Per-T operations for Vec<String>
/// static VEC_STRING_TYPE_OPS: ListTypeOps = ListTypeOps { ... };
/// ```
#[derive(Clone, Copy, Debug)]
#[repr(C)]
pub struct ListTypeOps {
    /// Initialize a list in place with a given capacity (per-T).
    ///
    /// # Safety
    /// The `list` parameter must point to uninitialized memory of sufficient size.
    pub init_in_place_with_capacity: Option<ListInitInPlaceWithCapacityFn>,

    /// Push an item to the list (per-T).
    ///
    /// # Safety
    /// - `list` must point to an initialized list
    /// - `item` must point to an initialized value that will be moved
    pub push: Option<ListPushFn>,

    /// Virtual table for list iterator operations (per-T).
    ///
    /// Iterator operations cannot be type-erased because the iterator state
    /// (`Box<slice::Iter<'_, T>>`) is type-specific.
    pub iter_vtable: IterVTable<PtrConst>,
}

impl ListTypeOps {
    /// Create a new `ListTypeOps` with required iterator vtable.
    pub const fn new(iter_vtable: IterVTable<PtrConst>) -> Self {
        Self {
            init_in_place_with_capacity: None,
            push: None,
            iter_vtable,
        }
    }

    /// Start building a `ListTypeOps`.
    pub const fn builder() -> ListTypeOpsBuilder {
        ListTypeOpsBuilder {
            init_in_place_with_capacity: None,
            push: None,
            iter_vtable: None,
        }
    }
}

/// Builder for `ListTypeOps`.
#[derive(Clone, Copy, Debug)]
pub struct ListTypeOpsBuilder {
    init_in_place_with_capacity: Option<ListInitInPlaceWithCapacityFn>,
    push: Option<ListPushFn>,
    iter_vtable: Option<IterVTable<PtrConst>>,
}

impl ListTypeOpsBuilder {
    /// Set the `init_in_place_with_capacity` function.
    pub const fn init_in_place_with_capacity(mut self, f: ListInitInPlaceWithCapacityFn) -> Self {
        self.init_in_place_with_capacity = Some(f);
        self
    }

    /// Set the `push` function.
    pub const fn push(mut self, f: ListPushFn) -> Self {
        self.push = Some(f);
        self
    }

    /// Set the iterator vtable.
    pub const fn iter_vtable(mut self, vtable: IterVTable<PtrConst>) -> Self {
        self.iter_vtable = Some(vtable);
        self
    }

    /// Build the `ListTypeOps`.
    ///
    /// # Panics
    /// Panics if `iter_vtable` was not set.
    pub const fn build(self) -> ListTypeOps {
        ListTypeOps {
            init_in_place_with_capacity: self.init_in_place_with_capacity,
            push: self.push,
            iter_vtable: match self.iter_vtable {
                Some(vt) => vt,
                None => panic!("ListTypeOps requires iter_vtable to be set"),
            },
        }
    }
}

//////////////////////////////////////////////////////////////////////
// ListVTable - Shareable list operations (can be type-erased)
//////////////////////////////////////////////////////////////////////

vtable_def! {
    /// Virtual table for shareable list operations.
    ///
    /// These operations can be type-erased and shared across all instantiations
    /// of a generic list type (e.g., one vtable for all `Vec<T>`). They work
    /// using runtime shape information to compute element offsets.
    ///
    /// Per-type operations that cannot be shared (init, push, iterator) live
    /// in [`ListTypeOps`].
    #[derive(Clone, Copy, Debug)]
    #[repr(C)]
    pub struct ListVTable + ListVTableBuilder {
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
    }
}
