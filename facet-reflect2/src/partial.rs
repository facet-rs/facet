//! Partial value construction.

use std::alloc::{alloc, dealloc};
use std::marker::PhantomData;
use std::ptr::NonNull;

use crate::arena::{Arena, Idx};
use crate::errors::{ErrorLocation, ReflectError, ReflectErrorKind};
use crate::frame::{Frame, FrameFlags};
use crate::ops::{Op, Path, Source};
use facet_core::{Facet, PtrUninit, Shape};

/// Manages incremental construction of a value.
pub struct Partial<'facet> {
    arena: Arena<Frame>,
    root: Idx<Frame>,
    current: Idx<Frame>,
    _marker: PhantomData<&'facet ()>,
}

impl<'facet> Partial<'facet> {
    /// Allocate for a known type.
    pub fn alloc<T: Facet<'facet>>() -> Result<Self, ReflectError> {
        Self::alloc_shape(T::SHAPE)
    }

    /// Allocate for a dynamic shape.
    pub fn alloc_shape(shape: &'static Shape) -> Result<Self, ReflectError> {
        let layout = shape.layout.sized_layout().map_err(|_| ReflectError {
            location: ErrorLocation {
                shape,
                path: Path::default(),
            },
            kind: ReflectErrorKind::Unsized { shape },
        })?;

        // Allocate memory (handle ZST case)
        let data = if layout.size() == 0 {
            PtrUninit::new(NonNull::<u8>::dangling().as_ptr())
        } else {
            // SAFETY: layout has non-zero size (checked above) and is valid from Shape
            let ptr = unsafe { alloc(layout) };
            if ptr.is_null() {
                return Err(ReflectError {
                    location: ErrorLocation {
                        shape,
                        path: Path::default(),
                    },
                    kind: ReflectErrorKind::AllocFailed { layout },
                });
            }
            PtrUninit::new(ptr)
        };

        // Create frame with OWNS_ALLOC flag
        let mut frame = Frame::new(data, shape);
        frame.flags |= FrameFlags::OWNS_ALLOC;

        // Store in arena
        let mut arena = Arena::new();
        let root = arena.alloc(frame);

        Ok(Self {
            arena,
            root,
            current: root,
            _marker: PhantomData,
        })
    }

    /// Apply a sequence of operations.
    pub fn apply(&mut self, ops: &[Op]) -> Result<(), ReflectError> {
        for op in ops {
            match op {
                Op::Set { path, source } => {
                    // For now: empty path only (setting current frame directly)
                    assert!(path.is_empty(), "non-empty paths not yet supported");

                    match source {
                        Source::Move(mov) => {
                            let frame = self.arena.get_mut(self.current);

                            // Exact shape match required
                            if !frame.shape.is_shape(mov.shape) {
                                return Err(ReflectError {
                                    location: ErrorLocation {
                                        shape: frame.shape,
                                        path: path.clone(),
                                    },
                                    kind: ReflectErrorKind::ShapeMismatch {
                                        expected: frame.shape,
                                        actual: mov.shape,
                                    },
                                });
                            }

                            // SAFETY:
                            // - mov.ptr points to a valid, initialized value of type matching mov.shape
                            //   (caller invariant from Move construction)
                            // - frame.data points to allocated memory of sufficient size
                            //   (guaranteed by alloc_shape using shape.layout)
                            // - shapes match (checked above), so size and alignment are compatible
                            // - copy_from only fails for unsized types, but we verified sized in alloc_shape
                            unsafe {
                                frame.data.copy_from(mov.ptr, frame.shape).unwrap();
                            }

                            frame.flags |= FrameFlags::INIT;
                        }
                        Source::Build(_) => todo!("Build source"),
                        Source::Default => todo!("Default source"),
                    }
                }
            }
        }
        Ok(())
    }

    /// Build the final value, consuming the Partial.
    ///
    /// # Panics
    ///
    /// Panics if `T::SHAPE` does not match the shape passed to `alloc`.
    pub fn build<T: Facet<'facet>>(mut self) -> Result<T, ReflectError> {
        let frame = self.arena.get(self.root);

        // Verify shape matches
        assert!(
            frame.shape.is_shape(T::SHAPE),
            "build() called with wrong type"
        );

        // Verify initialized
        if !frame.flags.contains(FrameFlags::INIT) {
            return Err(ReflectError {
                location: ErrorLocation {
                    shape: frame.shape,
                    path: Path::default(),
                },
                kind: ReflectErrorKind::NotInitialized,
            });
        }

        // SAFETY:
        // - frame.data was initialized via copy_from in apply()
        // - INIT flag is set (checked above)
        // - T::SHAPE matches frame.shape (asserted above), so reading as T is valid
        let value = unsafe { frame.data.assume_init().as_const().read::<T>() };

        // Free the frame from arena and deallocate its memory
        let frame = self.arena.free(self.root);

        // Mark root as invalid so Drop doesn't try to free it again
        self.root = Idx::COMPLETE;

        if frame.flags.contains(FrameFlags::OWNS_ALLOC) {
            let layout = frame.shape.layout.sized_layout().unwrap();
            if layout.size() > 0 {
                // SAFETY:
                // - frame.data was allocated with this layout in alloc_shape
                // - we own the allocation (OWNS_ALLOC flag)
                // - we've read the value out, so we're not dropping it, just deallocating
                unsafe {
                    dealloc(frame.data.as_mut_byte_ptr(), layout);
                }
            }
        }

        Ok(value)
    }
}

impl<'facet> Drop for Partial<'facet> {
    fn drop(&mut self) {
        // If root is valid, we need to clean up
        if self.root.is_valid() {
            let frame = self.arena.free(self.root);

            // Drop the value in place if initialized
            if frame.flags.contains(FrameFlags::INIT) {
                // SAFETY: INIT flag means the value is initialized
                unsafe {
                    frame.shape.call_drop_in_place(frame.data.assume_init());
                }
            }

            // Deallocate if we own the allocation
            if frame.flags.contains(FrameFlags::OWNS_ALLOC) {
                let layout = frame.shape.layout.sized_layout().unwrap();
                if layout.size() > 0 {
                    // SAFETY: we allocated this memory in alloc_shape with this layout
                    unsafe {
                        dealloc(frame.data.as_mut_byte_ptr(), layout);
                    }
                }
            }
        }
    }
}
