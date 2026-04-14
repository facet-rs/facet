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

    /// Returns the pop function, checking type_ops first.
    #[inline]
    pub fn pop(&self) -> Option<ListPopFn> {
        self.type_ops.and_then(|ops| ops.pop)
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
pub type ListInitInPlaceWithCapacityFn =
    unsafe extern "C" fn(list: PtrUninit, capacity: usize) -> PtrMut;

/// Push an item to the list
///
/// # Safety
///
/// The `list` parameter must point to aligned, initialized memory of the correct type.
/// `item` is moved out of (with [`core::ptr::read`]) — it should be deallocated afterwards (e.g.
/// with [`core::mem::forget`]) but NOT dropped.
/// Note: `item` must be PtrMut (not PtrConst) because ownership is transferred and the value
/// may be dropped later, which requires mutable access.
pub type ListPushFn = unsafe extern "C" fn(list: PtrMut, item: PtrMut);
// FIXME: this forces allocating item separately, copying it, and then dropping it — it's not great.

/// Pop the last item from the list, writing it into `out`.
///
/// Returns `true` if an item was popped (and `out` was written to), `false` if
/// the list was empty (in which case `out` is left uninitialized).
///
/// # Safety
///
/// - `list` must point to aligned, initialized memory of the correct type.
/// - `out` must point to uninitialized memory large enough for one element of
///   the list's element type and with the element's alignment.
pub type ListPopFn = unsafe extern "C" fn(list: PtrMut, out: PtrUninit) -> bool;

/// Swap the elements at indices `a` and `b` in the list.
///
/// Returns `false` if either index is out of bounds (in which case no swap
/// occurs); `true` on success. Swapping an index with itself is a no-op and
/// still returns `true`.
///
/// The `shape` parameter is the list's shape, allowing type-erased
/// implementations to extract the element size from `shape.type_params[0]`.
///
/// # Safety
///
/// The `list` parameter must point to aligned, initialized memory of the
/// correct type.
pub type ListSwapFn = unsafe fn(list: PtrMut, a: usize, b: usize, shape: &'static Shape) -> bool;

/// Get the number of items in the list
///
/// # Safety
///
/// The `list` parameter must point to aligned, initialized memory of the correct type.
pub type ListLenFn = unsafe extern "C" fn(list: PtrConst) -> usize;

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
pub type ListAsPtrFn = unsafe extern "C" fn(list: PtrConst) -> PtrConst;

/// Get mutable pointer to the data buffer of the list.
///
/// # Safety
///
/// The `list` parameter must point to aligned, initialized memory of the correct type.
pub type ListAsMutPtrFn = unsafe extern "C" fn(list: PtrMut) -> PtrMut;

/// Set the length of a list (for direct-fill operations).
///
/// # Safety
///
/// - The `list` parameter must point to aligned, initialized memory of the correct type.
/// - The new length must not exceed the list's capacity.
/// - All elements at indices `0..new_len` must be properly initialized.
/// - For types that are not `Copy`, the caller must ensure no double-drops occur.
pub type ListSetLenFn = unsafe extern "C" fn(list: PtrMut, len: usize);

/// Get raw mutable pointer to the data buffer as `*mut u8`.
///
/// This is used for direct-fill operations where the JIT writes directly
/// into the buffer. Returns the same pointer as `as_mut_ptr` but as raw bytes.
///
/// # Safety
///
/// The `list` parameter must point to aligned, initialized memory of the correct type.
pub type ListAsMutPtrTypedFn = unsafe extern "C" fn(list: PtrMut) -> *mut u8;

/// Reserve capacity for at least `additional` more elements.
///
/// After calling this, the list's capacity will be at least `len + additional`.
/// This may reallocate the buffer, invalidating any previously obtained pointers.
///
/// # Safety
///
/// The `list` parameter must point to aligned, initialized memory of the correct type.
pub type ListReserveFn = unsafe extern "C" fn(list: PtrMut, additional: usize);

/// Get the current capacity of the list.
///
/// # Safety
///
/// The `list` parameter must point to aligned, initialized memory of the correct type.
pub type ListCapacityFn = unsafe extern "C" fn(list: PtrConst) -> usize;

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

    /// Pop the last item from the list (per-T).
    ///
    /// # Safety
    /// - `list` must point to an initialized list
    /// - `out` must point to uninitialized memory large enough for the element
    pub pop: Option<ListPopFn>,

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
            pop: None,
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
            pop: None,
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
    pop: Option<ListPopFn>,
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

    /// Set the `pop` function.
    pub const fn pop(mut self, f: ListPopFn) -> Self {
        self.pop = Some(f);
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
            pop: self.pop,
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

        /// cf. [`ListSwapFn`]
        /// Only available for types that support in-place element swaps
        pub swap: Option<ListSwapFn>,
    }
}
