use facet_core::{GenericPtr, IterVTable, PtrConst, PtrMut, Shape, ShapeLayout};

use super::Peek;
use core::{fmt::Debug, marker::PhantomData};

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
    #[inline]
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
    state: PeekListLikeIterState<'mem>,
    index: usize,
    len: usize,
    def: ListLikeDef<'shape>,
    _list: PhantomData<Peek<'mem, 'facet, 'shape>>,
}

impl<'mem, 'facet, 'shape> Iterator for PeekListLikeIter<'mem, 'facet, 'shape> {
    type Item = Peek<'mem, 'facet, 'shape>;

    #[inline]
    fn next(&mut self) -> Option<Self::Item> {
        let item_ptr = match self.state {
            PeekListLikeIterState::Ptr { data, stride } => {
                if self.index >= self.len {
                    return None;
                }

                unsafe { data.field(stride * self.index) }
            }
            PeekListLikeIterState::Iter { iter, vtable } => unsafe { (vtable.next)(iter)? },
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

impl<'mem, 'facet, 'shape> ExactSizeIterator for PeekListLikeIter<'mem, 'facet, 'shape> {}

impl<'mem, 'facet, 'shape> IntoIterator for &'mem PeekListLike<'mem, 'facet, 'shape> {
    type Item = Peek<'mem, 'facet, 'shape>;
    type IntoIter = PeekListLikeIter<'mem, 'facet, 'shape>;

    #[inline]
    fn into_iter(self) -> Self::IntoIter {
        self.iter()
    }
}

enum PeekListLikeIterState<'mem> {
    Ptr {
        data: PtrConst<'mem>,
        stride: usize,
    },
    Iter {
        iter: PtrMut<'mem>,
        vtable: IterVTable<PtrConst<'static>>,
    },
}

impl Drop for PeekListLikeIterState<'_> {
    #[inline]
    fn drop(&mut self) {
        match self {
            Self::Iter { iter, vtable } => unsafe { (vtable.dealloc)(*iter) },
            Self::Ptr { .. } => {
                // Nothing to do
            }
        }
    }
}

/// Lets you read from a list, array or slice
#[derive(Clone, Copy)]
pub struct PeekListLike<'mem, 'facet, 'shape> {
    pub(crate) value: Peek<'mem, 'facet, 'shape>,
    pub(crate) def: ListLikeDef<'shape>,
    len: usize,
}

impl<'mem, 'facet, 'shape> Debug for PeekListLike<'mem, 'facet, 'shape> {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("PeekListLike").finish_non_exhaustive()
    }
}

impl<'mem, 'facet, 'shape> PeekListLike<'mem, 'facet, 'shape> {
    /// Creates a new peek list
    #[inline]
    pub fn new(value: Peek<'mem, 'facet, 'shape>, def: ListLikeDef<'shape>) -> Self {
        let len = match def {
            ListLikeDef::List(v) => unsafe { (v.vtable.len)(value.data().thin().unwrap()) },
            ListLikeDef::Slice(v) => {
                // Check if we have a bare slice with wide pointer (e.g., from Arc<[T]>::borrow_inner)
                // or a reference to a slice with thin pointer
                match value.data() {
                    GenericPtr::Wide(wide_ptr) => {
                        // For bare slices, we need to extract the length from the wide pointer
                        // We can safely cast to any slice type to get the length since it's metadata
                        let slice_as_units = unsafe { wide_ptr.get::<[()]>() };
                        slice_as_units.len()
                    }
                    GenericPtr::Thin(thin_ptr) => {
                        // For references to slices, use the vtable
                        unsafe { (v.vtable.len)(thin_ptr) }
                    }
                }
            }
            ListLikeDef::Array(v) => v.n,
        };
        Self { value, def, len }
    }

    /// Get the length of the list
    #[inline]
    pub fn len(&self) -> usize {
        self.len
    }

    /// Returns true if the list is empty
    #[inline]
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// Get an item from the list at the specified index
    ///
    /// Return `None` if the index is out of bounds
    pub fn get(&self, index: usize) -> Option<Peek<'mem, 'facet, 'shape>> {
        // Special handling for bare slices with wide pointers
        if let (ListLikeDef::Slice(_), GenericPtr::Wide(wide_ptr)) = (&self.def, self.value.data())
        {
            if index >= self.len() {
                return None;
            }

            // Get the element type layout
            let elem_layout = match self.def.t().layout {
                ShapeLayout::Sized(layout) => layout,
                ShapeLayout::Unsized => return None,
            };

            // Get the data pointer directly from the wide pointer
            let data_ptr = wide_ptr.as_byte_ptr();

            // Calculate the element pointer
            let elem_ptr = unsafe { data_ptr.add(index * elem_layout.size()) };

            // Create a Peek for the element
            return Some(unsafe {
                Peek::unchecked_new(GenericPtr::Thin(PtrConst::new(elem_ptr)), self.def.t())
            });
        }

        let as_ptr = match self.def {
            ListLikeDef::List(def) => {
                // Call get from the list's vtable directly if available
                let item = unsafe { (def.vtable.get)(self.value.data().thin().unwrap(), index)? };
                return Some(unsafe { Peek::unchecked_new(item, self.def.t()) });
            }
            ListLikeDef::Array(def) => def.vtable.as_ptr,
            ListLikeDef::Slice(def) => def.vtable.as_ptr,
        };

        if index >= self.len() {
            return None;
        }

        // Get the base pointer of the array
        let base_ptr = unsafe { as_ptr(self.value.data().thin().unwrap()) };

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
        let (as_ptr_fn, iter_vtable) = match self.def {
            ListLikeDef::List(def) => (def.vtable.as_ptr, Some(def.vtable.iter_vtable)),
            ListLikeDef::Array(def) => (Some(def.vtable.as_ptr), None),
            ListLikeDef::Slice(def) => (Some(def.vtable.as_ptr), None),
        };

        let state = match (as_ptr_fn, iter_vtable) {
            (Some(as_ptr_fn), _) => {
                // Special handling for bare slices with wide pointers
                let data = if let (ListLikeDef::Slice(_), GenericPtr::Wide(wide_ptr)) =
                    (&self.def, self.value.data())
                {
                    // Get the data pointer directly from the wide pointer
                    PtrConst::new(wide_ptr.as_byte_ptr())
                } else {
                    unsafe { as_ptr_fn(self.value.data().thin().unwrap()) }
                };

                let layout = self
                    .def
                    .t()
                    .layout
                    .sized_layout()
                    .expect("can only iterate over sized list elements");
                let stride = layout.size();

                PeekListLikeIterState::Ptr { data, stride }
            }
            (None, Some(vtable)) => {
                let iter =
                    unsafe { (vtable.init_with_value.unwrap())(self.value.data().thin().unwrap()) };
                PeekListLikeIterState::Iter { iter, vtable }
            }
            (None, None) => unreachable!(),
        };

        PeekListLikeIter {
            state,
            index: 0,
            len: self.len(),
            def: self.def(),
            _list: PhantomData,
        }
    }

    /// Def getter
    #[inline]
    pub fn def(&self) -> ListLikeDef<'shape> {
        self.def
    }
}
