use super::Peek;
use facet_core::{PtrMut, SetDef};

/// Iterator over values in a `PeekSet`
pub struct PeekSetIter<'mem, 'facet> {
    set: PeekSet<'mem, 'facet>,
    iter: PtrMut<'mem>,
}

impl<'mem, 'facet> Iterator for PeekSetIter<'mem, 'facet> {
    type Item = Peek<'mem, 'facet>;

    #[inline]
    fn next(&mut self) -> Option<Self::Item> {
        unsafe {
            let next = (self.set.def.vtable.iter_vtable.next)(self.iter)?;
            Some(Peek::unchecked_new(next, self.set.def.t()))
        }
    }
}

impl<'mem, 'facet> Drop for PeekSetIter<'mem, 'facet> {
    #[inline]
    fn drop(&mut self) {
        unsafe { (self.set.def.vtable.iter_vtable.dealloc)(self.iter) }
    }
}

impl<'mem, 'facet> IntoIterator for &'mem PeekSet<'mem, 'facet> {
    type Item = Peek<'mem, 'facet>;
    type IntoIter = PeekSetIter<'mem, 'facet>;

    #[inline]
    fn into_iter(self) -> Self::IntoIter {
        self.iter()
    }
}

/// Lets you read from a set
#[derive(Clone, Copy)]
pub struct PeekSet<'mem, 'facet> {
    pub(crate) value: Peek<'mem, 'facet>,

    pub(crate) def: SetDef,
}

impl<'mem, 'facet> core::fmt::Debug for PeekSet<'mem, 'facet> {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("PeekSet").finish_non_exhaustive()
    }
}

impl<'mem, 'facet> PeekSet<'mem, 'facet> {
    /// Constructor
    #[inline]
    pub fn new(value: Peek<'mem, 'facet>, def: SetDef) -> Self {
        Self { value, def }
    }

    /// Returns true if the set is empty
    #[inline]
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// Get the number of entries in the set
    #[inline]
    pub fn len(&self) -> usize {
        unsafe { (self.def.vtable.len_fn)(self.value.data().thin().unwrap()) }
    }

    /// Returns an iterator over the values in the set
    #[inline]
    pub fn iter(self) -> PeekSetIter<'mem, 'facet> {
        let iter_init_with_value_fn = self.def.vtable.iter_vtable.init_with_value.unwrap();
        let iter = unsafe { iter_init_with_value_fn(self.value.data().thin().unwrap()) };
        PeekSetIter { set: self, iter }
    }

    /// Def getter
    #[inline]
    pub fn def(&self) -> SetDef {
        self.def
    }
}
