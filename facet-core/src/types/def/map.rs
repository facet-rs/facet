use crate::ptr::{PtrConst, PtrMut, PtrUninit};

use super::{IterVTable, Shape};

/// Fields for map types
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
#[repr(C)]
#[non_exhaustive]
pub struct MapDef<'shape> {
    /// vtable for interacting with the map
    pub vtable: &'shape MapVTable,
    /// shape of the keys in the map
    pub k: fn() -> &'shape Shape<'shape>,
    /// shape of the values in the map
    pub v: fn() -> &'shape Shape<'shape>,
}

impl<'shape> MapDef<'shape> {
    /// Returns a builder for MapDef
    pub const fn builder() -> MapDefBuilder<'shape> {
        MapDefBuilder::new()
    }

    /// Returns the shape of the keys of the map
    pub fn k(&self) -> &'shape Shape<'shape> {
        (self.k)()
    }

    /// Returns the shape of the values of the map
    pub fn v(&self) -> &'shape Shape<'shape> {
        (self.v)()
    }
}

/// Builder for MapDef
pub struct MapDefBuilder<'shape> {
    vtable: Option<&'shape MapVTable>,
    k: Option<fn() -> &'shape Shape<'shape>>,
    v: Option<fn() -> &'shape Shape<'shape>>,
}

impl<'shape> MapDefBuilder<'shape> {
    /// Creates a new MapDefBuilder
    #[allow(clippy::new_without_default)]
    pub const fn new() -> Self {
        Self {
            vtable: None,
            k: None,
            v: None,
        }
    }

    /// Sets the vtable for the MapDef
    pub const fn vtable(mut self, vtable: &'shape MapVTable) -> Self {
        self.vtable = Some(vtable);
        self
    }

    /// Sets the key shape for the MapDef
    pub const fn k(mut self, k: fn() -> &'shape Shape<'shape>) -> Self {
        self.k = Some(k);
        self
    }

    /// Sets the value shape for the MapDef
    pub const fn v(mut self, v: fn() -> &'shape Shape<'shape>) -> Self {
        self.v = Some(v);
        self
    }

    /// Builds the MapDef
    pub const fn build(self) -> MapDef<'shape> {
        MapDef {
            vtable: self.vtable.unwrap(),
            k: self.k.unwrap(),
            v: self.v.unwrap(),
        }
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
#[derive(Clone, Copy, Debug, Hash, PartialEq, Eq)]
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
    /// Returns a builder for MapVTable
    pub const fn builder() -> MapVTableBuilder {
        MapVTableBuilder::new()
    }
}

/// Builds a [`MapVTable`]
pub struct MapVTableBuilder {
    init_in_place_with_capacity_fn: Option<MapInitInPlaceWithCapacityFn>,
    insert_fn: Option<MapInsertFn>,
    len_fn: Option<MapLenFn>,
    contains_key_fn: Option<MapContainsKeyFn>,
    get_value_ptr_fn: Option<MapGetValuePtrFn>,
    iter_vtable: Option<IterVTable<(PtrConst<'static>, PtrConst<'static>)>>,
}

impl MapVTableBuilder {
    /// Creates a new [`MapVTableBuilder`] with all fields set to `None`.
    #[allow(clippy::new_without_default)]
    pub const fn new() -> Self {
        Self {
            init_in_place_with_capacity_fn: None,
            insert_fn: None,
            len_fn: None,
            contains_key_fn: None,
            get_value_ptr_fn: None,
            iter_vtable: None,
        }
    }

    /// Sets the init_in_place_with_capacity_fn field
    pub const fn init_in_place_with_capacity(mut self, f: MapInitInPlaceWithCapacityFn) -> Self {
        self.init_in_place_with_capacity_fn = Some(f);
        self
    }

    /// Sets the insert_fn field
    pub const fn insert(mut self, f: MapInsertFn) -> Self {
        self.insert_fn = Some(f);
        self
    }

    /// Sets the len_fn field
    pub const fn len(mut self, f: MapLenFn) -> Self {
        self.len_fn = Some(f);
        self
    }

    /// Sets the contains_key_fn field
    pub const fn contains_key(mut self, f: MapContainsKeyFn) -> Self {
        self.contains_key_fn = Some(f);
        self
    }

    /// Sets the get_value_ptr_fn field
    pub const fn get_value_ptr(mut self, f: MapGetValuePtrFn) -> Self {
        self.get_value_ptr_fn = Some(f);
        self
    }

    /// Sets the iter_vtable field
    pub const fn iter_vtable(
        mut self,
        vtable: IterVTable<(PtrConst<'static>, PtrConst<'static>)>,
    ) -> Self {
        self.iter_vtable = Some(vtable);
        self
    }

    /// Builds the [`MapVTable`] from the current state of the builder.
    ///
    /// # Panics
    ///
    /// This method will panic if any of the required fields are `None`.
    pub const fn build(self) -> MapVTable {
        MapVTable {
            init_in_place_with_capacity_fn: self.init_in_place_with_capacity_fn.unwrap(),
            insert_fn: self.insert_fn.unwrap(),
            len_fn: self.len_fn.unwrap(),
            contains_key_fn: self.contains_key_fn.unwrap(),
            get_value_ptr_fn: self.get_value_ptr_fn.unwrap(),
            iter_vtable: self.iter_vtable.unwrap(),
        }
    }
}
