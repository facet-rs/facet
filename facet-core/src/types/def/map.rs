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
pub type MapInitInPlaceWithCapacityFn = unsafe fn(map: PtrUninit, capacity: usize) -> PtrMut;

/// Insert a key-value pair into the map
///
/// # Safety
///
/// The `map` parameter must point to aligned, initialized memory of the correct type.
/// `key` and `value` are moved out of (with [`core::ptr::read`]) — they should be deallocated
/// afterwards (e.g. with [`core::mem::forget`]) but NOT dropped.
pub type MapInsertFn = unsafe fn(map: PtrMut, key: PtrMut, value: PtrMut);

/// Get the number of entries in the map
///
/// # Safety
///
/// The `map` parameter must point to aligned, initialized memory of the correct type.
pub type MapLenFn = unsafe fn(map: PtrConst) -> usize;

/// Check if the map contains a key
///
/// # Safety
///
/// The `map` parameter must point to aligned, initialized memory of the correct type.
pub type MapContainsKeyFn = unsafe fn(map: PtrConst, key: PtrConst) -> bool;

/// Get pointer to a value for a given key, returns None if not found
///
/// # Safety
///
/// The `map` parameter must point to aligned, initialized memory of the correct type.
pub type MapGetValuePtrFn = unsafe fn(map: PtrConst, key: PtrConst) -> Option<PtrConst>;

/// Build a map from a contiguous slice of (K, V) pairs.
///
/// This is an optimization for JIT deserialization that avoids per-entry vtable
/// calls and rehashing by building the map with known capacity in one shot.
///
/// # Safety
///
/// - `map` must point to uninitialized memory of sufficient size for the map.
/// - `pairs_ptr` must point to `count` contiguous (K, V) tuples, properly aligned.
/// - Keys and values are moved out via `ptr::read` - the memory should be deallocated
///   but not dropped afterwards.
pub type MapFromPairSliceFn = unsafe fn(map: PtrUninit, pairs_ptr: *mut u8, count: usize) -> PtrMut;

vtable_def! {
    /// Virtual table for a Map<K, V>
    #[derive(Clone, Copy, Debug)]
    #[repr(C)]
    pub struct MapVTable + MapVTableBuilder {
        /// cf. [`MapInitInPlaceWithCapacityFn`]
        pub init_in_place_with_capacity: MapInitInPlaceWithCapacityFn,

        /// cf. [`MapInsertFn`]
        pub insert: MapInsertFn,

        /// cf. [`MapLenFn`]
        pub len: MapLenFn,

        /// cf. [`MapContainsKeyFn`]
        pub contains_key: MapContainsKeyFn,

        /// cf. [`MapGetValuePtrFn`]
        pub get_value_ptr: MapGetValuePtrFn,

        /// Virtual table for map iterator operations
        pub iter_vtable: IterVTable<(PtrConst, PtrConst)>,

        /// cf. [`MapFromPairSliceFn`] - optional optimization for JIT
        pub from_pair_slice: Option<MapFromPairSliceFn>,

        /// Size of (K, V) tuple in bytes (for JIT buffer allocation)
        pub pair_stride: usize,

        /// Offset of V within (K, V) tuple (for JIT value placement)
        pub value_offset_in_pair: usize,
    }
}
