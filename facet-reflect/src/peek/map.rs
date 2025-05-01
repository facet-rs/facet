use facet_core::{MapDef, PtrConst, PtrMut};

use super::Peek;

/// Iterator over key-value pairs in a `PeekMap`
pub struct PeekMapIter<'mem, 'facet_lifetime> {
    map: PeekMap<'mem, 'facet_lifetime>,
    iter: PtrMut<'mem>,
}

impl<'mem, 'facet_lifetime> Iterator for PeekMapIter<'mem, 'facet_lifetime> {
    type Item = (Peek<'mem, 'facet_lifetime>, Peek<'mem, 'facet_lifetime>);

    fn next(&mut self) -> Option<Self::Item> {
        unsafe {
            let next = (self.map.def.vtable.iter_vtable.next)(self.iter);
            next.map(|(key_ptr, value_ptr)| {
                (
                    Peek::unchecked_new(key_ptr, self.map.def.k()),
                    Peek::unchecked_new(value_ptr, self.map.def.v()),
                )
            })
        }
    }
}

impl Drop for PeekMapIter<'_, '_> {
    fn drop(&mut self) {
        unsafe { (self.map.def.vtable.iter_vtable.dealloc)(self.iter) }
    }
}

impl<'mem, 'facet_lifetime> IntoIterator for &'mem PeekMap<'mem, 'facet_lifetime> {
    type Item = (Peek<'mem, 'facet_lifetime>, Peek<'mem, 'facet_lifetime>);
    type IntoIter = PeekMapIter<'mem, 'facet_lifetime>;

    fn into_iter(self) -> Self::IntoIter {
        self.iter()
    }
}

/// Lets you read from a map (implements read-only [`facet_core::MapVTable`] proxies)
#[derive(Clone, Copy)]
pub struct PeekMap<'mem, 'facet_lifetime> {
    pub(crate) value: Peek<'mem, 'facet_lifetime>,

    pub(crate) def: MapDef,
}

impl core::fmt::Debug for PeekMap<'_, '_> {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("PeekMap").finish_non_exhaustive()
    }
}

impl<'mem, 'facet_lifetime> PeekMap<'mem, 'facet_lifetime> {
    /// Constructor
    pub fn new(value: Peek<'mem, 'facet_lifetime>, def: MapDef) -> Self {
        Self { value, def }
    }

    /// Get the number of entries in the map
    pub fn len(&self) -> usize {
        unsafe { (self.def.vtable.len_fn)(self.value.data()) }
    }

    /// Returns true if the map is empty
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// Check if the map contains a key
    pub fn contains_key(&self, key: &impl facet_core::Facet<'facet_lifetime>) -> bool {
        unsafe {
            let key_ptr = PtrConst::new(key);
            (self.def.vtable.contains_key_fn)(self.value.data(), key_ptr)
        }
    }

    /// Get a value from the map for the given key
    pub fn get<'k>(
        &self,
        key: &'k impl facet_core::Facet<'facet_lifetime>,
    ) -> Option<Peek<'mem, 'facet_lifetime>> {
        unsafe {
            let key_ptr = PtrConst::new(key);
            let value_ptr = (self.def.vtable.get_value_ptr_fn)(self.value.data(), key_ptr)?;
            Some(Peek::unchecked_new(value_ptr, self.def.v()))
        }
    }

    /// Returns an iterator over the key-value pairs in the map
    pub fn iter(self) -> PeekMapIter<'mem, 'facet_lifetime> {
        let iter = unsafe { (self.def.vtable.iter_fn)(self.value.data()) };
        PeekMapIter { map: self, iter }
    }

    /// Def getter
    pub fn def(&self) -> MapDef {
        self.def
    }
}
