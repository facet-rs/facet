//! Partial value construction.

use std::alloc::alloc;
use std::marker::PhantomData;
use std::ptr::NonNull;

use crate::arena::{Arena, Idx};
use crate::errors::{ReflectError, ReflectErrorKind};
use crate::frame::{Children, Frame, FrameFlags, absolute_path};
use crate::ops::{Op, Path, Source};
use facet_core::{Facet, Field, PtrUninit, Shape, Type, UserType};

/// Manages incremental construction of a value.
pub struct Partial<'facet> {
    arena: Arena<Frame>,
    root: Idx<Frame>,
    current: Idx<Frame>,
    root_shape: &'static Shape,
    poisoned: bool,
    _marker: PhantomData<&'facet ()>,
}

impl<'facet> Partial<'facet> {
    /// Create an error at the current frame location.
    fn error(&self, kind: ReflectErrorKind) -> ReflectError {
        let frame = self.arena.get(self.current);
        ReflectError::new(frame.shape, absolute_path(&self.arena, self.current), kind)
    }

    /// Create an error at a specific frame location.
    fn error_at(&self, idx: Idx<Frame>, kind: ReflectErrorKind) -> ReflectError {
        let frame = self.arena.get(idx);
        ReflectError::new(frame.shape, absolute_path(&self.arena, idx), kind)
    }

    /// Allocate for a known type.
    pub fn alloc<T: Facet<'facet>>() -> Result<Self, ReflectError> {
        Self::alloc_shape(T::SHAPE)
    }

    /// Allocate for a dynamic shape.
    pub fn alloc_shape(shape: &'static Shape) -> Result<Self, ReflectError> {
        let layout = shape
            .layout
            .sized_layout()
            .map_err(|_| ReflectError::at_root(shape, ReflectErrorKind::Unsized { shape }))?;

        // Allocate memory (handle ZST case)
        let data = if layout.size() == 0 {
            PtrUninit::new(NonNull::<u8>::dangling().as_ptr())
        } else {
            // SAFETY: layout has non-zero size (checked above) and is valid from Shape
            let ptr = unsafe { alloc(layout) };
            if ptr.is_null() {
                return Err(ReflectError::at_root(
                    shape,
                    ReflectErrorKind::AllocFailed { layout },
                ));
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
            root_shape: shape,
            poisoned: false,
            _marker: PhantomData,
        })
    }

    /// Poison the partial and clean up all resources.
    /// After this, any operation will return `Poisoned` error.
    fn poison(&mut self) {
        if self.poisoned {
            return;
        }
        self.poisoned = true;

        // Clean up if root is valid
        if self.root.is_valid() {
            // Drop any initialized value
            self.arena.get_mut(self.root).uninit();

            // Free the frame and deallocate if we own the allocation
            let frame = self.arena.free(self.root);
            frame.dealloc_if_owned();

            // Mark root as invalid
            self.root = Idx::COMPLETE;
        }
    }

    /// Apply a sequence of operations.
    pub fn apply(&mut self, ops: &[Op<'_>]) -> Result<(), ReflectError> {
        if self.poisoned {
            return Err(ReflectError::at_root(
                self.root_shape,
                ReflectErrorKind::Poisoned,
            ));
        }

        let result = self.apply_inner(ops);
        if result.is_err() {
            self.poison();
        }
        result
    }

    fn apply_inner(&mut self, ops: &[Op<'_>]) -> Result<(), ReflectError> {
        for op in ops {
            match op {
                Op::Set { path, source } => {
                    // Resolve path to a temporary frame for the target
                    let frame = self.arena.get(self.current);
                    let target = self.resolve_path(frame, path)?;

                    match source {
                        Source::Move(mov) => {
                            // Verify shape matches
                            target.assert_shape(mov.shape(), path)?;

                            // Drop any existing value before overwriting
                            if path.is_empty() {
                                let frame = self.arena.get_mut(self.current);
                                frame.uninit();
                            }

                            // Re-resolve path after potential mutation
                            let frame = self.arena.get(self.current);
                            let mut target = self.resolve_path(frame, path)?;

                            // Copy the value into the target frame
                            // SAFETY: Move's safety invariant guarantees ptr is valid for shape
                            unsafe {
                                target
                                    .copy_from(mov.ptr(), mov.shape())
                                    .map_err(|kind| self.error(kind))?;
                            }

                            // Now get mutable borrow to update state
                            let frame = self.arena.get_mut(self.current);

                            // Mark as initialized
                            if path.is_empty() {
                                frame.flags |= FrameFlags::INIT;
                            } else {
                                // Mark child as complete
                                let field_idx = path.as_slice()[0] as usize;
                                let Children::Indexed(c) = &mut frame.children else {
                                    return Err(self.error(ReflectErrorKind::NotIndexedChildren));
                                };
                                c.mark_complete(field_idx);
                            }
                        }
                        Source::Build(_) => todo!("Build source"),
                        Source::Default => {
                            // Drop any existing value before overwriting
                            if path.is_empty() {
                                let frame = self.arena.get_mut(self.current);
                                frame.uninit();
                            }

                            // Re-resolve path after potential mutation
                            let frame = self.arena.get(self.current);
                            let target = self.resolve_path(frame, path)?;

                            // Call default_in_place on the target
                            // SAFETY: target.data points to uninitialized memory of the correct type
                            let ok = unsafe { target.shape.call_default_in_place(target.data) };
                            if ok.is_none() {
                                return Err(self.error(ReflectErrorKind::NoDefault {
                                    shape: target.shape,
                                }));
                            }

                            // Now get mutable borrow to update state
                            let frame = self.arena.get_mut(self.current);

                            // Mark as initialized
                            if path.is_empty() {
                                frame.flags |= FrameFlags::INIT;
                            } else {
                                // Mark child as complete
                                let field_idx = path.as_slice()[0] as usize;
                                let Children::Indexed(c) = &mut frame.children else {
                                    return Err(self.error(ReflectErrorKind::NotIndexedChildren));
                                };
                                c.mark_complete(field_idx);
                            }
                        }
                    }
                }
            }
        }
        Ok(())
    }

    /// Resolve a path to a temporary frame for the target location.
    ///
    /// For an empty path, returns a frame pointing to the current frame's data.
    /// For a non-empty path, returns a frame pointing to the field's memory.
    fn resolve_path(&self, frame: &Frame, path: &Path) -> Result<Frame, ReflectError> {
        if path.is_empty() {
            return Ok(Frame::new(frame.data, frame.shape));
        }

        // For now, only support single-level paths into structs
        let indices = path.as_slice();
        if indices.len() != 1 {
            return Err(self.error(ReflectErrorKind::MultiLevelPathNotSupported {
                depth: indices.len(),
            }));
        }

        let field_idx = indices[0];
        let field = self.get_struct_field(frame.shape, field_idx)?;

        // Compute field pointer: base + offset
        let field_ptr = unsafe { PtrUninit::new(frame.data.as_mut_byte_ptr().add(field.offset)) };

        Ok(Frame::new(field_ptr, field.shape()))
    }

    /// Get a struct field by index.
    fn get_struct_field(
        &self,
        shape: &'static Shape,
        index: u32,
    ) -> Result<&'static Field, ReflectError> {
        // Get struct type from shape
        let fields = match shape.ty {
            Type::User(UserType::Struct(ref s)) => s.fields,
            _ => {
                return Err(self.error(ReflectErrorKind::NotAStruct));
            }
        };

        // Bounds check
        let idx = index as usize;
        if idx >= fields.len() {
            return Err(self.error(ReflectErrorKind::FieldIndexOutOfBounds {
                index,
                field_count: fields.len(),
            }));
        }

        Ok(&fields[idx])
    }

    /// Build the final value, consuming the Partial.
    pub fn build<T: Facet<'facet>>(mut self) -> Result<T, ReflectError> {
        let frame = self.arena.get(self.root);

        // Verify shape matches
        if !frame.shape.is_shape(T::SHAPE) {
            return Err(self.error_at(
                self.root,
                ReflectErrorKind::ShapeMismatch {
                    expected: frame.shape,
                    actual: T::SHAPE,
                },
            ));
        }

        // Verify initialized - check based on type
        let is_initialized = if frame.flags.contains(FrameFlags::INIT) {
            // Whole value was set (e.g., scalar or Move of entire struct)
            true
        } else {
            // For compound types, check all children are complete
            match &frame.children {
                Children::Indexed(c) => c.all_complete(),
                Children::None => false,
            }
        };

        if !is_initialized {
            return Err(self.error_at(self.root, ReflectErrorKind::NotInitialized));
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

        frame.dealloc_if_owned();

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
            frame.dealloc_if_owned();
        }
    }
}
