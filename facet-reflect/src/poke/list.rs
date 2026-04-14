use super::Poke;
use core::{fmt::Debug, marker::PhantomData, mem::ManuallyDrop, ptr::NonNull};
use facet_core::{FieldError, ListDef, PtrMut, PtrUninit, Shape};

use crate::{Guard, HeapValue, ReflectError, ReflectErrorKind};

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

    /// Push a value onto the end of the list.
    ///
    /// Returns an error if the underlying list type does not support push (e.g.
    /// immutable lists) or if the value's shape does not match the list's
    /// element type.
    pub fn push<T: facet_core::Facet<'facet>>(&mut self, value: T) -> Result<(), ReflectError> {
        if self.def.t() != T::SHAPE {
            return Err(self.value.err(ReflectErrorKind::WrongShape {
                expected: self.def.t(),
                actual: T::SHAPE,
            }));
        }
        let push_fn = self.push_fn()?;
        let mut value = ManuallyDrop::new(value);
        unsafe {
            let item_ptr = PtrMut::new(&mut value as *mut ManuallyDrop<T> as *mut u8);
            push_fn(self.value.data_mut(), item_ptr);
        }
        Ok(())
    }

    /// Type-erased [`push`](Self::push).
    ///
    /// Accepts a [`HeapValue`] whose shape must match the list's element type.
    /// The value is moved out of the `HeapValue` into the list.
    pub fn push_from_heap<const BORROW: bool>(
        &mut self,
        value: HeapValue<'facet, BORROW>,
    ) -> Result<(), ReflectError> {
        if self.def.t() != value.shape() {
            return Err(self.value.err(ReflectErrorKind::WrongShape {
                expected: self.def.t(),
                actual: value.shape(),
            }));
        }
        let push_fn = self.push_fn()?;
        let mut value = value;
        let guard = value
            .guard
            .take()
            .expect("HeapValue guard was already taken");
        unsafe {
            let item_ptr = PtrMut::new(guard.ptr.as_ptr());
            push_fn(self.value.data_mut(), item_ptr);
        }
        drop(guard);
        Ok(())
    }

    /// Pop the last value off the end of the list.
    ///
    /// Returns `Ok(None)` if the list is empty. Returns an error if the
    /// underlying list type does not support pop.
    pub fn pop(&mut self) -> Result<Option<HeapValue<'facet, true>>, ReflectError> {
        let pop_fn = self.def.pop().ok_or_else(|| {
            self.value.err(ReflectErrorKind::OperationFailed {
                shape: self.value.shape(),
                operation: "pop: list type does not support pop",
            })
        })?;
        let elem_shape = self.def.t();
        let layout = elem_shape.layout.sized_layout().map_err(|_| {
            self.value.err(ReflectErrorKind::Unsized {
                shape: elem_shape,
                operation: "pop",
            })
        })?;
        let ptr = if layout.size() == 0 {
            NonNull::<u8>::dangling()
        } else {
            let raw = unsafe { alloc::alloc::alloc(layout) };
            match NonNull::new(raw) {
                Some(p) => p,
                None => alloc::alloc::handle_alloc_error(layout),
            }
        };
        let out = PtrUninit::new(ptr.as_ptr());
        let popped = unsafe { pop_fn(self.value.data_mut(), out) };
        if !popped {
            if layout.size() != 0 {
                unsafe { alloc::alloc::dealloc(ptr.as_ptr(), layout) };
            }
            return Ok(None);
        }
        Ok(Some(HeapValue {
            guard: Some(Guard {
                ptr,
                layout,
                should_dealloc: layout.size() != 0,
            }),
            shape: elem_shape,
            phantom: PhantomData,
        }))
    }

    /// Swap the elements at indices `a` and `b`.
    ///
    /// Returns an error if the underlying list type does not support swap or
    /// if either index is out of bounds. Swapping an index with itself is a
    /// no-op.
    pub fn swap(&mut self, a: usize, b: usize) -> Result<(), ReflectError> {
        let swap_fn = self.def.vtable.swap.ok_or_else(|| {
            self.value.err(ReflectErrorKind::OperationFailed {
                shape: self.value.shape(),
                operation: "swap: list type does not support swap",
            })
        })?;
        let len = self.len();
        let ok = unsafe { swap_fn(self.value.data_mut(), a, b, self.value.shape()) };
        if !ok {
            let out_of_bounds = if a >= len { a } else { b };
            return Err(self.value.err(ReflectErrorKind::FieldError {
                shape: self.value.shape(),
                field_error: FieldError::IndexOutOfBounds {
                    index: out_of_bounds,
                    bound: len,
                },
            }));
        }
        Ok(())
    }

    /// Resolve the per-T push function or build an error if absent.
    #[inline]
    fn push_fn(&self) -> Result<facet_core::ListPushFn, ReflectError> {
        self.def.push().ok_or_else(|| {
            self.value.err(ReflectErrorKind::OperationFailed {
                shape: self.value.shape(),
                operation: "push: list type does not support push",
            })
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

    #[test]
    fn poke_list_push_pop() {
        let mut v: Vec<i32> = alloc::vec![];
        {
            let poke = Poke::new(&mut v);
            let mut list = poke.into_list().unwrap();
            list.push(1i32).unwrap();
            list.push(2i32).unwrap();
            list.push(3i32).unwrap();
        }
        assert_eq!(v, alloc::vec![1, 2, 3]);

        {
            let poke = Poke::new(&mut v);
            let mut list = poke.into_list().unwrap();
            let popped = list.pop().unwrap().unwrap();
            assert_eq!(popped.materialize::<i32>().unwrap(), 3);
        }
        assert_eq!(v, alloc::vec![1, 2]);
    }

    #[test]
    fn poke_list_pop_empty_returns_none() {
        let mut v: Vec<i32> = alloc::vec![];
        let poke = Poke::new(&mut v);
        let mut list = poke.into_list().unwrap();
        let popped = list.pop().unwrap();
        assert!(popped.is_none());
    }

    #[test]
    fn poke_list_push_wrong_shape_fails() {
        let mut v: Vec<i32> = alloc::vec![];
        let poke = Poke::new(&mut v);
        let mut list = poke.into_list().unwrap();
        let res = list.push(7u32);
        assert!(matches!(
            res,
            Err(ref err) if matches!(err.kind, ReflectErrorKind::WrongShape { .. })
        ));
    }

    #[test]
    fn poke_list_push_from_heap() {
        let mut v: Vec<i32> = alloc::vec![];
        let poke = Poke::new(&mut v);
        let mut list = poke.into_list().unwrap();

        let hv = crate::Partial::alloc::<i32>()
            .unwrap()
            .set(42i32)
            .unwrap()
            .build()
            .unwrap();
        list.push_from_heap(hv).unwrap();
        assert_eq!(v, alloc::vec![42]);
    }

    #[test]
    fn poke_list_pop_string() {
        let mut v: Vec<alloc::string::String> = alloc::vec![
            alloc::string::String::from("a"),
            alloc::string::String::from("b"),
        ];
        let poke = Poke::new(&mut v);
        let mut list = poke.into_list().unwrap();
        let popped = list.pop().unwrap().unwrap();
        assert_eq!(popped.materialize::<alloc::string::String>().unwrap(), "b");
        assert_eq!(v, alloc::vec![alloc::string::String::from("a")]);
    }

    #[test]
    fn poke_list_swap() {
        let mut v: Vec<i32> = alloc::vec![1, 2, 3];
        let poke = Poke::new(&mut v);
        let mut list = poke.into_list().unwrap();
        list.swap(0, 2).unwrap();
        assert_eq!(v, alloc::vec![3, 2, 1]);
    }

    #[test]
    fn poke_list_swap_self_is_noop() {
        let mut v: Vec<i32> = alloc::vec![1, 2, 3];
        let poke = Poke::new(&mut v);
        let mut list = poke.into_list().unwrap();
        list.swap(1, 1).unwrap();
        assert_eq!(v, alloc::vec![1, 2, 3]);
    }

    #[test]
    fn poke_list_swap_out_of_bounds_fails() {
        let mut v: Vec<i32> = alloc::vec![1, 2, 3];
        let poke = Poke::new(&mut v);
        let mut list = poke.into_list().unwrap();
        let res = list.swap(0, 10);
        assert!(matches!(
            res,
            Err(ref err) if matches!(err.kind, ReflectErrorKind::FieldError { .. })
        ));
    }
}
