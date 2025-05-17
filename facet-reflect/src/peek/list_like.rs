use facet_core::{PtrConst, Shape, ShapeLayout};

use super::Peek;
use core::fmt::Debug;

/// Fields for types which act like lists
#[derive(Clone, Copy)]
pub enum ListLikeDef<'shape> {
    /// Ordered list of heterogenous values, variable size
    ///
    /// e.g. `Vec<T>`
    List(facet_core::ListDef<'shape>),

    /// Fixed-size array of heterogenous values
    ///
    /// e.g. `[T; 32]`
    Array(facet_core::ArrayDef<'shape>),

    /// Slice — a reference to a contiguous sequence of elements
    ///
    /// e.g. `&[T]`
    Slice(facet_core::SliceDef<'shape>),
}

impl<'shape> ListLikeDef<'shape> {
    /// Returns the shape of the items in the list
    pub fn t(&self) -> &'shape Shape<'shape> {
        match self {
            ListLikeDef::List(v) => v.t(),
            ListLikeDef::Array(v) => v.t(),
            ListLikeDef::Slice(v) => v.t(),
        }
    }
}

/// Iterator over a `PeekListLike`
pub struct PeekListLikeIter<'mem, 'facet, 'shape> {
    list: PeekListLike<'mem, 'facet, 'shape>,
    index: usize,
    len: usize,
}

impl<'mem, 'facet, 'shape> Iterator for PeekListLikeIter<'mem, 'facet, 'shape> {
    type Item = Peek<'mem, 'facet, 'shape>;

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

impl<'mem, 'facet, 'shape> ExactSizeIterator for PeekListLikeIter<'mem, 'facet, 'shape> {}

impl<'mem, 'facet, 'shape> IntoIterator for &'mem PeekListLike<'mem, 'facet, 'shape> {
    type Item = Peek<'mem, 'facet, 'shape>;
    type IntoIter = PeekListLikeIter<'mem, 'facet, 'shape>;

    fn into_iter(self) -> Self::IntoIter {
        self.iter()
    }
}

/// Lets you read from a list, array or slice
#[derive(Clone, Copy)]
pub struct PeekListLike<'mem, 'facet, 'shape> {
    pub(crate) value: Peek<'mem, 'facet, 'shape>,
    pub(crate) def: ListLikeDef<'shape>,
    len: usize,
    as_ptr: unsafe fn(this: PtrConst) -> PtrConst,
}

impl<'mem, 'facet, 'shape> Debug for PeekListLike<'mem, 'facet, 'shape> {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("PeekListLike").finish_non_exhaustive()
    }
}

impl<'mem, 'facet, 'shape> PeekListLike<'mem, 'facet, 'shape> {
    /// Creates a new peek list
    pub fn new(value: Peek<'mem, 'facet, 'shape>, def: ListLikeDef<'shape>) -> Self {
        let (len, as_ptr_fn) = match def {
            ListLikeDef::List(v) => (unsafe { (v.vtable.len)(value.data()) }, v.vtable.as_ptr),
            ListLikeDef::Slice(v) => (unsafe { (v.vtable.len)(value.data()) }, v.vtable.as_ptr),
            ListLikeDef::Array(v) => (v.n, v.vtable.as_ptr),
        };
        Self {
            value,
            def,
            len,
            as_ptr: as_ptr_fn,
        }
    }

    /// Get the length of the list
    pub fn len(&self) -> usize {
        self.len
    }

    /// Returns true if the list is empty
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// Get an item from the list at the specified index
    ///
    /// # Panics
    ///
    /// Panics if the index is out of bounds
    pub fn get(&self, index: usize) -> Option<Peek<'mem, 'facet, 'shape>> {
        if index >= self.len() {
            return None;
        }

        // Get the base pointer of the array
        let base_ptr = unsafe { (self.as_ptr)(self.value.data()) };

        // Get the layout of the element type
        let elem_layout = match self.def.t().layout {
            ShapeLayout::Sized(layout) => layout,
            ShapeLayout::Unsized => return None, // Cannot handle unsized elements
        };

        // Calculate the offset based on element size
        let offset = index * elem_layout.size();

        // Apply the offset to get the item's pointer
        let item_ptr = unsafe { base_ptr.field(offset) };

        Some(unsafe { Peek::unchecked_new(item_ptr, self.def.t()) })
    }

    /// Returns an iterator over the list
    pub fn iter(self) -> PeekListLikeIter<'mem, 'facet, 'shape> {
        PeekListLikeIter {
            list: self,
            index: 0,
            len: self.len(),
        }
    }

    /// Def getter
    pub fn def(&self) -> ListLikeDef<'shape> {
        self.def
    }
}
