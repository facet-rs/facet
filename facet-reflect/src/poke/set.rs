use core::mem::ManuallyDrop;

use facet_core::{Facet, SetDef};

use crate::{ReflectError, ReflectErrorKind};

use super::Poke;

/// Lets you mutate a set (implements mutable [`facet_core::SetVTable`] proxies)
pub struct PokeSet<'mem, 'facet> {
    value: Poke<'mem, 'facet>,
    def: SetDef,
}

impl<'mem, 'facet> core::fmt::Debug for PokeSet<'mem, 'facet> {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("PokeSet").finish_non_exhaustive()
    }
}

impl<'mem, 'facet> PokeSet<'mem, 'facet> {
    /// Creates a new poke set
    ///
    /// # Safety
    ///
    /// The caller must ensure that `def` contains valid vtable function pointers that:
    /// - Correctly implement the set operations for the actual type
    /// - Do not cause undefined behavior when called
    /// - Return pointers within valid memory bounds
    /// - Match the element type specified in `def.t()`
    ///
    /// Violating these requirements can lead to memory safety issues.
    #[inline]
    pub const unsafe fn new(value: Poke<'mem, 'facet>, def: SetDef) -> Self {
        Self { value, def }
    }

    fn err(&self, kind: ReflectErrorKind) -> ReflectError {
        self.value.err(kind)
    }

    /// Get the number of entries in the set
    #[inline]
    pub fn len(&self) -> usize {
        unsafe { (self.def.vtable.len)(self.value.data()) }
    }

    /// Returns true if the set is empty
    #[inline]
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// Check if the set contains a value
    #[inline]
    pub fn contains(&self, value: &impl Facet<'facet>) -> Result<bool, ReflectError> {
        self.contains_peek(crate::Peek::new(value))
    }

    /// Check if the set contains a value (using a `Peek`)
    #[inline]
    pub fn contains_peek(&self, value: crate::Peek<'_, 'facet>) -> Result<bool, ReflectError> {
        if self.def.t() == value.shape() {
            return Ok(unsafe { (self.def.vtable.contains)(self.value.data(), value.data()) });
        }
        Err(self.err(ReflectErrorKind::WrongShape {
            expected: self.def.t(),
            actual: value.shape(),
        }))
    }

    /// Insert a value into the set. Returns `true` if the value was newly
    /// inserted, `false` if it was already present.
    pub fn insert<T: Facet<'facet>>(&mut self, value: T) -> Result<bool, ReflectError> {
        if self.def.t() != T::SHAPE {
            return Err(self.err(ReflectErrorKind::WrongShape {
                expected: self.def.t(),
                actual: T::SHAPE,
            }));
        }

        let mut value = ManuallyDrop::new(value);
        let inserted = unsafe {
            let value_ptr = facet_core::PtrMut::new(&mut value as *mut ManuallyDrop<T> as *mut u8);
            (self.def.vtable.insert)(self.value.data_mut(), value_ptr)
        };
        Ok(inserted)
    }

    /// Returns an iterator over the values in the set (read-only).
    #[inline]
    pub fn iter(&self) -> crate::PeekSetIter<'_, 'facet> {
        self.as_peek_set().iter()
    }

    /// Def getter
    #[inline]
    pub const fn def(&self) -> SetDef {
        self.def
    }

    /// Converts this `PokeSet` back into a `Poke`
    #[inline]
    pub fn into_inner(self) -> Poke<'mem, 'facet> {
        self.value
    }

    /// Returns a read-only `PeekSet` view
    #[inline]
    pub fn as_peek_set(&self) -> crate::PeekSet<'_, 'facet> {
        unsafe { crate::PeekSet::new(self.value.as_peek(), self.def) }
    }
}

#[cfg(test)]
mod tests {
    use alloc::collections::BTreeSet;

    use super::*;

    #[test]
    fn poke_set_len_and_insert() {
        let mut s: BTreeSet<i32> = BTreeSet::new();
        let poke = Poke::new(&mut s);
        let mut set = poke.into_set().unwrap();
        assert_eq!(set.len(), 0);

        assert!(set.insert(1i32).unwrap());
        assert!(set.insert(2i32).unwrap());
        assert!(!set.insert(1i32).unwrap());

        assert_eq!(set.len(), 2);
        assert!(s.contains(&1));
        assert!(s.contains(&2));
    }

    #[test]
    fn poke_set_contains() {
        let mut s: BTreeSet<i32> = BTreeSet::new();
        s.insert(42);
        let poke = Poke::new(&mut s);
        let set = poke.into_set().unwrap();

        assert!(set.contains(&42i32).unwrap());
        assert!(!set.contains(&7i32).unwrap());
    }
}
