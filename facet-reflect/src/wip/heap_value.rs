use crate::Peek;
use crate::ReflectError;
use crate::trace;
use core::{alloc::Layout, marker::PhantomData};
use facet_core::{Facet, PtrConst, PtrMut, Shape};
#[cfg(feature = "log")]
use owo_colors::OwoColorize as _;

/// A type-erased value stored on the heap
pub struct HeapValue<'facet, 'shape> {
    pub(crate) guard: Option<Guard>,
    pub(crate) shape: &'shape Shape<'shape>,
    pub(crate) phantom: PhantomData<&'facet ()>,
}

impl<'facet, 'shape> Drop for HeapValue<'facet, 'shape> {
    fn drop(&mut self) {
        if let Some(guard) = self.guard.take() {
            if let Some(drop_fn) = (self.shape.vtable.drop_in_place)() {
                unsafe { drop_fn(PtrMut::new(guard.ptr)) };
            }
            drop(guard);
        }
    }
}

impl<'facet, 'shape> HeapValue<'facet, 'shape> {
    /// Returns a peek that allows exploring the heap value.
    pub fn peek(&self) -> Peek<'_, 'facet, 'shape> {
        unsafe { Peek::unchecked_new(PtrConst::new(self.guard.as_ref().unwrap().ptr), self.shape) }
    }

    /// Turn this heapvalue into a concrete type
    pub fn materialize<T: Facet<'facet>>(mut self) -> Result<T, ReflectError<'shape>> {
        if self.shape != T::SHAPE {
            return Err(ReflectError::WrongShape {
                expected: self.shape,
                actual: T::SHAPE,
            });
        }

        let guard = self.guard.take().unwrap();
        let data = PtrConst::new(guard.ptr);
        let res = unsafe { data.read::<T>() };
        drop(guard); // free memory (but don't drop in place)
        Ok(res)
    }
}

impl<'facet, 'shape> HeapValue<'facet, 'shape> {
    /// Formats the value using its Display implementation, if available
    pub fn fmt_display(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        if let Some(display_fn) = (self.shape.vtable.display)() {
            unsafe { display_fn(PtrConst::new(self.guard.as_ref().unwrap().ptr), f) }
        } else {
            write!(f, "⟨{}⟩", self.shape)
        }
    }

    /// Formats the value using its Debug implementation, if available
    pub fn fmt_debug(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        if let Some(debug_fn) = (self.shape.vtable.debug)() {
            unsafe { debug_fn(PtrConst::new(self.guard.as_ref().unwrap().ptr), f) }
        } else {
            write!(f, "⟨{}⟩", self.shape)
        }
    }
}

impl<'facet, 'shape> core::fmt::Display for HeapValue<'facet, 'shape> {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        self.fmt_display(f)
    }
}

impl<'facet, 'shape> core::fmt::Debug for HeapValue<'facet, 'shape> {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        self.fmt_debug(f)
    }
}

impl<'facet, 'shape> PartialEq for HeapValue<'facet, 'shape> {
    fn eq(&self, other: &Self) -> bool {
        if self.shape != other.shape {
            return false;
        }
        if let Some(eq_fn) = (self.shape.vtable.eq)() {
            unsafe {
                eq_fn(
                    PtrConst::new(self.guard.as_ref().unwrap().ptr),
                    PtrConst::new(other.guard.as_ref().unwrap().ptr),
                )
            }
        } else {
            false
        }
    }
}

impl<'facet, 'shape> PartialOrd for HeapValue<'facet, 'shape> {
    fn partial_cmp(&self, other: &Self) -> Option<core::cmp::Ordering> {
        if self.shape != other.shape {
            return None;
        }
        if let Some(partial_ord_fn) = (self.shape.vtable.partial_ord)() {
            unsafe {
                partial_ord_fn(
                    PtrConst::new(self.guard.as_ref().unwrap().ptr),
                    PtrConst::new(other.guard.as_ref().unwrap().ptr),
                )
            }
        } else {
            None
        }
    }
}

/// A guard structure to manage memory allocation and deallocation.
///
/// This struct holds a raw pointer to the allocated memory and the layout
/// information used for allocation. It's responsible for deallocating
/// the memory when dropped.
pub struct Guard {
    /// Raw pointer to the allocated memory.
    pub(crate) ptr: *mut u8,
    /// Layout information of the allocated memory.
    pub(crate) layout: Layout,
}

impl Drop for Guard {
    fn drop(&mut self) {
        if self.layout.size() != 0 {
            trace!(
                "Deallocating memory at ptr: {:p}, size: {}, align: {}",
                self.ptr.cyan(),
                self.layout.size().yellow(),
                self.layout.align().green()
            );
            // SAFETY: `ptr` has been allocated via the global allocator with the given layout
            unsafe { alloc::alloc::dealloc(self.ptr, self.layout) };
        }
    }
}
