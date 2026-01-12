use super::Peek;
use core::{fmt::Debug, marker::PhantomData};
use facet_core::{ListDef, PtrConst, PtrMut};

/// Iterator over a `PeekList`
pub struct PeekListIter<'mem, 'facet> {
    state: PeekListIterState<'mem>,
    index: usize,
    len: usize,
    def: ListDef,
    _list: PhantomData<Peek<'mem, 'facet>>,
}

impl<'mem, 'facet> Iterator for PeekListIter<'mem, 'facet> {
    type Item = Peek<'mem, 'facet>;

    #[inline]
    fn next(&mut self) -> Option<Self::Item> {
        let item_ptr = match &self.state.kind {
            PeekListIterStateKind::Ptr { data, stride } => {
                if self.index >= self.len {
                    return None;
                }

                unsafe { data.field(stride * self.index) }
            }
            PeekListIterStateKind::Iter { iter } => unsafe {
                (self.def.iter_vtable().unwrap().next)(*iter)?
            },
        };

        // Update the index. This is used pointer iteration and for
        // calculating the iterator's size
        self.index += 1;

        Some(unsafe { Peek::unchecked_new(item_ptr, self.def.t()) })
    }

    #[inline]
    fn size_hint(&self) -> (usize, Option<usize>) {
        let remaining = self.len.saturating_sub(self.index);
        (remaining, Some(remaining))
    }
}

impl ExactSizeIterator for PeekListIter<'_, '_> {}

impl Drop for PeekListIter<'_, '_> {
    #[inline]
    fn drop(&mut self) {
        match &self.state.kind {
            PeekListIterStateKind::Iter { iter } => unsafe {
                (self.def.iter_vtable().unwrap().dealloc)(*iter)
            },
            PeekListIterStateKind::Ptr { .. } => {
                // Nothing to do
            }
        }
    }
}

impl<'mem, 'facet> IntoIterator for &'mem PeekList<'mem, 'facet> {
    type Item = Peek<'mem, 'facet>;
    type IntoIter = PeekListIter<'mem, 'facet>;

    #[inline]
    fn into_iter(self) -> Self::IntoIter {
        self.iter()
    }
}

struct PeekListIterState<'mem> {
    kind: PeekListIterStateKind,
    _phantom: PhantomData<&'mem ()>,
}

enum PeekListIterStateKind {
    Ptr { data: PtrConst, stride: usize },
    Iter { iter: PtrMut },
}

/// Lets you read from a list (implements read-only [`facet_core::ListVTable`] proxies)
#[derive(Clone, Copy)]
pub struct PeekList<'mem, 'facet> {
    value: Peek<'mem, 'facet>,
    def: ListDef,
}

impl Debug for PeekList<'_, '_> {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("PeekList").finish_non_exhaustive()
    }
}

impl<'mem, 'facet> PeekList<'mem, 'facet> {
    /// Creates a new peek list
    ///
    /// # Safety
    ///
    /// The caller must ensure that `def` contains valid vtable function pointers that:
    /// - Correctly implement the list operations for the actual type
    /// - Do not cause undefined behavior when called
    /// - Return pointers within valid memory bounds
    /// - Match the element type specified in `def.t()`
    ///
    /// Violating these requirements can lead to memory safety issues.
    #[inline]
    pub const unsafe fn new(value: Peek<'mem, 'facet>, def: ListDef) -> Self {
        Self { value, def }
    }

    /// Get the length of the list
    #[inline]
    pub fn len(&self) -> usize {
        unsafe { (self.def.vtable.len)(self.value.data()) }
    }

    /// Returns true if the list is empty
    #[inline]
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// Get an item from the list at the specified index
    #[inline]
    pub fn get(&self, index: usize) -> Option<Peek<'mem, 'facet>> {
        let item = unsafe { (self.def.vtable.get)(self.value.data(), index, self.value.shape())? };

        Some(unsafe { Peek::unchecked_new(item, self.def.t()) })
    }

    /// Returns an iterator over the list
    pub fn iter(self) -> PeekListIter<'mem, 'facet> {
        let state = if let Some(as_ptr_fn) = self.def.vtable.as_ptr {
            let data = unsafe { as_ptr_fn(self.value.data()) };
            let layout = self
                .def
                .t()
                .layout
                .sized_layout()
                .expect("can only iterate over sized list elements");
            let stride = layout.size();

            PeekListIterState {
                kind: PeekListIterStateKind::Ptr { data, stride },
                _phantom: PhantomData,
            }
        } else {
            let iter = unsafe {
                (self.def.iter_vtable().unwrap().init_with_value.unwrap())(self.value.data())
            };
            PeekListIterState {
                kind: PeekListIterStateKind::Iter { iter },
                _phantom: PhantomData,
            }
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
    #[inline]
    pub const fn def(&self) -> ListDef {
        self.def
    }
}
