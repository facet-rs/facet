//! Partial value construction.

// Ops appliers
mod end;
mod insert;
mod push;
mod set;

use std::alloc::alloc;
use std::marker::PhantomData;
use std::ptr::NonNull;

use crate::arena::{Arena, Idx};
use crate::errors::{ReflectError, ReflectErrorKind};
use crate::frame::{Frame, FrameFlags, FrameKind, absolute_path};
use crate::ops::{Op, Path};
use facet_core::{
    Def, EnumType, Facet, Field, PtrUninit, SequenceType, Shape, Type, UserType, Variant,
};

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
        // Check Def first because Option/Result have Def::Option/Result
        // but are also UserType::Enum at the ty level
        let mut frame = match &shape.def {
            Def::Option(_) => Frame::new_option(data, shape),
            Def::Result(_) => Frame::new_result(data, shape),
            Def::List(_) => {
                // Lists start uninitialized - Build will initialize them
                Frame::new(data, shape)
            }
            _ => match shape.ty {
                Type::User(UserType::Struct(ref s)) => {
                    Frame::new_struct(data, shape, s.fields.len())
                }
                Type::User(UserType::Enum(_)) => Frame::new_enum(data, shape),
                Type::Sequence(SequenceType::Array(ref a)) => {
                    // Arrays are like structs with N indexed elements
                    Frame::new_struct(data, shape, a.n)
                }
                _ => Frame::new(data, shape),
            },
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

        // Walk from current frame up to root, cleaning up each frame.
        // This handles in-progress child frames (e.g., list elements being built).
        let mut idx = self.current;
        while idx.is_valid() {
            let frame = self.arena.get_mut(idx);
            let parent = frame.parent_link.parent_idx();

            // Drop any initialized data in this frame
            frame.uninit();

            // Free the frame and deallocate if it owns its allocation
            // Note: If this is a MapValue frame, the key TempAlloc in ParentLink
            // will be dropped when the frame is freed (ParentLink is moved out).
            let frame = self.arena.free(idx);
            frame.dealloc_if_owned();

            // Move to parent
            idx = parent.unwrap_or(Idx::COMPLETE);
        }

        // Mark as cleaned up
        self.current = Idx::COMPLETE;
        self.root = Idx::COMPLETE;
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

    /// Apply a batch of operations with proper ownership tracking.
    ///
    /// Unlike `apply()`, this method updates the batch's consumption tracking
    /// so that unconsumed `Imm` values are properly dropped when the batch is dropped.
    ///
    /// The caller must `mem::forget` all source values after adding them to the batch.
    /// The batch takes ownership and handles cleanup of both consumed and unconsumed values.
    pub fn apply_batch(&mut self, batch: &crate::ops::OpBatch<'_>) -> Result<(), ReflectError> {
        if self.poisoned {
            return Err(ReflectError::at_root(
                self.root_shape,
                ReflectErrorKind::Poisoned,
            ));
        }

        let result = self.apply_batch_inner(batch);
        if result.is_err() {
            self.poison();
        }
        result
    }

    fn apply_batch_inner(&mut self, batch: &crate::ops::OpBatch<'_>) -> Result<(), ReflectError> {
        let ops = batch.ops();
        for (i, op) in ops.iter().enumerate() {
            // Mark this op as consumed BEFORE processing it.
            // If processing fails after copying Imm bytes, those bytes are now
            // owned by the partial (or a TempAlloc that will clean them up).
            // The batch should NOT drop them again.
            batch.mark_consumed_up_to(i + 1);

            match op {
                Op::Set {
                    dst: path,
                    src: source,
                } => {
                    let frame = self.arena.get(self.current);
                    let is_enum_variant_selection = !path.is_empty()
                        && matches!(frame.kind, FrameKind::Enum(_))
                        && matches!(frame.shape.ty, Type::User(UserType::Enum(_)));

                    if is_enum_variant_selection {
                        self.apply_enum_variant_set(path, source)?;
                    } else {
                        self.apply_regular_set(path, source)?;
                    }
                }
                Op::Push { src } => {
                    self.apply_push(src)?;
                }
                Op::Insert { key, value } => {
                    self.apply_insert(key, value)?;
                }
                Op::End => {
                    self.apply_end()?;
                }
            }
        }
        Ok(())
    }

    fn apply_inner(&mut self, ops: &[Op<'_>]) -> Result<(), ReflectError> {
        for op in ops {
            match op {
                Op::Set {
                    dst: path,
                    src: source,
                } => {
                    // Check if current frame is an enum frame (not inside a variant's fields)
                    // and path is non-empty - that means we're selecting a variant
                    let frame = self.arena.get(self.current);
                    let is_enum_variant_selection = !path.is_empty()
                        && matches!(frame.kind, FrameKind::Enum(_))
                        && matches!(frame.shape.ty, Type::User(UserType::Enum(_)));

                    // Check if current frame is an Option/Result frame with variant selection
                    let is_option_variant_selection = !path.is_empty()
                        && matches!(frame.kind, FrameKind::Option(_))
                        && matches!(frame.shape.def, Def::Option(_));

                    let is_result_variant_selection = !path.is_empty()
                        && matches!(frame.kind, FrameKind::Result(_))
                        && matches!(frame.shape.def, Def::Result(_));

                    if is_enum_variant_selection {
                        self.apply_enum_variant_set(path, source)?;
                    } else if is_option_variant_selection {
                        self.apply_option_variant_set(path, source)?;
                    } else if is_result_variant_selection {
                        self.apply_result_variant_set(path, source)?;
                    } else {
                        self.apply_regular_set(path, source)?;
                    }
                }
                Op::Push { src } => {
                    self.apply_push(src)?;
                }
                Op::Insert { key, value } => {
                    self.apply_insert(key, value)?;
                }
                Op::End => {
                    self.apply_end()?;
                }
            }
        }
        Ok(())
    }

    /// Ensure the current collection (list, map, or set) is initialized.
    /// This is called lazily on first Push/Insert.
    fn ensure_collection_initialized(&mut self) -> Result<(), ReflectError> {
        let frame = self.arena.get(self.current);

        let needs_init = match &frame.kind {
            FrameKind::List(l) => !l.initialized,
            FrameKind::Map(m) => !m.initialized,
            FrameKind::Set(s) => !s.initialized,
            _ => return Ok(()), // Not a collection, nothing to do
        };

        if !needs_init {
            return Ok(());
        }

        // Initialize based on frame kind (which has the def)
        let frame = self.arena.get(self.current);
        match &frame.kind {
            FrameKind::List(list_frame) => {
                let init_fn = list_frame
                    .def
                    .init_in_place_with_capacity()
                    .ok_or_else(|| {
                        self.error(ReflectErrorKind::ListDoesNotSupportOp { shape: frame.shape })
                    })?;
                // SAFETY: frame.data points to uninitialized list memory
                unsafe { init_fn(frame.data, 0) };
            }
            FrameKind::Map(map_frame) => {
                let init_fn = map_frame.def.vtable.init_in_place_with_capacity;
                // SAFETY: frame.data points to uninitialized map memory
                unsafe { init_fn(frame.data, 0) };
            }
            FrameKind::Set(set_frame) => {
                let init_fn = set_frame.def.vtable.init_in_place_with_capacity;
                // SAFETY: frame.data points to uninitialized set memory
                unsafe { init_fn(frame.data, 0) };
            }
            _ => unreachable!(),
        }

        // Mark as initialized
        let frame = self.arena.get_mut(self.current);
        match &mut frame.kind {
            FrameKind::List(l) => l.initialized = true,
            FrameKind::Map(m) => m.initialized = true,
            FrameKind::Set(s) => s.initialized = true,
            _ => unreachable!(),
        }
        frame.flags |= FrameFlags::INIT;

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

        // For now, only support single-level paths
        let indices = path.as_slice();
        if indices.len() != 1 {
            return Err(self.error(ReflectErrorKind::MultiLevelPathNotSupported {
                depth: indices.len(),
            }));
        }

        let index = indices[0];

        // Check if we're inside a variant - use variant's fields for resolution
        if let FrameKind::VariantData(v) = &frame.kind {
            let field = self.get_struct_field(v.variant.data.fields, index)?;
            let field_ptr =
                unsafe { PtrUninit::new(frame.data.as_mut_byte_ptr().add(field.offset)) };
            return Ok(Frame::new(field_ptr, field.shape()));
        }

        match frame.shape.ty {
            Type::User(UserType::Struct(ref s)) => {
                let field = self.get_struct_field(s.fields, index)?;
                let field_ptr =
                    unsafe { PtrUninit::new(frame.data.as_mut_byte_ptr().add(field.offset)) };
                Ok(Frame::new(field_ptr, field.shape()))
            }
            Type::User(UserType::Enum(ref e)) => {
                // Validate the variant index
                let _variant = self.get_enum_variant(e, index)?;
                // For enums, we return the shape of the whole enum (not the variant)
                // The variant's fields will be accessed in a nested frame after Build
                Ok(Frame::new(frame.data, frame.shape))
            }
            Type::Sequence(SequenceType::Array(ref a)) => {
                // Validate array index
                if index as usize >= a.n {
                    return Err(self.error(ReflectErrorKind::ArrayIndexOutOfBounds {
                        index,
                        array_len: a.n,
                    }));
                }
                // Calculate element offset: index * element_size
                // Note: Layout::size() includes trailing padding, so it equals the stride
                let element_shape = a.t;
                let element_layout = element_shape.layout.sized_layout().map_err(|_| {
                    self.error(ReflectErrorKind::Unsized {
                        shape: element_shape,
                    })
                })?;
                let offset = (index as usize) * element_layout.size();
                let element_ptr =
                    unsafe { PtrUninit::new(frame.data.as_mut_byte_ptr().add(offset)) };
                Ok(Frame::new(element_ptr, element_shape))
            }
            _ => {
                // Check for Option/Result types (they have special Def but not a special Type)
                match &frame.shape.def {
                    Def::Option(_) => {
                        // Validate variant index: 0 = None, 1 = Some
                        if index > 1 {
                            return Err(
                                self.error(ReflectErrorKind::OptionVariantOutOfBounds { index })
                            );
                        }
                        // Return shape of the whole Option (like enums)
                        Ok(Frame::new(frame.data, frame.shape))
                    }
                    Def::Result(_) => {
                        // Validate variant index: 0 = Ok, 1 = Err
                        if index > 1 {
                            return Err(
                                self.error(ReflectErrorKind::ResultVariantOutOfBounds { index })
                            );
                        }
                        // Return shape of the whole Result (like enums)
                        Ok(Frame::new(frame.data, frame.shape))
                    }
                    _ => Err(self.error(ReflectErrorKind::NotAStruct)),
                }
            }
        }
    }

    /// Get a struct field by index.
    fn get_struct_field(
        &self,
        fields: &'static [Field],
        index: u32,
    ) -> Result<&'static Field, ReflectError> {
        let idx = index as usize;
        if idx >= fields.len() {
            return Err(self.error(ReflectErrorKind::FieldIndexOutOfBounds {
                index,
                field_count: fields.len(),
            }));
        }
        Ok(&fields[idx])
    }

    /// Get an enum variant by index.
    fn get_enum_variant(
        &self,
        enum_type: &EnumType,
        index: u32,
    ) -> Result<&'static Variant, ReflectError> {
        let idx = index as usize;
        if idx >= enum_type.variants.len() {
            return Err(self.error(ReflectErrorKind::VariantIndexOutOfBounds {
                index,
                variant_count: enum_type.variants.len(),
            }));
        }
        Ok(&enum_type.variants[idx])
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
            frame.kind.is_complete()
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

        // Mark as invalid so Drop doesn't try to free again
        self.root = Idx::COMPLETE;
        self.current = Idx::COMPLETE;

        frame.dealloc_if_owned();

        Ok(value)
    }
}

impl<'facet> Drop for Partial<'facet> {
    fn drop(&mut self) {
        // Walk from current frame up to root, cleaning up each frame.
        // This handles in-progress child frames (e.g., list elements being built).
        let mut idx = self.current;
        while idx.is_valid() {
            let frame = self.arena.get_mut(idx);
            let parent = frame.parent_link.parent_idx();

            // Drop any initialized data in this frame
            frame.uninit();

            // Free the frame and deallocate if it owns its allocation
            // Note: If this is a MapValue frame, the key TempAlloc in ParentLink
            // will be dropped when the frame is freed (ParentLink is moved out).
            let frame = self.arena.free(idx);
            frame.dealloc_if_owned();

            // Move to parent
            idx = parent.unwrap_or(Idx::COMPLETE);
        }
    }
}
