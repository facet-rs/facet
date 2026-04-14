use super::Poke;
use core::{fmt::Debug, marker::PhantomData};
use facet_core::{ListDef, PtrMut, Shape};

use crate::{ReflectError, ReflectErrorKind};

/// Iterator over a `PokeList` yielding mutable `Poke`s.
///
/// Constructed by [`PokeList::iter_mut`]. Walks element strides starting from the list's
/// `as_mut_ptr`; `iter_mut` refuses to build one if that entry is missing or the element
/// type is unsized.
pub struct PokeListIter<'mem, 'facet> {
    data: PtrMut,
    stride: usize,
    index: usize,
    len: usize,
    elem_shape: &'static Shape,
    _list: PhantomData<Poke<'mem, 'facet>>,
}

impl<'mem, 'facet> Iterator for PokeListIter<'mem, 'facet> {
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

impl ExactSizeIterator for PokeListIter<'_, '_> {}

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

    /// Returns a mutable iterator over the list.
    ///
    /// Requires contiguous mutable access: the element type must be sized and the list
    /// vtable must expose `as_mut_ptr`. Returns [`ReflectErrorKind::OperationFailed`] otherwise;
    /// use [`PokeList::get_mut`] per index when `iter_mut` is unavailable.
    ///
    /// The previous fallback that synthesized a mutable iterator from the list's `iter_vtable`
    /// was unsound: that vtable yields `PtrConst` items backed by shared references, and writing
    /// through them is UB.
    pub fn iter_mut(self) -> Result<PokeListIter<'mem, 'facet>, ReflectError> {
        let elem_shape = self.def.t();
        let stride = match elem_shape.layout {
            facet_core::ShapeLayout::Sized(layout) => layout.size(),
            facet_core::ShapeLayout::Unsized => {
                return Err(self.value.err(ReflectErrorKind::OperationFailed {
                    shape: self.value.shape(),
                    operation: "iter_mut requires sized element type",
                }));
            }
        };

        let Some(as_mut_ptr_fn) = self.def.vtable.as_mut_ptr else {
            return Err(self.value.err(ReflectErrorKind::OperationFailed {
                shape: self.value.shape(),
                operation:
                    "iter_mut requires a contiguous `as_mut_ptr` vtable entry; use `get_mut` per index",
            }));
        };

        let data = unsafe { as_mut_ptr_fn(self.value.data) };
        let len = self.len();
        Ok(PokeListIter {
            data,
            stride,
            index: 0,
            len,
            elem_shape,
            _list: PhantomData,
        })
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
        for mut item in list.iter_mut().unwrap() {
            let val = *item.get::<i32>().unwrap();
            item.set(val * 10).unwrap();
            sum += val;
        }

        assert_eq!(sum, 6);
        assert_eq!(v, alloc::vec![10, 20, 30]);
    }
}
