use crate::ptr::{PtrConst, PtrMut, PtrUninit};

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
    for<'mem> unsafe fn(map: PtrUninit<'mem>, capacity: usize) -> PtrMut<'mem>;

/// Insert a key-value pair into the map
///
/// # Safety
///
/// The `map` parameter must point to aligned, initialized memory of the correct type.
/// `key` and `value` are moved out of (with [`core::ptr::read`]) — they should be deallocated
/// afterwards (e.g. with [`core::mem::forget`]) but NOT dropped.
pub type MapInsertFn =
    for<'map, 'key, 'value> unsafe fn(map: PtrMut<'map>, key: PtrMut<'key>, value: PtrMut<'value>);

/// Get the number of entries in the map
///
/// # Safety
///
/// The `map` parameter must point to aligned, initialized memory of the correct type.
pub type MapLenFn = for<'map> unsafe fn(map: PtrConst<'map>) -> usize;

/// Check if the map contains a key
///
/// # Safety
///
/// The `map` parameter must point to aligned, initialized memory of the correct type.
pub type MapContainsKeyFn =
    for<'map, 'key> unsafe fn(map: PtrConst<'map>, key: PtrConst<'key>) -> bool;

/// Get pointer to a value for a given key, returns None if not found
///
/// # Safety
///
/// The `map` parameter must point to aligned, initialized memory of the correct type.
pub type MapGetValuePtrFn =
    for<'map, 'key> unsafe fn(map: PtrConst<'map>, key: PtrConst<'key>) -> Option<PtrConst<'map>>;

/// Virtual table for a Map<K, V>
#[derive(Clone, Copy, Debug)]
#[repr(C)]
pub struct MapVTable {
    /// cf. [`MapInitInPlaceWithCapacityFn`]
    pub init_in_place_with_capacity_fn: MapInitInPlaceWithCapacityFn,

    /// cf. [`MapInsertFn`]
    pub insert_fn: MapInsertFn,

    /// cf. [`MapLenFn`]
    pub len_fn: MapLenFn,

    /// cf. [`MapContainsKeyFn`]
    pub contains_key_fn: MapContainsKeyFn,

    /// cf. [`MapGetValuePtrFn`]
    pub get_value_ptr_fn: MapGetValuePtrFn,

    /// Virtual table for map iterator operations
    pub iter_vtable: IterVTable<(PtrConst<'static>, PtrConst<'static>)>,
}

impl MapVTable {
    /// Const ctor; all map vtable hooks must be provided.
    pub const fn new(
        init_in_place_with_capacity_fn: MapInitInPlaceWithCapacityFn,
        insert_fn: MapInsertFn,
        len_fn: MapLenFn,
        contains_key_fn: MapContainsKeyFn,
        get_value_ptr_fn: MapGetValuePtrFn,
        iter_vtable: IterVTable<(PtrConst<'static>, PtrConst<'static>)>,
    ) -> Self {
        Self {
            init_in_place_with_capacity_fn,
            insert_fn,
            len_fn,
            contains_key_fn,
            get_value_ptr_fn,
            iter_vtable,
        }
    }
}
