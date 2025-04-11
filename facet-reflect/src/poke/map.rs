use crate::PokeValueUninit;
use facet_core::{MapDef, MapVTable, Opaque, OpaqueConst, OpaqueUninit, Shape};

/// Allows initializing an uninitialized map
pub struct PokeMapUninit<'mem> {
    data: OpaqueUninit<'mem>,
    shape: &'static Shape,
    def: MapDef,
}

impl<'mem> PokeMapUninit<'mem> {
    #[inline(always)]
    /// Coerce back into a `PokeValue`
    pub fn into_value(self) -> PokeValueUninit<'mem> {
        unsafe { PokeValueUninit::new(self.data, self.shape) }
    }

    #[inline(always)]
    /// Shape getter
    pub fn shape(&self) -> &'static Shape {
        self.shape
    }
    /// Creates a new uninitialized map write-proxy
    ///
    /// # Safety
    ///
    /// The data buffer must match the size and alignment of the shape.
    pub(crate) unsafe fn new(data: OpaqueUninit<'mem>, shape: &'static Shape, def: MapDef) -> Self {
        Self { data, shape, def }
    }

    /// Initializes the map with an optional size hint
    pub fn init(self, size_hint: Option<usize>) -> Result<PokeMap<'mem>, OpaqueUninit<'mem>> {
        let res = if let Some(capacity) = size_hint {
            let init_in_place_with_capacity = self.def.vtable.init_in_place_with_capacity_fn;
            unsafe { init_in_place_with_capacity(self.data, capacity) }
        } else {
            let pv = unsafe { PokeValueUninit::new(self.data, self.shape) };
            pv.default_in_place().map_err(|_| ())
        };
        let data = res.map_err(|_| self.data)?;
        Ok(unsafe { PokeMap::new(data, self.shape, self.def) })
    }
}

/// Allows poking a map (inserting, etc.)
pub struct PokeMap<'mem> {
    data: Opaque<'mem>,
    #[allow(dead_code)]
    shape: &'static Shape,
    def: MapDef,
}

impl<'mem> PokeMap<'mem> {
    /// Creates a value-proxy for a map
    ///
    /// # Safety
    ///
    /// The data buffer must match the size and alignment of the shape.
    #[inline]
    pub(crate) unsafe fn new(data: Opaque<'mem>, shape: &'static Shape, def: MapDef) -> Self {
        Self { data, shape, def }
    }

    /// Get the shape of the map
    #[inline(always)]
    pub fn shape(&self) -> &'static Shape {
        self.shape
    }

    /// Gets the vtable for the map
    #[inline(always)]
    pub fn map_vtable(&self) -> &'static MapVTable {
        self.def.vtable
    }

    /// Inserts a key-value pair into the map
    ///
    /// # Safety
    ///
    /// `key` and `value` are moved out of (with [`core::ptr::read`]) — they should be deallocated
    /// afterwards but NOT dropped.
    #[inline]
    pub unsafe fn insert(&mut self, key: Opaque<'_>, value: Opaque<'_>) {
        unsafe { (self.map_vtable().insert_fn)(self.data, key, value) }
    }

    /// Gets the number of entries in the map
    #[inline]
    pub fn len(&self) -> usize {
        unsafe { (self.map_vtable().len_fn)(self.data.as_const()) }
    }

    /// Checks if the map contains no entries
    #[inline]
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// Checks if the map contains a key
    #[inline]
    pub fn contains_key(&self, key: OpaqueConst<'_>) -> bool {
        unsafe { (self.map_vtable().contains_key_fn)(self.data.as_const(), key) }
    }

    /// Gets a pointer to the value for a given key
    ///
    /// Returns `None` if the key is not found.
    #[inline]
    pub fn get_value_ptr<'key>(&self, key: OpaqueConst<'key>) -> Option<OpaqueConst<'mem>> {
        unsafe { (self.map_vtable().get_value_ptr_fn)(self.data.as_const(), key) }
    }

    /// Takes ownership of this `PokeList` and returns the underlying data.
    pub fn build_in_place(self) -> Opaque<'mem> {
        self.data
    }

    /// Returns a reference to the `MapDef` of this `PokeMap`.
    #[inline]
    pub fn def(&self) -> &MapDef {
        &self.def
    }
}
