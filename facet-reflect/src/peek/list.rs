use super::Peek;
use core::fmt::Debug;
use facet_core::ListDef;

/// Iterator over a `PeekList`
pub struct PeekListIter<'mem, 'facet_lifetime> {
    list: PeekList<'mem, 'facet_lifetime>,
    index: usize,
    len: usize,
}

impl<'mem, 'facet_lifetime> Iterator for PeekListIter<'mem, 'facet_lifetime> {
    type Item = Peek<'mem, 'facet_lifetime>;

    fn next(&mut self) -> Option<Self::Item> {
        if self.index >= self.len {
            return None;
        }
        let item = self.list.get(self.index);
        self.index += 1;
        item
    }

    fn size_hint(&self) -> (usize, Option<usize>) {
        let remaining = self.len.saturating_sub(self.index);
        (remaining, Some(remaining))
    }
}

impl ExactSizeIterator for PeekListIter<'_, '_> {}

impl<'mem, 'facet_lifetime> IntoIterator for &'mem PeekList<'mem, 'facet_lifetime> {
    type Item = Peek<'mem, 'facet_lifetime>;
    type IntoIter = PeekListIter<'mem, 'facet_lifetime>;

    fn into_iter(self) -> Self::IntoIter {
        self.iter()
    }
}

/// Lets you read from a list (implements read-only [`facet_core::ListVTable`] proxies)
#[derive(Clone, Copy)]
pub struct PeekList<'mem, 'facet_lifetime> {
    pub(crate) value: Peek<'mem, 'facet_lifetime>,
    pub(crate) def: ListDef,
}

impl Debug for PeekList<'_, '_> {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("PeekList").finish_non_exhaustive()
    }
}

impl<'mem, 'facet_lifetime> PeekList<'mem, 'facet_lifetime> {
    /// Creates a new peek list
    pub fn new(value: Peek<'mem, 'facet_lifetime>, def: ListDef) -> Self {
        Self { value, def }
    }

    /// Get the length of the list
    pub fn len(&self) -> usize {
        unsafe { (self.def.vtable.len)(self.value.data()) }
    }

    /// Returns true if the list is empty
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }
    /// Get an item from the list at the specified index
    pub fn get(&self, index: usize) -> Option<Peek<'mem, 'facet_lifetime>> {
        if index >= self.len() {
            return None;
        }

        let Ok(layout) = self.def.t().layout.sized_layout() else {
            return None;
        };

        let data = unsafe { (self.def.vtable.as_ptr.unwrap())(self.value.data()) };

        // SAFETY: we verify index bounds at the start of the function
        let item_ptr = unsafe { data.field(layout.size() * index) };

        Some(unsafe { Peek::unchecked_new(item_ptr, self.def.t()) })
    }

    /// Returns an iterator over the list
    pub fn iter(self) -> PeekListIter<'mem, 'facet_lifetime> {
        PeekListIter {
            list: self,
            index: 0,
            len: self.len(),
        }
    }

    /// Def getter
    pub fn def(&self) -> ListDef {
        self.def
    }
}
