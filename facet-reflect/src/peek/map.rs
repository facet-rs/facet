use facet_core::{MapDef, PtrMut};

use crate::{ReflectError, ReflectErrorKind};

use super::Peek;

/// Iterator over key-value pairs in a `PeekMap`
pub struct PeekMapIter<'mem, 'facet> {
    map: PeekMap<'mem, 'facet>,
    iter: PtrMut,
}

impl<'mem, 'facet> Iterator for PeekMapIter<'mem, 'facet> {
    type Item = (Peek<'mem, 'facet>, Peek<'mem, 'facet>);

    #[inline]
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

impl<'mem, 'facet> Drop for PeekMapIter<'mem, 'facet> {
    #[inline]
    fn drop(&mut self) {
        unsafe { (self.map.def.vtable.iter_vtable.dealloc)(self.iter) }
    }
}

impl<'mem, 'facet> IntoIterator for &'mem PeekMap<'mem, 'facet> {
    type Item = (Peek<'mem, 'facet>, Peek<'mem, 'facet>);
    type IntoIter = PeekMapIter<'mem, 'facet>;

    #[inline]
    fn into_iter(self) -> Self::IntoIter {
        self.iter()
    }
}

/// Lets you read from a map (implements read-only [`facet_core::MapVTable`] proxies)
#[derive(Clone, Copy)]
pub struct PeekMap<'mem, 'facet> {
    value: Peek<'mem, 'facet>,

    def: MapDef,
}

impl<'mem, 'facet> core::fmt::Debug for PeekMap<'mem, 'facet> {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("PeekMap").finish_non_exhaustive()
    }
}

impl<'mem, 'facet> PeekMap<'mem, 'facet> {
    /// Constructor
    ///
    /// # Safety
    ///
    /// The caller must ensure that `def` contains valid vtable function pointers that:
    /// - Correctly implement the map operations for the actual type
    /// - Do not cause undefined behavior when called
    /// - Return pointers within valid memory bounds
    /// - Match the key and value types specified in `def.k()` and `def.v()`
    ///
    /// Violating these requirements can lead to memory safety issues.
    #[inline]
    pub const unsafe fn new(value: Peek<'mem, 'facet>, def: MapDef) -> Self {
        Self { value, def }
    }

    fn err(&self, kind: ReflectErrorKind) -> ReflectError {
        self.value.err(kind)
    }

    /// Get the number of entries in the map
    #[inline]
    pub fn len(&self) -> usize {
        unsafe { (self.def.vtable.len)(self.value.data()) }
    }

    /// Returns true if the map is empty
    #[inline]
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// Check if the map contains a key
    #[inline]
    pub fn contains_key(&self, key: &impl facet_core::Facet<'facet>) -> Result<bool, ReflectError> {
        self.contains_key_peek(Peek::new(key))
    }

    /// Get a value from the map for the given key
    #[inline]
    pub fn get<'k>(
        &self,
        key: &'k impl facet_core::Facet<'facet>,
    ) -> Result<Option<Peek<'mem, 'facet>>, ReflectError> {
        self.get_peek(Peek::new(key))
    }

    /// Check if the map contains a key
    #[inline]
    pub fn contains_key_peek(&self, key: Peek<'_, 'facet>) -> Result<bool, ReflectError> {
        if self.def.k() == key.shape {
            return Ok(unsafe { (self.def.vtable.contains_key)(self.value.data(), key.data()) });
        }

        Err(self.err(ReflectErrorKind::WrongShape {
            expected: self.def.k(),
            actual: key.shape,
        }))
    }

    /// Get a value from the map for the given key
    #[inline]
    pub fn get_peek(
        &self,
        key: Peek<'_, 'facet>,
    ) -> Result<Option<Peek<'mem, 'facet>>, ReflectError> {
        if self.def.k() == key.shape {
            return Ok(unsafe {
                let Some(value_ptr) =
                    (self.def.vtable.get_value_ptr)(self.value.data(), key.data())
                else {
                    return Ok(None);
                };
                Some(Peek::unchecked_new(value_ptr, self.def.v()))
            });
        }

        Err(self.err(ReflectErrorKind::WrongShape {
            expected: self.def.k(),
            actual: key.shape,
        }))
    }

    /// Returns an iterator over the key-value pairs in the map
    #[inline]
    pub fn iter(self) -> PeekMapIter<'mem, 'facet> {
        let iter_init_with_value_fn = self.def.vtable.iter_vtable.init_with_value.unwrap();
        let iter = unsafe { iter_init_with_value_fn(self.value.data()) };
        PeekMapIter { map: self, iter }
    }

    /// Def getter
    #[inline]
    pub const fn def(&self) -> MapDef {
        self.def
    }
}
