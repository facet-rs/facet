use facet_core::{PtrConst, Shape, ShapeLayout};

use super::Peek;
use core::fmt::Debug;

/// Fields for types which act like lists
#[derive(Clone, Copy)]
pub enum ListLikeDef {
    /// Ordered list of heterogenous values, variable size
    ///
    /// e.g. `Vec<T>`
    List(facet_core::ListDef),

    /// Fixed-size array of heterogenous values
    ///
    /// e.g. `[T; 32]`
    Array(facet_core::ArrayDef),

    /// Slice — a reference to a contiguous sequence of elements
    ///
    /// e.g. `&[T]`
    Slice(facet_core::SliceDef),
}

impl ListLikeDef {
    /// Returns the shape of the items in the list
    pub fn t(&self) -> &'static Shape {
        match self {
            ListLikeDef::List(v) => v.t(),
            ListLikeDef::Array(v) => v.t(),
            ListLikeDef::Slice(v) => v.t(),
        }
    }
}

/// Iterator over a `PeekListLike`
pub struct PeekListLikeIter<'mem, 'facet_lifetime> {
    list: PeekListLike<'mem, 'facet_lifetime>,
    index: usize,
    len: usize,
}

impl<'mem, 'facet_lifetime> Iterator for PeekListLikeIter<'mem, 'facet_lifetime> {
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

impl ExactSizeIterator for PeekListLikeIter<'_, '_> {}

impl<'mem, 'facet_lifetime> IntoIterator for &'mem PeekListLike<'mem, 'facet_lifetime> {
    type Item = Peek<'mem, 'facet_lifetime>;
    type IntoIter = PeekListLikeIter<'mem, 'facet_lifetime>;

    fn into_iter(self) -> Self::IntoIter {
        self.iter()
    }
}

/// Lets you read from a list, array or slice
#[derive(Clone, Copy)]
pub struct PeekListLike<'mem, 'facet_lifetime> {
    pub(crate) value: Peek<'mem, 'facet_lifetime>,
    pub(crate) def: ListLikeDef,
    len: usize,
    as_ptr: unsafe fn(this: PtrConst) -> PtrConst,
}

impl Debug for PeekListLike<'_, '_> {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("PeekListLike").finish_non_exhaustive()
    }
}

impl<'mem, 'facet_lifetime> PeekListLike<'mem, 'facet_lifetime> {
    /// Creates a new peek list
    pub fn new(value: Peek<'mem, 'facet_lifetime>, def: ListLikeDef) -> Self {
        let (len, as_ptr_fn) = match def {
            ListLikeDef::List(v) => (
                unsafe { (v.vtable.len)(value.data()) },
                v.vtable.as_ptr.unwrap(),
            ),
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
    pub fn get(&self, index: usize) -> Option<Peek<'mem, 'facet_lifetime>> {
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
    pub fn iter(self) -> PeekListLikeIter<'mem, 'facet_lifetime> {
        PeekListLikeIter {
            list: self,
            index: 0,
            len: self.len(),
        }
    }

    /// Def getter
    pub fn def(&self) -> ListLikeDef {
        self.def
    }
}
