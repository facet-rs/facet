use super::{PtrConst, PtrMut, PtrUninit};

use super::{IterVTable, Shape};

/// Fields for map types
#[derive(Clone, Copy, Debug)]
#[repr(C)]
pub struct MapDef {
    /// vtable for interacting with the map
    pub vtable: &'static MapVTable,
    /// shape of the keys in the map
    pub k: &'static Shape,
    /// shape of the values in the map
    pub v: &'static Shape,
}

impl MapDef {
    /// Construct a `MapDef` from its vtable and key/value shapes.
    pub const fn new(vtable: &'static MapVTable, k: &'static Shape, v: &'static Shape) -> Self {
        Self { vtable, k, v }
    }

    /// Returns the shape of the keys of the map
    pub const fn k(&self) -> &'static Shape {
        self.k
    }

    /// Returns the shape of the values of the map
    pub const fn v(&self) -> &'static Shape {
        self.v
    }
}

/// Initialize a map in place with a given capacity
///
/// # Safety
///
/// The `map` parameter must point to uninitialized memory of sufficient size.
/// The function must properly initialize the memory.
pub type MapInitInPlaceWithCapacityFn =
    unsafe extern "C" fn(map: PtrUninit, capacity: usize) -> PtrMut;

/// Insert a key-value pair into the map
///
/// # Safety
///
/// The `map` parameter must point to aligned, initialized memory of the correct type.
/// `key` and `value` are moved out of (with [`core::ptr::read`]) — they should be deallocated
/// afterwards (e.g. with [`core::mem::forget`]) but NOT dropped.
pub type MapInsertFn = unsafe extern "C" fn(map: PtrMut, key: PtrMut, value: PtrMut);

/// Insert a key-value pair when the key is available as a borrowed string.
///
/// Returns `true` when the value was consumed. Returning `false` means the value
/// remains initialized at its input pointer and the caller should fall back to
/// another insertion path.
///
/// # Safety
///
/// The `map` parameter must point to aligned, initialized memory of the correct type.
/// `key` must point to a valid `str`. `value` must point to an initialized value
/// of the map's value type.
pub type MapInsertBorrowedStrKeyFn =
    unsafe extern "C" fn(map: PtrMut, key: PtrConst, value: PtrMut) -> bool;

/// Insert a key-value pair when both key and value are available as borrowed strings.
///
/// Returns `true` when the entry was inserted. Returning `false` means neither
/// string was consumed and the caller should fall back to another insertion path.
///
/// # Safety
///
/// The `map` parameter must point to aligned, initialized memory of the correct type.
/// `key` and `value` must each point to a valid `str`.
pub type MapInsertBorrowedStrEntryFn =
    unsafe extern "C" fn(map: PtrMut, key: PtrConst, value: PtrConst) -> bool;

/// Insert a key-value pair when the key has already been decoded as an owned
/// [`alloc::string::String`].
///
/// Returns `true` when the key and value were consumed. Returning `false` means the
/// key and value remain initialized at their input pointers and the caller should
/// fall back to [`MapInsertFn`].
///
/// # Safety
///
/// The `map` parameter must point to aligned, initialized memory of the correct type.
/// `key` must point to an initialized [`alloc::string::String`]. `value` must point to
/// an initialized value of the map's value type.
pub type MapInsertOwnedStringKeyFn =
    unsafe extern "C" fn(map: PtrMut, key: PtrMut, value: PtrMut) -> bool;

/// Get the number of entries in the map
///
/// # Safety
///
/// The `map` parameter must point to aligned, initialized memory of the correct type.
pub type MapLenFn = unsafe extern "C" fn(map: PtrConst) -> usize;

/// Check if the map contains a key
///
/// # Safety
///
/// The `map` parameter must point to aligned, initialized memory of the correct type.
pub type MapContainsKeyFn = unsafe extern "C" fn(map: PtrConst, key: PtrConst) -> bool;

/// Get pointer to a value for a given key, returns None if not found
///
/// # Safety
///
/// The `map` parameter must point to aligned, initialized memory of the correct type.
pub type MapGetValuePtrFn = unsafe extern "C" fn(map: PtrConst, key: PtrConst) -> *const u8;

/// Build a map from a contiguous slice of (K, V) pairs.
///
/// This is an optimization for bulk deserialization that avoids per-entry vtable
/// calls and rehashing by building the map with known capacity in one shot.
///
/// # Safety
///
/// - `map` must point to uninitialized memory of sufficient size for the map.
/// - `pairs_ptr` must point to `count` contiguous (K, V) tuples, properly aligned.
/// - Keys and values are moved out via `ptr::read` - the memory should be deallocated
///   but not dropped afterwards.
pub type MapFromPairSliceFn =
    unsafe extern "C" fn(map: PtrUninit, pairs_ptr: *mut u8, count: usize) -> PtrMut;

vtable_def! {
    /// Virtual table for a Map<K, V>
    #[derive(Clone, Copy, Debug)]
    #[repr(C)]
    pub struct MapVTable + MapVTableBuilder {
        /// cf. [`MapInitInPlaceWithCapacityFn`]
        pub init_in_place_with_capacity: MapInitInPlaceWithCapacityFn,

        /// cf. [`MapInsertFn`]
        pub insert: MapInsertFn,

        /// cf. [`MapInsertBorrowedStrKeyFn`]
        pub insert_borrowed_str_key: Option<MapInsertBorrowedStrKeyFn>,

        /// cf. [`MapInsertBorrowedStrEntryFn`]
        pub insert_borrowed_str_entry: Option<MapInsertBorrowedStrEntryFn>,

        /// cf. [`MapInsertOwnedStringKeyFn`]
        pub insert_owned_string_key: Option<MapInsertOwnedStringKeyFn>,

        /// cf. [`MapLenFn`]
        pub len: MapLenFn,

        /// cf. [`MapContainsKeyFn`]
        pub contains_key: MapContainsKeyFn,

        /// cf. [`MapGetValuePtrFn`]
        pub get_value_ptr: MapGetValuePtrFn,

        /// Virtual table for map iterator operations
        pub iter_vtable: IterVTable<(PtrConst, PtrConst)>,

        /// cf. [`MapFromPairSliceFn`] - optional bulk-construction optimization
        pub from_pair_slice: Option<MapFromPairSliceFn>,

        /// Size of (K, V) tuple in bytes for temporary pair buffers.
        pub pair_stride: usize,

        /// Offset of V within (K, V) tuple for temporary pair buffers.
        pub value_offset_in_pair: usize,
    }
}
