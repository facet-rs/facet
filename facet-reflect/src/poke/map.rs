use core::mem::ManuallyDrop;

use facet_core::{Facet, MapDef};

use crate::{HeapValue, ReflectError, ReflectErrorKind};

use super::Poke;

/// Lets you mutate a map (implements mutable [`facet_core::MapVTable`] proxies)
pub struct PokeMap<'mem, 'facet> {
    value: Poke<'mem, 'facet>,
    def: MapDef,
}

impl<'mem, 'facet> core::fmt::Debug for PokeMap<'mem, 'facet> {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("PokeMap").finish_non_exhaustive()
    }
}

impl<'mem, 'facet> PokeMap<'mem, 'facet> {
    /// Creates a new poke map
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
    pub const unsafe fn new(value: Poke<'mem, 'facet>, def: MapDef) -> Self {
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
    pub fn contains_key(&self, key: &impl Facet<'facet>) -> Result<bool, ReflectError> {
        self.contains_key_peek(crate::Peek::new(key))
    }

    /// Check if the map contains a key (using a `Peek`)
    #[inline]
    pub fn contains_key_peek(&self, key: crate::Peek<'_, 'facet>) -> Result<bool, ReflectError> {
        if self.def.k() == key.shape() {
            return Ok(unsafe { (self.def.vtable.contains_key)(self.value.data(), key.data()) });
        }

        Err(self.err(ReflectErrorKind::WrongShape {
            expected: self.def.k(),
            actual: key.shape(),
        }))
    }

    /// Get a value from the map for the given key, as a read-only `Peek`
    #[inline]
    pub fn get(
        &self,
        key: &impl Facet<'facet>,
    ) -> Result<Option<crate::Peek<'_, 'facet>>, ReflectError> {
        self.get_peek(crate::Peek::new(key))
    }

    /// Get a value from the map for the given key (using a `Peek`), as a read-only `Peek`
    #[inline]
    pub fn get_peek(
        &self,
        key: crate::Peek<'_, 'facet>,
    ) -> Result<Option<crate::Peek<'_, 'facet>>, ReflectError> {
        if self.def.k() != key.shape() {
            return Err(self.err(ReflectErrorKind::WrongShape {
                expected: self.def.k(),
                actual: key.shape(),
            }));
        }

        let value_ptr = unsafe { (self.def.vtable.get_value_ptr)(self.value.data(), key.data()) };
        if value_ptr.is_null() {
            return Ok(None);
        }
        let value_ptr = facet_core::PtrConst::new_sized(value_ptr);
        Ok(Some(unsafe {
            crate::Peek::unchecked_new(value_ptr, self.def.v())
        }))
    }

    /// Insert a key-value pair into the map.
    ///
    /// Both key and value must have shapes matching the map's key and value types.
    /// The key and value are moved into the map.
    pub fn insert<K, V>(&mut self, key: K, value: V) -> Result<(), ReflectError>
    where
        K: Facet<'facet>,
        V: Facet<'facet>,
    {
        if self.def.k() != K::SHAPE {
            return Err(self.err(ReflectErrorKind::WrongShape {
                expected: self.def.k(),
                actual: K::SHAPE,
            }));
        }
        if self.def.v() != V::SHAPE {
            return Err(self.err(ReflectErrorKind::WrongShape {
                expected: self.def.v(),
                actual: V::SHAPE,
            }));
        }

        // The insert vtable moves the key and value (via ptr::read), so we need to
        // hand over temporary storage that we will not drop afterwards.
        let mut key = ManuallyDrop::new(key);
        let mut value = ManuallyDrop::new(value);
        unsafe {
            let key_ptr = facet_core::PtrMut::new(&mut key as *mut ManuallyDrop<K> as *mut u8);
            let value_ptr = facet_core::PtrMut::new(&mut value as *mut ManuallyDrop<V> as *mut u8);
            (self.def.vtable.insert)(self.value.data_mut(), key_ptr, value_ptr);
        }

        Ok(())
    }

    /// Type-erased [`insert`](Self::insert).
    ///
    /// Accepts [`HeapValue`]s for key and value; their shapes must match the map's key and
    /// value types. Both values are moved into the map.
    pub fn insert_from_heap<const KB: bool, const VB: bool>(
        &mut self,
        key: HeapValue<'facet, KB>,
        value: HeapValue<'facet, VB>,
    ) -> Result<(), ReflectError> {
        if self.def.k() != key.shape() {
            return Err(self.err(ReflectErrorKind::WrongShape {
                expected: self.def.k(),
                actual: key.shape(),
            }));
        }
        if self.def.v() != value.shape() {
            return Err(self.err(ReflectErrorKind::WrongShape {
                expected: self.def.v(),
                actual: value.shape(),
            }));
        }

        let mut key = key;
        let mut value = value;
        let key_guard = key.guard.take().expect("key HeapValue guard already taken");
        let value_guard = value
            .guard
            .take()
            .expect("value HeapValue guard already taken");
        unsafe {
            let key_ptr = facet_core::PtrMut::new(key_guard.ptr.as_ptr());
            let value_ptr = facet_core::PtrMut::new(value_guard.ptr.as_ptr());
            (self.def.vtable.insert)(self.value.data_mut(), key_ptr, value_ptr);
        }
        drop(key_guard);
        drop(value_guard);
        Ok(())
    }

    /// Returns an iterator over the key-value pairs in the map (read-only).
    #[inline]
    pub fn iter(&self) -> crate::PeekMapIter<'_, 'facet> {
        self.as_peek_map().iter()
    }

    /// Def getter
    #[inline]
    pub const fn def(&self) -> MapDef {
        self.def
    }

    /// Converts this `PokeMap` back into a `Poke`
    #[inline]
    pub fn into_inner(self) -> Poke<'mem, 'facet> {
        self.value
    }

    /// Returns a read-only `PeekMap` view
    #[inline]
    pub fn as_peek_map(&self) -> crate::PeekMap<'_, 'facet> {
        unsafe { crate::PeekMap::new(self.value.as_peek(), self.def) }
    }
}

#[cfg(test)]
mod tests {
    use alloc::collections::BTreeMap;

    use super::*;

    #[test]
    fn poke_map_len_and_insert() {
        let mut m: BTreeMap<String, i32> = BTreeMap::new();
        let poke = Poke::new(&mut m);
        let mut map = poke.into_map().unwrap();
        assert_eq!(map.len(), 0);
        map.insert(String::from("one"), 1i32).unwrap();
        map.insert(String::from("two"), 2i32).unwrap();
        assert_eq!(map.len(), 2);

        assert_eq!(m.get("one"), Some(&1));
        assert_eq!(m.get("two"), Some(&2));
    }

    #[test]
    fn poke_map_contains_and_get() {
        let mut m: BTreeMap<String, i32> = BTreeMap::new();
        m.insert(String::from("a"), 10);
        let poke = Poke::new(&mut m);
        let map = poke.into_map().unwrap();

        let key = String::from("a");
        assert!(map.contains_key(&key).unwrap());

        let v = map.get(&key).unwrap().unwrap();
        assert_eq!(*v.get::<i32>().unwrap(), 10);
    }

    #[test]
    fn poke_map_insert_from_heap() {
        let mut m: BTreeMap<String, i32> = BTreeMap::new();
        let poke = Poke::new(&mut m);
        let mut map = poke.into_map().unwrap();

        let key = crate::Partial::alloc::<String>()
            .unwrap()
            .set(String::from("k"))
            .unwrap()
            .build()
            .unwrap();
        let value = crate::Partial::alloc::<i32>()
            .unwrap()
            .set(42i32)
            .unwrap()
            .build()
            .unwrap();
        map.insert_from_heap(key, value).unwrap();

        assert_eq!(m.get("k"), Some(&42));
    }
}
