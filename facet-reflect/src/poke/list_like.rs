use core::{fmt::Debug, marker::PhantomData};
use facet_core::{PtrMut, Shape, ShapeLayout};

use crate::{ReflectError, ReflectErrorKind, peek::ListLikeDef};

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

/// Iterator over a `PokeListLike` yielding mutable `Poke`s.
///
/// Constructed by [`PokeListLike::iter_mut`]. Only contiguous list-likes support
/// mutable iteration — this iterator walks element strides starting from
/// `as_mut_ptr`. See [`PokeListLike::iter_mut`] for the error conditions.
pub struct PokeListLikeIter<'mem, 'facet> {
    data: PtrMut,
    stride: usize,
    index: usize,
    len: usize,
    elem_shape: &'static Shape,
    _list: PhantomData<Poke<'mem, 'facet>>,
}

impl<'mem, 'facet> Iterator for PokeListLikeIter<'mem, 'facet> {
    type Item = Poke<'mem, 'facet>;

    #[inline]
    fn next(&mut self) -> Option<Self::Item> {
        if self.index >= self.len {
            return None;
        }
        let item_ptr = unsafe { self.data.field(self.stride * self.index) };
        self.index += 1;
        Some(unsafe { Poke::from_raw_parts(item_ptr, self.elem_shape) })
    }

    #[inline]
    fn size_hint(&self) -> (usize, Option<usize>) {
        let remaining = self.len.saturating_sub(self.index);
        (remaining, Some(remaining))
    }
}

impl<'mem, 'facet> ExactSizeIterator for PokeListLikeIter<'mem, 'facet> {}

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

    fn err(&self, kind: ReflectErrorKind) -> ReflectError {
        self.value.err(kind)
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
    ///
    /// Requires contiguous mutable access to the backing storage: the element type must be
    /// sized, and for `List` the vtable must expose `as_mut_ptr`. (`Array` and `Slice` always
    /// expose `as_mut_ptr`.) Returns [`ReflectErrorKind::OperationFailed`] if either condition
    /// fails; use [`PokeListLike::get_mut`] per index when `iter_mut` is unavailable.
    ///
    /// The previous fallback that synthesized a mutable iterator from the list's `iter_vtable`
    /// was unsound: that vtable yields `PtrConst` items backed by shared references, and
    /// writing through them is UB.
    pub fn iter_mut(self) -> Result<PokeListLikeIter<'mem, 'facet>, ReflectError> {
        let elem_shape = self.def.t();
        let stride = match elem_shape.layout {
            ShapeLayout::Sized(layout) => layout.size(),
            ShapeLayout::Unsized => {
                return Err(self.err(ReflectErrorKind::OperationFailed {
                    shape: self.value.shape,
                    operation: "iter_mut requires sized element type",
                }));
            }
        };

        let data = match self.def {
            ListLikeDef::List(def) => match def.vtable.as_mut_ptr {
                Some(as_mut_ptr_fn) => unsafe { as_mut_ptr_fn(self.value.data) },
                None => {
                    return Err(self.err(ReflectErrorKind::OperationFailed {
                        shape: self.value.shape,
                        operation:
                            "iter_mut requires a contiguous `as_mut_ptr` vtable entry; use `get_mut` per index",
                    }));
                }
            },
            ListLikeDef::Array(def) => unsafe { (def.vtable.as_mut_ptr)(self.value.data) },
            ListLikeDef::Slice(def) => unsafe { (def.vtable.as_mut_ptr)(self.value.data) },
        };

        Ok(PokeListLikeIter {
            data,
            stride,
            index: 0,
            len: self.len,
            elem_shape,
            _list: PhantomData,
        })
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
        for mut item in ll.iter_mut().unwrap() {
            let cur = *item.get::<i32>().unwrap();
            item.set(cur * 10).unwrap();
        }
        assert_eq!(v, alloc::vec![10, 20, 30]);
    }
}
