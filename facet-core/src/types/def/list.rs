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

    /// Returns the set_len function for direct-fill operations.
    #[inline]
    pub fn set_len(&self) -> Option<ListSetLenFn> {
        self.type_ops.and_then(|ops| ops.set_len)
    }

    /// Returns the as_mut_ptr_typed function for direct-fill operations.
    #[inline]
    pub fn as_mut_ptr_typed(&self) -> Option<ListAsMutPtrTypedFn> {
        self.type_ops.and_then(|ops| ops.as_mut_ptr_typed)
    }

    /// Returns the reserve function for direct-fill operations.
    #[inline]
    pub fn reserve(&self) -> Option<ListReserveFn> {
        self.type_ops.and_then(|ops| ops.reserve)
    }

    /// Returns the capacity function for direct-fill operations.
    #[inline]
    pub fn capacity(&self) -> Option<ListCapacityFn> {
        self.type_ops.and_then(|ops| ops.capacity)
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
pub type ListPushFn = unsafe fn(list: PtrMut, item: PtrConst);
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

/// Set the length of a list (for direct-fill operations).
///
/// # Safety
///
/// - The `list` parameter must point to aligned, initialized memory of the correct type.
/// - The new length must not exceed the list's capacity.
/// - All elements at indices `0..new_len` must be properly initialized.
/// - For types that are not `Copy`, the caller must ensure no double-drops occur.
pub type ListSetLenFn = unsafe fn(list: PtrMut, len: usize);

/// Get raw mutable pointer to the data buffer as `*mut u8`.
///
/// This is used for direct-fill operations where the JIT writes directly
/// into the buffer. Returns the same pointer as `as_mut_ptr` but as raw bytes.
///
/// # Safety
///
/// The `list` parameter must point to aligned, initialized memory of the correct type.
pub type ListAsMutPtrTypedFn = unsafe fn(list: PtrMut) -> *mut u8;

/// Reserve capacity for at least `additional` more elements.
///
/// After calling this, the list's capacity will be at least `len + additional`.
/// This may reallocate the buffer, invalidating any previously obtained pointers.
///
/// # Safety
///
/// The `list` parameter must point to aligned, initialized memory of the correct type.
pub type ListReserveFn = unsafe fn(list: PtrMut, additional: usize);

/// Get the current capacity of the list.
///
/// # Safety
///
/// The `list` parameter must point to aligned, initialized memory of the correct type.
pub type ListCapacityFn = unsafe fn(list: PtrConst) -> usize;

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

    /// Set the length of the list (per-T, for direct-fill operations).
    ///
    /// # Safety
    /// - `list` must point to an initialized list
    /// - `len` must not exceed the list's capacity
    /// - All elements at indices `0..len` must be properly initialized
    pub set_len: Option<ListSetLenFn>,

    /// Get raw mutable pointer to the data buffer (per-T, for direct-fill).
    ///
    /// # Safety
    /// - `list` must point to an initialized list
    pub as_mut_ptr_typed: Option<ListAsMutPtrTypedFn>,

    /// Reserve capacity for additional elements (per-T, for direct-fill).
    ///
    /// # Safety
    /// - `list` must point to an initialized list
    pub reserve: Option<ListReserveFn>,

    /// Get current capacity (per-T, for direct-fill).
    ///
    /// # Safety
    /// - `list` must point to an initialized list
    pub capacity: Option<ListCapacityFn>,

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
            set_len: None,
            as_mut_ptr_typed: None,
            reserve: None,
            capacity: None,
            iter_vtable,
        }
    }

    /// Start building a `ListTypeOps`.
    pub const fn builder() -> ListTypeOpsBuilder {
        ListTypeOpsBuilder {
            init_in_place_with_capacity: None,
            push: None,
            set_len: None,
            as_mut_ptr_typed: None,
            reserve: None,
            capacity: None,
            iter_vtable: None,
        }
    }
}

/// Builder for `ListTypeOps`.
#[derive(Clone, Copy, Debug)]
pub struct ListTypeOpsBuilder {
    init_in_place_with_capacity: Option<ListInitInPlaceWithCapacityFn>,
    push: Option<ListPushFn>,
    set_len: Option<ListSetLenFn>,
    as_mut_ptr_typed: Option<ListAsMutPtrTypedFn>,
    reserve: Option<ListReserveFn>,
    capacity: Option<ListCapacityFn>,
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

    /// Set the `set_len` function (for direct-fill operations).
    pub const fn set_len(mut self, f: ListSetLenFn) -> Self {
        self.set_len = Some(f);
        self
    }

    /// Set the `as_mut_ptr_typed` function (for direct-fill operations).
    pub const fn as_mut_ptr_typed(mut self, f: ListAsMutPtrTypedFn) -> Self {
        self.as_mut_ptr_typed = Some(f);
        self
    }

    /// Set the `reserve` function (for direct-fill operations).
    pub const fn reserve(mut self, f: ListReserveFn) -> Self {
        self.reserve = Some(f);
        self
    }

    /// Set the `capacity` function (for direct-fill operations).
    pub const fn capacity(mut self, f: ListCapacityFn) -> Self {
        self.capacity = Some(f);
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
            set_len: self.set_len,
            as_mut_ptr_typed: self.as_mut_ptr_typed,
            reserve: self.reserve,
            capacity: self.capacity,
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
