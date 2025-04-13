use facet_core::{MapDef, MapVTable, Shape};

use super::PokeValue;

/// Allows poking a map (inserting, etc.)
pub struct PokeMap<'mem> {
    pub(crate) value: PokeValue<'mem>,

    pub(crate) def: MapDef,
}

impl PokeMap<'_> {
    /// Get the shape of the map
    #[inline(always)]
    pub fn shape(&self) -> &'static Shape {
        self.value.shape
    }

    /// Gets the vtable for the map
    #[inline(always)]
    pub fn map_vtable(&self) -> &'static MapVTable {
        self.def.vtable
    }

    // /// Inserts a key-value pair into the map
    // ///
    // /// # Safety
    // ///
    // /// `key` and `value` are moved out of (with [`core::ptr::read`]) — they should be deallocated
    // /// afterwards but NOT dropped.
    // #[inline]
    // pub unsafe fn insert(&mut self, key: Opaque<'_>, value: Opaque<'_>) {
    //     unsafe { (self.map_vtable().insert_fn)(self.data, key, value) }
    // }

    // /// Gets the number of entries in the map
    // #[inline]
    // pub fn len(&self) -> usize {
    //     unsafe { (self.map_vtable().len_fn)(self.data.as_const()) }
    // }

    // /// Checks if the map contains no entries
    // #[inline]
    // pub fn is_empty(&self) -> bool {
    //     self.len() == 0
    // }

    // /// Checks if the map contains a key
    // #[inline]
    // pub fn contains_key(&self, key: OpaqueConst<'_>) -> bool {
    //     unsafe { (self.map_vtable().contains_key_fn)(self.data.as_const(), key) }
    // }

    // /// Gets a pointer to the value for a given key
    // ///
    // /// Returns `None` if the key is not found.
    // #[inline]
    // pub fn get_value_ptr<'key>(&self, key: OpaqueConst<'key>) -> Option<OpaqueConst<'mem>> {
    //     unsafe { (self.map_vtable().get_value_ptr_fn)(self.data.as_const(), key) }
    // }

    // /// Takes ownership of this `PokeList` and returns the underlying data.
    // pub fn build_in_place(self) -> Opaque<'mem> {
    //     self.data
    // }

    // /// Returns a reference to the `MapDef` of this `PokeMap`.
    // #[inline]
    // pub fn def(&self) -> &MapDef {
    //     &self.def
    // }
}
