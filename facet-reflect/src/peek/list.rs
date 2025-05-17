use super::Peek;
use core::{fmt::Debug, marker::PhantomData};
use facet_core::{ListDef, PtrConst, PtrMut};

/// Iterator over a `PeekList`
pub struct PeekListIter<'mem, 'facet_lifetime> {
    state: PeekListIterState<'mem>,
    index: usize,
    len: usize,
    def: ListDef,
    _list: PhantomData<Peek<'mem, 'facet_lifetime>>,
}

impl<'mem, 'facet_lifetime> Iterator for PeekListIter<'mem, 'facet_lifetime> {
    type Item = Peek<'mem, 'facet_lifetime>;

    fn next(&mut self) -> Option<Self::Item> {
        let item_ptr = match self.state {
            PeekListIterState::Ptr { data, stride } => {
                if self.index >= self.len {
                    return None;
                }

                unsafe { data.field(stride * self.index) }
            }
            PeekListIterState::Iter { iter } => unsafe {
                (self.def.vtable.iter_vtable.next)(iter)?
            },
        };

        // Update the index. This is used pointer iteration and for
        // calculating the iterator's size
        self.index += 1;

        Some(unsafe { Peek::unchecked_new(item_ptr, self.def.t()) })
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

enum PeekListIterState<'mem> {
    Ptr { data: PtrConst<'mem>, stride: usize },
    Iter { iter: PtrMut<'mem> },
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
        let item = unsafe { (self.def.vtable.get)(self.value.data(), index)? };

        Some(unsafe { Peek::unchecked_new(item, self.def.t()) })
    }

    /// Returns an iterator over the list
    pub fn iter(self) -> PeekListIter<'mem, 'facet_lifetime> {
        let state = if let Some(as_ptr_fn) = self.def.vtable.as_ptr {
            let data = unsafe { as_ptr_fn(self.value.data()) };
            let layout = self
                .def
                .t()
                .layout
                .sized_layout()
                .expect("can only iterate over sized list elements");
            let stride = layout.size();

            PeekListIterState::Ptr { data, stride }
        } else {
            let iter = unsafe {
                (self.def.vtable.iter_vtable.init_with_value.unwrap())(self.value.data())
            };
            PeekListIterState::Iter { iter }
        };

        PeekListIter {
            state,
            index: 0,
            len: self.len(),
            def: self.def(),
            _list: PhantomData,
        }
    }

    /// Def getter
    pub fn def(&self) -> ListDef {
        self.def
    }
}
