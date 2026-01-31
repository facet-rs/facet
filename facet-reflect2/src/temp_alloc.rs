//! Temporary allocation utilities for shape-based memory management.

use std::alloc::Layout;
use std::ptr::NonNull;

use facet_core::{PtrConst, PtrUninit, Shape};

use crate::errors::ReflectErrorKind;

/// A temporary allocation for a value of a given shape.
///
/// This owns the memory and will deallocate it on drop.
/// The value may or may not be initialized - tracked by the `initialized` flag.
pub struct TempAlloc {
    ptr: PtrUninit,
    shape: &'static Shape,
    layout: Layout,
    initialized: bool,
}

impl TempAlloc {
    /// Allocate temporary storage for a value of the given shape.
    ///
    /// Returns an error if the shape is unsized or allocation fails.
    pub fn new(shape: &'static Shape) -> Result<Self, ReflectErrorKind> {
        let layout = shape
            .layout
            .sized_layout()
            .map_err(|_| ReflectErrorKind::Unsized { shape })?;

        let ptr = if layout.size() == 0 {
            PtrUninit::new(NonNull::<u8>::dangling().as_ptr())
        } else {
            // SAFETY: layout has non-zero size
            let raw = unsafe { std::alloc::alloc(layout) };
            if raw.is_null() {
                return Err(ReflectErrorKind::AllocFailed { layout });
            }
            PtrUninit::new(raw)
        };

        Ok(Self {
            ptr,
            shape,
            layout,
            initialized: false,
        })
    }

    /// Get the pointer to the allocation.
    pub fn ptr(&self) -> PtrUninit {
        self.ptr
    }

    /// Copy a value into the allocation, marking it as initialized.
    ///
    /// # Safety
    ///
    /// - `src` must point to a valid, initialized value matching the shape
    pub unsafe fn copy_from(&mut self, src: PtrConst) {
        debug_assert!(!self.initialized, "already initialized");
        // SAFETY: caller guarantees src points to valid data matching self.shape
        unsafe {
            self.ptr.copy_from(src, self.shape).unwrap();
        }
        self.initialized = true;
    }

    /// Initialize the allocation with the type's default value.
    ///
    /// Returns `None` if the type has no default.
    pub fn init_default(&mut self) -> Option<()> {
        debug_assert!(!self.initialized, "already initialized");
        // SAFETY: ptr points to uninitialized memory of the correct layout
        let result = unsafe { self.shape.call_default_in_place(self.ptr) };
        if result.is_some() {
            self.initialized = true;
        }
        result
    }

    /// Mark the value as moved out (will not be dropped on deallocation).
    ///
    /// Call this after the value has been consumed by a move operation.
    pub fn mark_moved(&mut self) {
        self.initialized = false;
    }
}

impl Drop for TempAlloc {
    fn drop(&mut self) {
        // Drop the value if initialized
        if self.initialized {
            // SAFETY: initialized means the value is valid
            unsafe {
                self.shape.call_drop_in_place(self.ptr.assume_init());
            }
        }

        // Deallocate the memory
        if self.layout.size() > 0 {
            // SAFETY: we allocated this memory with this layout
            unsafe {
                std::alloc::dealloc(self.ptr.as_mut_byte_ptr(), self.layout);
            }
        }
    }
}
