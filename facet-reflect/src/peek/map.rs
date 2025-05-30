use facet_core::{MapDef, PtrConst, PtrMut};

use super::Peek;

/// Iterator over key-value pairs in a `PeekMap`
pub struct PeekMapIter<'mem, 'facet, 'shape> {
    map: PeekMap<'mem, 'facet, 'shape>,
    iter: PtrMut<'mem>,
}

impl<'mem, 'facet, 'shape> Iterator for PeekMapIter<'mem, 'facet, 'shape> {
    type Item = (Peek<'mem, 'facet, 'shape>, Peek<'mem, 'facet, 'shape>);

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

impl<'mem, 'facet, 'shape> Drop for PeekMapIter<'mem, 'facet, 'shape> {
    fn drop(&mut self) {
        unsafe { (self.map.def.vtable.iter_vtable.dealloc)(self.iter) }
    }
}

impl<'mem, 'facet, 'shape> IntoIterator for &'mem PeekMap<'mem, 'facet, 'shape> {
    type Item = (Peek<'mem, 'facet, 'shape>, Peek<'mem, 'facet, 'shape>);
    type IntoIter = PeekMapIter<'mem, 'facet, 'shape>;

    fn into_iter(self) -> Self::IntoIter {
        self.iter()
    }
}

/// Lets you read from a map (implements read-only [`facet_core::MapVTable`] proxies)
#[derive(Clone, Copy)]
pub struct PeekMap<'mem, 'facet, 'shape> {
    pub(crate) value: Peek<'mem, 'facet, 'shape>,

    pub(crate) def: MapDef<'shape>,
}

impl<'mem, 'facet, 'shape> core::fmt::Debug for PeekMap<'mem, 'facet, 'shape> {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("PeekMap").finish_non_exhaustive()
    }
}

impl<'mem, 'facet, 'shape> PeekMap<'mem, 'facet, 'shape> {
    /// Constructor
    pub fn new(value: Peek<'mem, 'facet, 'shape>, def: MapDef<'shape>) -> Self {
        Self { value, def }
    }

    /// Get the number of entries in the map
    pub fn len(&self) -> usize {
        unsafe { (self.def.vtable.len_fn)(self.value.data().thin().unwrap()) }
    }

    /// Returns true if the map is empty
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// Check if the map contains a key
    pub fn contains_key(&self, key: &impl facet_core::Facet<'facet>) -> bool {
        unsafe {
            let key_ptr = PtrConst::new(key);
            (self.def.vtable.contains_key_fn)(self.value.data().thin().unwrap(), key_ptr)
        }
    }

    /// Get a value from the map for the given key
    pub fn get<'k>(
        &self,
        key: &'k impl facet_core::Facet<'facet>,
    ) -> Option<Peek<'mem, 'facet, 'shape>> {
        unsafe {
            let key_ptr = PtrConst::new(key);
            let value_ptr =
                (self.def.vtable.get_value_ptr_fn)(self.value.data().thin().unwrap(), key_ptr)?;
            Some(Peek::unchecked_new(value_ptr, self.def.v()))
        }
    }

    /// Returns an iterator over the key-value pairs in the map
    pub fn iter(self) -> PeekMapIter<'mem, 'facet, 'shape> {
        let iter_init_with_value_fn = self.def.vtable.iter_vtable.init_with_value.unwrap();
        let iter = unsafe { iter_init_with_value_fn(self.value.data().thin().unwrap()) };
        PeekMapIter { map: self, iter }
    }

    /// Def getter
    pub fn def(&self) -> MapDef<'shape> {
        self.def
    }
}
