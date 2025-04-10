use facet_core::{MapDef, Opaque, OpaqueConst};

use crate::Peek;

use super::PeekValue;

/// Iterator over key-value pairs in a `PeekMap`
pub struct PeekMapIter<'mem> {
    map: PeekMap<'mem>,
    iter: Opaque<'mem>,
}

impl<'mem> Iterator for PeekMapIter<'mem> {
    type Item = (Peek<'mem>, Peek<'mem>);

    fn next(&mut self) -> Option<Self::Item> {
        unsafe {
            let next = (self.map.def.vtable.iter_vtable.next)(self.iter);
            next.map(|(key_ptr, value_ptr)| {
                (
                    Peek::unchecked_new(key_ptr, self.map.def.k),
                    Peek::unchecked_new(value_ptr, self.map.def.v),
                )
            })
        }
    }
}

impl<'mem> IntoIterator for &'mem PeekMap<'mem> {
    type Item = (Peek<'mem>, Peek<'mem>);
    type IntoIter = PeekMapIter<'mem>;

    fn into_iter(self) -> Self::IntoIter {
        self.iter()
    }
}

/// Lets you read from a map (implements read-only [`MapVTable`] proxies)
#[derive(Clone, Copy)]
pub struct PeekMap<'mem> {
    value: PeekValue<'mem>,
    def: MapDef,
}

impl<'mem> core::ops::Deref for PeekMap<'mem> {
    type Target = PeekValue<'mem>;

    #[inline(always)]
    fn deref(&self) -> &Self::Target {
        &self.value
    }
}

impl<'mem> PeekMap<'mem> {
    /// Constructor
    pub fn new(value: PeekValue<'mem>, def: MapDef) -> Self {
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
    pub fn contains_key(&self, key: &impl facet_core::Facet) -> bool {
        unsafe {
            let key_ptr = OpaqueConst::new(key);
            (self.def.vtable.contains_key_fn)(self.value.data(), key_ptr)
        }
    }

    /// Get a value from the map for the given key
    pub fn get<'k>(&self, key: &'k impl facet_core::Facet) -> Option<Peek<'mem>> {
        unsafe {
            let key_ptr = OpaqueConst::new(key);
            let value_ptr = (self.def.vtable.get_value_ptr_fn)(self.value.data(), key_ptr)?;
            Some(Peek::unchecked_new(value_ptr, self.def.v))
        }
    }

    /// Returns an iterator over the key-value pairs in the map
    pub fn iter(self) -> PeekMapIter<'mem> {
        let iter = unsafe { (self.def.vtable.iter_fn)(self.value.data()) };
        PeekMapIter { map: self, iter }
    }
}
