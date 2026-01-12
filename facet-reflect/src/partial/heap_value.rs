use crate::Peek;
use crate::ReflectError;
use crate::trace;
use core::ptr::NonNull;
use core::{alloc::Layout, marker::PhantomData};
use facet_core::{Facet, PtrConst, PtrMut, Shape};

/// A type-erased value stored on the heap
///
/// The `BORROW` const generic indicates whether this value may contain borrowed data:
/// - `BORROW = true` (default): The value may contain references with lifetime `'facet`
/// - `BORROW = false`: The value is fully owned and contains no borrowed data
pub struct HeapValue<'facet, const BORROW: bool = true> {
    pub(crate) guard: Option<Guard>,
    pub(crate) shape: &'static Shape,
    pub(crate) phantom: PhantomData<&'facet ()>,
}

impl<'facet, const BORROW: bool> Drop for HeapValue<'facet, BORROW> {
    fn drop(&mut self) {
        if let Some(guard) = self.guard.take() {
            unsafe {
                self.shape
                    .call_drop_in_place(PtrMut::new(guard.ptr.as_ptr()));
            }
            drop(guard);
        }
    }
}

impl<'facet, const BORROW: bool> HeapValue<'facet, BORROW> {
    /// Returns a peek that allows exploring the heap value.
    pub fn peek(&self) -> Peek<'_, 'facet> {
        unsafe {
            Peek::unchecked_new(
                PtrConst::new(self.guard.as_ref().unwrap().ptr.as_ptr()),
                self.shape,
            )
        }
    }

    /// Returns the shape of this heap value.
    pub const fn shape(&self) -> &'static Shape {
        self.shape
    }

    /// Turn this heapvalue into a concrete type
    pub fn materialize<T: Facet<'facet>>(mut self) -> Result<T, ReflectError> {
        trace!(
            "HeapValue::materialize: Materializing heap value with shape {} to type {}",
            self.shape,
            T::SHAPE
        );
        if self.shape != T::SHAPE {
            trace!(
                "HeapValue::materialize: Shape mismatch! Expected {}, but heap value has {}",
                T::SHAPE,
                self.shape
            );
            return Err(ReflectError::WrongShape {
                expected: self.shape,
                actual: T::SHAPE,
            });
        }

        trace!("HeapValue::materialize: Shapes match, proceeding with materialization");
        let guard = self.guard.take().unwrap();
        let data = PtrConst::new(guard.ptr.as_ptr());
        let res = unsafe { data.read::<T>() };
        drop(guard); // free memory (but don't drop in place)
        trace!("HeapValue::materialize: Successfully materialized value");
        Ok(res)
    }
}

impl<'facet, const BORROW: bool> HeapValue<'facet, BORROW> {
    /// Formats the value using its Display implementation, if available
    pub fn fmt_display(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        let ptr = PtrConst::new(self.guard.as_ref().unwrap().ptr.as_ptr());
        if let Some(result) = unsafe { self.shape.call_display(ptr, f) } {
            return result;
        }
        write!(f, "⟨{}⟩", self.shape)
    }

    /// Formats the value using its Debug implementation, if available
    pub fn fmt_debug(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        let ptr = PtrConst::new(self.guard.as_ref().unwrap().ptr.as_ptr());
        if let Some(result) = unsafe { self.shape.call_debug(ptr, f) } {
            return result;
        }
        write!(f, "⟨{}⟩", self.shape)
    }
}

impl<'facet, const BORROW: bool> core::fmt::Display for HeapValue<'facet, BORROW> {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        self.fmt_display(f)
    }
}

impl<'facet, const BORROW: bool> core::fmt::Debug for HeapValue<'facet, BORROW> {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        self.fmt_debug(f)
    }
}

impl<'facet, const BORROW: bool> PartialEq for HeapValue<'facet, BORROW> {
    fn eq(&self, other: &Self) -> bool {
        if self.shape != other.shape {
            return false;
        }
        let self_ptr = PtrConst::new(self.guard.as_ref().unwrap().ptr.as_ptr());
        let other_ptr = PtrConst::new(other.guard.as_ref().unwrap().ptr.as_ptr());
        unsafe { self.shape.call_partial_eq(self_ptr, other_ptr) }.unwrap_or(false)
    }
}

impl<'facet, const BORROW: bool> PartialOrd for HeapValue<'facet, BORROW> {
    fn partial_cmp(&self, other: &Self) -> Option<core::cmp::Ordering> {
        if self.shape != other.shape {
            return None;
        }
        let self_ptr = PtrConst::new(self.guard.as_ref().unwrap().ptr.as_ptr());
        let other_ptr = PtrConst::new(other.guard.as_ref().unwrap().ptr.as_ptr());
        unsafe { self.shape.call_partial_cmp(self_ptr, other_ptr) }.flatten()
    }
}

/// A guard structure to manage memory allocation and deallocation.
///
/// This struct holds a raw pointer to the allocated memory and the layout
/// information used for allocation. It's responsible for deallocating
/// the memory when dropped, unless the allocation is managed elsewhere.
pub struct Guard {
    /// Raw pointer to the allocated memory.
    pub(crate) ptr: NonNull<u8>,
    /// Layout information of the allocated memory.
    pub(crate) layout: Layout,
    /// Whether this guard should deallocate the memory on drop.
    /// Set to false when the allocation is managed elsewhere (e.g., `Arc<[T]>` from slice builder).
    pub(crate) should_dealloc: bool,
}

impl Drop for Guard {
    fn drop(&mut self) {
        if self.should_dealloc && self.layout.size() != 0 {
            trace!(
                "Deallocating memory at ptr: {:p}, size: {}, align: {}",
                self.ptr,
                self.layout.size(),
                self.layout.align()
            );
            // SAFETY: `ptr` has been allocated via the global allocator with the given layout
            unsafe { alloc::alloc::dealloc(self.ptr.as_ptr(), self.layout) };
        }
    }
}

impl<'facet, const BORROW: bool> HeapValue<'facet, BORROW> {
    /// Unsafely get a reference to the underlying value as type T.
    ///
    /// # Safety
    ///
    /// Caller must guarantee that the underlying value is of type T.
    pub const unsafe fn as_ref<T>(&self) -> &T {
        unsafe { &*(self.guard.as_ref().unwrap().ptr.as_ptr() as *const T) }
    }
}
