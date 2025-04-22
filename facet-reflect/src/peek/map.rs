use facet_core::{MapDef, PtrConst, PtrMut};

use super::Peek;

/// Iterator over key-value pairs in a `PeekMap`
pub struct PeekMapIter<'mem> {
    map: PeekMap<'mem>,
    iter: PtrMut<'mem>,
}

impl<'mem> Iterator for PeekMapIter<'mem> {
    type Item = (Peek<'mem>, Peek<'mem>);

    fn next(&mut self) -> Option<Self::Item> {
        unsafe {
            let next = (self.map.def.vtable.iter_vtable.next)(self.iter);
            next.map(|(key_ptr, value_ptr)| {
                (
                    Peek {
                        data: key_ptr,
                        shape: self.map.def.k,
                    },
                    Peek {
                        data: value_ptr,
                        shape: self.map.def.v,
                    },
                )
            })
        }
    }
}

impl Drop for PeekMapIter<'_> {
    fn drop(&mut self) {
        unsafe { (self.map.def.vtable.iter_vtable.dealloc)(self.iter) }
    }
}

impl<'mem> IntoIterator for &'mem PeekMap<'mem> {
    type Item = (Peek<'mem>, Peek<'mem>);
    type IntoIter = PeekMapIter<'mem>;

    fn into_iter(self) -> Self::IntoIter {
        self.iter()
    }
}

/// Lets you read from a map (implements read-only [`facet_core::MapVTable`] proxies)
#[derive(Clone, Copy)]
pub struct PeekMap<'mem> {
    pub(crate) value: Peek<'mem>,

    pub(crate) def: MapDef,
}

impl<'mem> PeekMap<'mem> {
    /// Constructor
    pub fn new(value: Peek<'mem>, def: MapDef) -> Self {
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
    pub fn contains_key<T: facet_core::Facet<'mem>>(&self, key: &T) -> bool {
        unsafe {
            let key_ptr = PtrConst::new(key);
            (self.def.vtable.contains_key_fn)(self.value.data(), key_ptr)
        }
    }

    /// Get a value from the map for the given key
    pub fn get<T: facet_core::Facet<'mem>>(&self, key: &T) -> Option<Peek<'mem>> {
        unsafe {
            let key_ptr = PtrConst::new(key);
            let value_ptr = (self.def.vtable.get_value_ptr_fn)(self.value.data(), key_ptr)?;
            Some(Peek {
                data: value_ptr,
                shape: self.def.v,
            })
        }
    }

    /// Returns an iterator over the key-value pairs in the map
    pub fn iter(self) -> PeekMapIter<'mem> {
        let iter = unsafe { (self.def.vtable.iter_fn)(self.value.data()) };
        PeekMapIter { map: self, iter }
    }

    /// Def getter
    pub fn def(&self) -> MapDef {
        self.def
    }
}
