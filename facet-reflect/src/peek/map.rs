use facet_core::{MapDef, PtrConst, PtrMut};

use super::Peek;

/// Iterator over key-value pairs in a `PeekMap`
pub struct PeekMapIter<'mem, 'facet_lifetime, 'shape> {
    map: PeekMap<'mem, 'facet_lifetime, 'shape>,
    iter: PtrMut<'mem>,
}

impl<'mem, 'facet_lifetime, 'shape> Iterator for PeekMapIter<'mem, 'facet_lifetime, 'shape> {
    type Item = (
        Peek<'mem, 'facet_lifetime, 'shape>,
        Peek<'mem, 'facet_lifetime, 'shape>,
    );

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

impl<'mem, 'facet_lifetime, 'shape> DoubleEndedIterator
    for PeekMapIter<'mem, 'facet_lifetime, 'shape>
{
    fn next_back(&mut self) -> Option<Self::Item> {
        let next_back_fn = self.map.def.vtable.iter_vtable.next_back.unwrap();
        unsafe {
            let next_back = next_back_fn(self.iter);
            next_back.map(|(key_ptr, value_ptr)| {
                (
                    Peek::unchecked_new(key_ptr, self.map.def.k()),
                    Peek::unchecked_new(value_ptr, self.map.def.v()),
                )
            })
        }
    }
}

impl<'mem, 'facet_lifetime, 'shape> Drop for PeekMapIter<'mem, 'facet_lifetime, 'shape> {
    fn drop(&mut self) {
        unsafe { (self.map.def.vtable.iter_vtable.dealloc)(self.iter) }
    }
}

impl<'mem, 'facet_lifetime, 'shape> IntoIterator for &'mem PeekMap<'mem, 'facet_lifetime, 'shape> {
    type Item = (
        Peek<'mem, 'facet_lifetime, 'shape>,
        Peek<'mem, 'facet_lifetime, 'shape>,
    );
    type IntoIter = PeekMapIter<'mem, 'facet_lifetime, 'shape>;

    fn into_iter(self) -> Self::IntoIter {
        self.iter()
    }
}

/// Lets you read from a map (implements read-only [`facet_core::MapVTable`] proxies)
#[derive(Clone, Copy)]
pub struct PeekMap<'mem, 'facet_lifetime, 'shape> {
    pub(crate) value: Peek<'mem, 'facet_lifetime, 'shape>,

    pub(crate) def: MapDef<'shape>,
}

impl<'mem, 'facet_lifetime, 'shape> core::fmt::Debug for PeekMap<'mem, 'facet_lifetime, 'shape> {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("PeekMap").finish_non_exhaustive()
    }
}

impl<'mem, 'facet_lifetime, 'shape> PeekMap<'mem, 'facet_lifetime, 'shape> {
    /// Constructor
    pub fn new(value: Peek<'mem, 'facet_lifetime, 'shape>, def: MapDef<'shape>) -> Self {
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
    ) -> Option<Peek<'mem, 'facet_lifetime, 'shape>> {
        unsafe {
            let key_ptr = PtrConst::new(key);
            let value_ptr = (self.def.vtable.get_value_ptr_fn)(self.value.data(), key_ptr)?;
            Some(Peek::unchecked_new(value_ptr, self.def.v()))
        }
    }

    /// Returns an iterator over the key-value pairs in the map
    pub fn iter(self) -> PeekMapIter<'mem, 'facet_lifetime, 'shape> {
        let iter_init_with_value_fn = self.def.vtable.iter_vtable.init_with_value.unwrap();
        let iter = unsafe { iter_init_with_value_fn(self.value.data()) };
        PeekMapIter { map: self, iter }
    }

    /// Def getter
    pub fn def(&self) -> MapDef<'shape> {
        self.def
    }
}
