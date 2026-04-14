use core::{fmt::Debug, marker::PhantomData};
use facet_core::{PtrMut, ShapeLayout};

use crate::peek::ListLikeDef;

use super::Poke;

/// Lets you mutate a list, array or slice.
pub struct PokeListLike<'mem, 'facet> {
    value: Poke<'mem, 'facet>,
    def: ListLikeDef,
    len: usize,
}

impl<'mem, 'facet> Debug for PokeListLike<'mem, 'facet> {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("PokeListLike").finish_non_exhaustive()
    }
}

/// Iterator over a `PokeListLike` yielding mutable `Poke`s
pub struct PokeListLikeIter<'mem, 'facet> {
    state: PokeListLikeIterStateKind,
    index: usize,
    len: usize,
    def: ListLikeDef,
    _list: PhantomData<Poke<'mem, 'facet>>,
}

enum PokeListLikeIterStateKind {
    Ptr { data: PtrMut, stride: usize },
    Iter { iter: PtrMut },
}

impl<'mem, 'facet> Iterator for PokeListLikeIter<'mem, 'facet> {
    type Item = Poke<'mem, 'facet>;

    #[inline]
    fn next(&mut self) -> Option<Self::Item> {
        if self.index >= self.len {
            return None;
        }

        let item_ptr = match &mut self.state {
            PokeListLikeIterStateKind::Ptr { data, stride } => unsafe {
                data.field(*stride * self.index)
            },
            PokeListLikeIterStateKind::Iter { iter } => match self.def {
                ListLikeDef::List(def) => {
                    let vtable = def.iter_vtable().unwrap();
                    let const_ptr = unsafe { (vtable.next)(*iter)? };
                    // SAFETY: we created this iterator from a PokeListLike which has
                    // mutable access, so converting the const pointer back to mutable is sound.
                    PtrMut::new(const_ptr.as_byte_ptr() as *mut u8)
                }
                _ => unreachable!("non-list list-likes always use Ptr state"),
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

impl<'mem, 'facet> ExactSizeIterator for PokeListLikeIter<'mem, 'facet> {}

impl Drop for PokeListLikeIter<'_, '_> {
    #[inline]
    fn drop(&mut self) {
        if let PokeListLikeIterStateKind::Iter { iter } = &self.state
            && let ListLikeDef::List(def) = self.def
            && let Some(vtable) = def.iter_vtable()
        {
            unsafe { (vtable.dealloc)(*iter) }
        }
    }
}

impl<'mem, 'facet> PokeListLike<'mem, 'facet> {
    /// Creates a new poke list-like
    ///
    /// # Safety
    ///
    /// The caller must ensure that `def` contains valid vtable function pointers that:
    /// - Correctly implement the list-like operations for the actual type
    /// - Do not cause undefined behavior when called
    /// - Return pointers within valid memory bounds
    /// - Match the element type specified in `def.t()`
    #[inline]
    pub unsafe fn new(value: Poke<'mem, 'facet>, def: ListLikeDef) -> Self {
        let len = match def {
            ListLikeDef::List(v) => unsafe { (v.vtable.len)(value.data()) },
            ListLikeDef::Slice(_) => {
                let slice_as_units = unsafe { value.data().get::<[()]>() };
                slice_as_units.len()
            }
            ListLikeDef::Array(v) => v.n,
        };
        Self { value, def, len }
    }

    /// Get the length of the list-like.
    #[inline]
    pub const fn len(&self) -> usize {
        self.len
    }

    /// Returns true if the list-like is empty.
    #[inline]
    pub const fn is_empty(&self) -> bool {
        self.len == 0
    }

    /// Def getter.
    #[inline]
    pub const fn def(&self) -> ListLikeDef {
        self.def
    }

    /// Get a read-only `Peek` for the item at the specified index.
    #[inline]
    pub fn get(&self, index: usize) -> Option<crate::Peek<'_, 'facet>> {
        self.as_peek_list_like().get(index)
    }

    /// Get a mutable `Poke` for the item at the specified index.
    pub fn get_mut(&mut self, index: usize) -> Option<Poke<'_, 'facet>> {
        if index >= self.len {
            return None;
        }

        let item_ptr = match self.def {
            ListLikeDef::List(def) => {
                let get_mut_fn = def.vtable.get_mut?;
                unsafe { get_mut_fn(self.value.data_mut(), index, self.value.shape())? }
            }
            ListLikeDef::Array(def) => {
                let elem_layout = match self.def.t().layout {
                    ShapeLayout::Sized(layout) => layout,
                    ShapeLayout::Unsized => return None,
                };
                let base = unsafe { (def.vtable.as_mut_ptr)(self.value.data_mut()) };
                unsafe { base.field(index * elem_layout.size()) }
            }
            ListLikeDef::Slice(def) => {
                let elem_layout = match self.def.t().layout {
                    ShapeLayout::Sized(layout) => layout,
                    ShapeLayout::Unsized => return None,
                };
                let base = unsafe { (def.vtable.as_mut_ptr)(self.value.data_mut()) };
                unsafe { base.field(index * elem_layout.size()) }
            }
        };

        Some(unsafe { Poke::from_raw_parts(item_ptr, self.def.t()) })
    }

    /// Returns a mutable iterator over the list-like.
    pub fn iter_mut(self) -> PokeListLikeIter<'mem, 'facet> {
        let state = match self.def {
            ListLikeDef::List(def) => {
                if let Some(as_mut_ptr_fn) = def.vtable.as_mut_ptr {
                    let data = unsafe { as_mut_ptr_fn(self.value.data) };
                    let layout = self
                        .def
                        .t()
                        .layout
                        .sized_layout()
                        .expect("can only iterate over sized list-like elements");
                    PokeListLikeIterStateKind::Ptr {
                        data,
                        stride: layout.size(),
                    }
                } else {
                    let iter = unsafe {
                        (def.iter_vtable().unwrap().init_with_value.unwrap())(self.value.data())
                    };
                    PokeListLikeIterStateKind::Iter { iter }
                }
            }
            ListLikeDef::Array(def) => {
                let data = unsafe { (def.vtable.as_mut_ptr)(self.value.data) };
                let layout = self
                    .def
                    .t()
                    .layout
                    .sized_layout()
                    .expect("can only iterate over sized array elements");
                PokeListLikeIterStateKind::Ptr {
                    data,
                    stride: layout.size(),
                }
            }
            ListLikeDef::Slice(def) => {
                let data = unsafe { (def.vtable.as_mut_ptr)(self.value.data) };
                let layout = self
                    .def
                    .t()
                    .layout
                    .sized_layout()
                    .expect("can only iterate over sized slice elements");
                PokeListLikeIterStateKind::Ptr {
                    data,
                    stride: layout.size(),
                }
            }
        };

        PokeListLikeIter {
            state,
            index: 0,
            len: self.len,
            def: self.def,
            _list: PhantomData,
        }
    }

    /// Converts this `PokeListLike` back into a `Poke`.
    #[inline]
    pub fn into_inner(self) -> Poke<'mem, 'facet> {
        self.value
    }

    /// Returns a read-only `PeekListLike` view.
    #[inline]
    pub fn as_peek_list_like(&self) -> crate::PeekListLike<'_, 'facet> {
        unsafe { crate::PeekListLike::new(self.value.as_peek(), self.def) }
    }
}

impl<'mem, 'facet> IntoIterator for PokeListLike<'mem, 'facet> {
    type Item = Poke<'mem, 'facet>;
    type IntoIter = PokeListLikeIter<'mem, 'facet>;

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
    fn poke_list_like_vec_len_and_get_mut() {
        let mut v: Vec<i32> = alloc::vec![1, 2, 3];
        let poke = Poke::new(&mut v);
        let mut ll = poke.into_list_like().unwrap();
        assert_eq!(ll.len(), 3);

        {
            let mut item = ll.get_mut(1).unwrap();
            item.set(200i32).unwrap();
        }
        assert_eq!(v, alloc::vec![1, 200, 3]);
    }

    #[test]
    fn poke_list_like_array_get_mut() {
        let mut arr: [i32; 3] = [10, 20, 30];
        let poke = Poke::new(&mut arr);
        let mut ll = poke.into_list_like().unwrap();
        assert_eq!(ll.len(), 3);

        {
            let mut item = ll.get_mut(0).unwrap();
            item.set(99i32).unwrap();
        }
        assert_eq!(arr, [99, 20, 30]);
    }

    #[test]
    fn poke_list_like_iter_mut() {
        let mut v: Vec<i32> = alloc::vec![1, 2, 3];
        let poke = Poke::new(&mut v);
        let ll = poke.into_list_like().unwrap();
        for mut item in ll {
            let cur = *item.get::<i32>().unwrap();
            item.set(cur * 10).unwrap();
        }
        assert_eq!(v, alloc::vec![10, 20, 30]);
    }
}
