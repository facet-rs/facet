use super::Poke;
use core::{fmt::Debug, marker::PhantomData};
use facet_core::{ListDef, PtrMut};

/// Iterator over a `PokeList`
pub struct PokeListIter<'mem, 'facet> {
    state: PokeListIterState<'mem>,
    index: usize,
    len: usize,
    def: ListDef,
    _list: PhantomData<Poke<'mem, 'facet>>,
}

impl<'mem, 'facet> Iterator for PokeListIter<'mem, 'facet> {
    type Item = Poke<'mem, 'facet>;

    #[inline]
    fn next(&mut self) -> Option<Self::Item> {
        let item_ptr = match &mut self.state.kind {
            PokeListIterStateKind::Ptr { data, stride } => {
                if self.index >= self.len {
                    return None;
                }

                unsafe { data.field(*stride * self.index) }
            }
            PokeListIterStateKind::Iter { iter } => unsafe {
                // The iter vtable returns PtrConst, but we know the underlying data is mutable
                // because we created this iterator from a PokeList which has mutable access.
                // We need to convert the const pointer back to mutable.
                let const_ptr = (self.def.iter_vtable().unwrap().next)(*iter)?;
                PtrMut::new(const_ptr.as_byte_ptr() as *mut u8)
            },
        };

        self.index += 1;

        Some(unsafe { Poke::from_raw_parts(item_ptr, self.def.t()) })
    }

    #[inline]
    fn size_hint(&self) -> (usize, Option<usize>) {
        let remaining = self.len.saturating_sub(self.index);
        (remaining, Some(remaining))
    }
}

impl ExactSizeIterator for PokeListIter<'_, '_> {}

impl Drop for PokeListIter<'_, '_> {
    #[inline]
    fn drop(&mut self) {
        match &self.state.kind {
            PokeListIterStateKind::Iter { iter } => unsafe {
                (self.def.iter_vtable().unwrap().dealloc)(*iter)
            },
            PokeListIterStateKind::Ptr { .. } => {
                // Nothing to do
            }
        }
    }
}

struct PokeListIterState<'mem> {
    kind: PokeListIterStateKind,
    _phantom: PhantomData<&'mem mut ()>,
}

enum PokeListIterStateKind {
    Ptr { data: PtrMut, stride: usize },
    Iter { iter: PtrMut },
}

/// Lets you mutate a list (implements mutable [`facet_core::ListVTable`] proxies)
pub struct PokeList<'mem, 'facet> {
    value: Poke<'mem, 'facet>,
    def: ListDef,
}

impl Debug for PokeList<'_, '_> {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("PokeList").finish_non_exhaustive()
    }
}

impl<'mem, 'facet> PokeList<'mem, 'facet> {
    /// Creates a new poke list
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
    pub const unsafe fn new(value: Poke<'mem, 'facet>, def: ListDef) -> Self {
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

    /// Get an immutable reference to an item from the list at the specified index
    #[inline]
    pub fn get(&self, index: usize) -> Option<crate::Peek<'_, 'facet>> {
        let item = unsafe { (self.def.vtable.get)(self.value.data(), index, self.value.shape())? };

        Some(unsafe { crate::Peek::unchecked_new(item, self.def.t()) })
    }

    /// Get a mutable reference to an item from the list at the specified index
    #[inline]
    pub fn get_mut(&mut self, index: usize) -> Option<Poke<'_, 'facet>> {
        let get_mut_fn = self.def.vtable.get_mut?;
        let item = unsafe { get_mut_fn(self.value.data, index, self.value.shape())? };

        Some(unsafe { Poke::from_raw_parts(item, self.def.t()) })
    }

    /// Returns a mutable iterator over the list
    pub fn iter_mut(self) -> PokeListIter<'mem, 'facet> {
        let state = if let Some(as_mut_ptr_fn) = self.def.vtable.as_mut_ptr {
            let data = unsafe { as_mut_ptr_fn(self.value.data) };
            let layout = self
                .def
                .t()
                .layout
                .sized_layout()
                .expect("can only iterate over sized list elements");
            let stride = layout.size();

            PokeListIterState {
                kind: PokeListIterStateKind::Ptr { data, stride },
                _phantom: PhantomData,
            }
        } else {
            // Fall back to the immutable iterator, but we know we have mutable access
            let iter = unsafe {
                (self.def.iter_vtable().unwrap().init_with_value.unwrap())(self.value.data())
            };
            PokeListIterState {
                kind: PokeListIterStateKind::Iter { iter },
                _phantom: PhantomData,
            }
        };

        PokeListIter {
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

    /// Converts this `PokeList` back into a `Poke`
    #[inline]
    pub fn into_inner(self) -> Poke<'mem, 'facet> {
        self.value
    }

    /// Returns a read-only `PeekList` view
    #[inline]
    pub fn as_peek_list(&self) -> crate::PeekList<'_, 'facet> {
        unsafe { crate::PeekList::new(self.value.as_peek(), self.def) }
    }
}

impl<'mem, 'facet> IntoIterator for PokeList<'mem, 'facet> {
    type Item = Poke<'mem, 'facet>;
    type IntoIter = PokeListIter<'mem, 'facet>;

    #[inline]
    fn into_iter(self) -> Self::IntoIter {
        self.iter_mut()
    }
}

#[cfg(test)]
mod tests {
    use alloc::vec::Vec;

    use super::*;

    #[test]
    fn poke_list_len() {
        let mut v: Vec<i32> = alloc::vec![1, 2, 3, 4, 5];
        let poke = Poke::new(&mut v);
        let list = poke.into_list().unwrap();
        assert_eq!(list.len(), 5);
    }

    #[test]
    fn poke_list_get() {
        let mut v: Vec<i32> = alloc::vec![10, 20, 30];
        let poke = Poke::new(&mut v);
        let list = poke.into_list().unwrap();

        let item = list.get(1).unwrap();
        assert_eq!(*item.get::<i32>().unwrap(), 20);
    }

    #[test]
    fn poke_list_get_mut() {
        let mut v: Vec<i32> = alloc::vec![10, 20, 30];
        let poke = Poke::new(&mut v);
        let mut list = poke.into_list().unwrap();

        {
            let mut item = list.get_mut(1).unwrap();
            item.set(99i32).unwrap();
        }

        // Verify the change
        let item = list.get(1).unwrap();
        assert_eq!(*item.get::<i32>().unwrap(), 99);
    }

    #[test]
    fn poke_list_iter_mut() {
        let mut v: Vec<i32> = alloc::vec![1, 2, 3];
        let poke = Poke::new(&mut v);
        let list = poke.into_list().unwrap();

        let mut sum = 0;
        for mut item in list {
            let val = *item.get::<i32>().unwrap();
            item.set(val * 10).unwrap();
            sum += val;
        }

        assert_eq!(sum, 6);
        assert_eq!(v, alloc::vec![10, 20, 30]);
    }
}
