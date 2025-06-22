use super::Peek;
use facet_core::{PtrMut, SetDef};

/// Iterator over values in a `PeekSet`
pub struct PeekSetIter<'mem, 'facet, 'shape> {
    set: PeekSet<'mem, 'facet, 'shape>,
    iter: PtrMut<'mem>,
}

impl<'mem, 'facet, 'shape> Iterator for PeekSetIter<'mem, 'facet, 'shape> {
    type Item = Peek<'mem, 'facet, 'shape>;

    fn next(&mut self) -> Option<Self::Item> {
        unsafe {
            let next = (self.set.def.vtable.iter_vtable.next)(self.iter)?;
            Some(Peek::unchecked_new(next, self.set.def.t()))
        }
    }
}

impl<'mem, 'facet, 'shape> Drop for PeekSetIter<'mem, 'facet, 'shape> {
    fn drop(&mut self) {
        unsafe { (self.set.def.vtable.iter_vtable.dealloc)(self.iter) }
    }
}

impl<'mem, 'facet, 'shape> IntoIterator for &'mem PeekSet<'mem, 'facet, 'shape> {
    type Item = Peek<'mem, 'facet, 'shape>;
    type IntoIter = PeekSetIter<'mem, 'facet, 'shape>;

    fn into_iter(self) -> Self::IntoIter {
        self.iter()
    }
}

/// Lets you read from a set
#[derive(Clone, Copy)]
pub struct PeekSet<'mem, 'facet, 'shape> {
    pub(crate) value: Peek<'mem, 'facet, 'shape>,

    pub(crate) def: SetDef<'shape>,
}

impl<'mem, 'facet, 'shape> core::fmt::Debug for PeekSet<'mem, 'facet, 'shape> {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("PeekSet").finish_non_exhaustive()
    }
}

impl<'mem, 'facet, 'shape> PeekSet<'mem, 'facet, 'shape> {
    /// Constructor
    pub fn new(value: Peek<'mem, 'facet, 'shape>, def: SetDef<'shape>) -> Self {
        Self { value, def }
    }

    /// Get the number of entries in the set
    pub fn len(&self) -> usize {
        unsafe { (self.def.vtable.len_fn)(self.value.data().thin().unwrap()) }
    }

    /// Returns an iterator over the values in the set
    pub fn iter(self) -> PeekSetIter<'mem, 'facet, 'shape> {
        let iter_init_with_value_fn = self.def.vtable.iter_vtable.init_with_value.unwrap();
        let iter = unsafe { iter_init_with_value_fn(self.value.data().thin().unwrap()) };
        PeekSetIter { set: self, iter }
    }

    /// Def getter
    pub fn def(&self) -> SetDef<'shape> {
        self.def
    }
}
