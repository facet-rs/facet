//! Partial value construction.

use std::alloc::{alloc, dealloc};
use std::marker::PhantomData;
use std::ptr::NonNull;

use crate::arena::{Arena, Idx};
use crate::errors::{ErrorLocation, ReflectError, ReflectErrorKind};
use crate::frame::{Frame, FrameFlags};
use crate::ops::{Op, Path, Source};
use facet_core::{Facet, Field, PtrUninit, Shape, Type, UserType};

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
        // Use appropriate constructor based on type
        let mut frame = match shape.ty {
            Type::User(UserType::Struct(ref s)) => Frame::new_struct(data, shape, s.fields.len()),
            _ => Frame::new(data, shape),
        };
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
    pub fn apply(&mut self, ops: &[Op<'_>]) -> Result<(), ReflectError> {
        for op in ops {
            match op {
                Op::Set { path, source } => {
                    // Resolve path first (immutable borrow)
                    let frame = self.arena.get(self.current);
                    let (target_ptr, target_shape, frame_shape) = self.resolve_path(frame, path)?;

                    match source {
                        Source::Move(mov) => {
                            // Verify shape matches
                            if !target_shape.is_shape(mov.shape()) {
                                return Err(ReflectError {
                                    location: ErrorLocation {
                                        shape: frame_shape,
                                        path: path.clone(),
                                    },
                                    kind: ReflectErrorKind::ShapeMismatch {
                                        expected: target_shape,
                                        actual: mov.shape(),
                                    },
                                });
                            }

                            // Copy the value
                            // SAFETY: Move's safety invariant guarantees ptr is valid for shape
                            unsafe {
                                target_ptr.copy_from(mov.ptr(), target_shape).unwrap();
                            }

                            // Now get mutable borrow to update state
                            let frame = self.arena.get_mut(self.current);

                            // Mark as initialized
                            if path.is_empty() {
                                frame.flags |= FrameFlags::INIT;
                            } else {
                                // Mark child as complete
                                let field_idx = path.as_slice()[0] as usize;
                                frame.mark_child_complete(field_idx);
                            }
                        }
                        Source::Build(_) => todo!("Build source"),
                        Source::Default => todo!("Default source"),
                    }
                }
            }
        }
        Ok(())
    }

    /// Resolve a path to a target pointer and shape.
    ///
    /// For an empty path, returns the frame's data pointer and shape.
    /// For a non-empty path, navigates through struct fields.
    ///
    /// Returns (target_ptr, target_shape, frame_shape) - frame_shape is needed for error reporting.
    fn resolve_path(
        &self,
        frame: &Frame,
        path: &Path,
    ) -> Result<(PtrUninit, &'static Shape, &'static Shape), ReflectError> {
        if path.is_empty() {
            return Ok((frame.data, frame.shape, frame.shape));
        }

        // For now, only support single-level paths into structs
        let indices = path.as_slice();
        assert!(
            indices.len() == 1,
            "multi-level paths not yet supported (got {} levels)",
            indices.len()
        );

        let field_idx = indices[0];
        let field = self.get_struct_field(frame.shape, field_idx, path)?;

        // Compute field pointer: base + offset
        let field_ptr = unsafe { PtrUninit::new(frame.data.as_mut_byte_ptr().add(field.offset)) };

        Ok((field_ptr, field.shape(), frame.shape))
    }

    /// Get a struct field by index.
    fn get_struct_field(
        &self,
        shape: &'static Shape,
        index: u32,
        path: &Path,
    ) -> Result<&'static Field, ReflectError> {
        // Get struct type from shape
        let fields = match shape.ty {
            Type::User(UserType::Struct(ref s)) => s.fields,
            _ => {
                return Err(ReflectError {
                    location: ErrorLocation {
                        shape,
                        path: path.clone(),
                    },
                    kind: ReflectErrorKind::NotAStruct,
                });
            }
        };

        // Bounds check
        let idx = index as usize;
        if idx >= fields.len() {
            return Err(ReflectError {
                location: ErrorLocation {
                    shape,
                    path: path.clone(),
                },
                kind: ReflectErrorKind::FieldIndexOutOfBounds {
                    index,
                    field_count: fields.len(),
                },
            });
        }

        Ok(&fields[idx])
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

        // Verify initialized - check based on type
        let is_initialized = if frame.flags.contains(FrameFlags::INIT) {
            // Whole value was set (e.g., scalar or Move of entire struct)
            true
        } else {
            // For compound types, check all children are complete
            frame.all_children_complete()
        };

        if !is_initialized {
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
            // Drop any initialized value
            self.arena.get_mut(self.root).uninit();

            // Free the frame and deallocate if we own the allocation
            let frame = self.arena.free(self.root);
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
