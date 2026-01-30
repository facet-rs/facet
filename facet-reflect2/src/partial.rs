//! Partial value construction.

use std::alloc::alloc;
use std::marker::PhantomData;
use std::ptr::NonNull;

use crate::arena::{Arena, Idx};
use crate::enum_helpers::{
    drop_variant_fields, read_discriminant, variant_index_from_discriminant, write_discriminant,
};
use crate::errors::{ReflectError, ReflectErrorKind};
use crate::frame::{Frame, FrameFlags, FrameKind, absolute_path};
use crate::ops::{Op, Path, Source};
use facet_core::{EnumType, Facet, Field, PtrUninit, Shape, Type, UserType, Variant};

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
            Type::User(UserType::Enum(_)) => Frame::new_enum(data, shape),
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
                    // Check if current frame is an enum frame (not inside a variant's fields)
                    // and path is non-empty - that means we're selecting a variant
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
                Op::End => {
                    // Pop back to parent frame
                    let frame = self.arena.get(self.current);
                    let Some((parent_idx, field_idx)) = frame.parent else {
                        return Err(self.error(ReflectErrorKind::EndAtRoot));
                    };

                    // Check if current frame is complete
                    let is_complete = if frame.flags.contains(FrameFlags::INIT) {
                        true
                    } else {
                        frame.kind.is_complete()
                    };

                    if !is_complete {
                        return Err(self.error(ReflectErrorKind::EndWithIncomplete));
                    }

                    // Free the current frame (memory stays - it's part of parent's allocation)
                    let _ = self.arena.free(self.current);

                    // Mark field/variant complete in parent
                    let parent = self.arena.get_mut(parent_idx);
                    match &mut parent.kind {
                        FrameKind::Struct(s) => {
                            s.mark_field_complete(field_idx as usize);
                        }
                        FrameKind::VariantData(v) => {
                            v.mark_field_complete(field_idx as usize);
                        }
                        FrameKind::Enum(e) => {
                            // Mark the variant as complete
                            if let Some((variant_idx, _)) = e.selected {
                                e.selected = Some((variant_idx, Idx::COMPLETE));
                            }
                        }
                        FrameKind::Scalar => {
                            return Err(
                                self.error_at(parent_idx, ReflectErrorKind::NotIndexedChildren)
                            );
                        }
                    }

                    // Pop back to parent
                    self.current = parent_idx;
                }
            }
        }
        Ok(())
    }

    /// Apply a Set operation for regular (non-enum-variant) targets.
    fn apply_regular_set(&mut self, path: &Path, source: &Source<'_>) -> Result<(), ReflectError> {
        // Resolve path to a temporary frame for the target
        let frame = self.arena.get(self.current);
        let target = self.resolve_path(frame, path)?;

        match source {
            Source::Imm(mov) => {
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

                    // For enums, read the discriminant and update selected variant
                    if let Type::User(UserType::Enum(ref enum_type)) = frame.shape.ty {
                        if let FrameKind::Enum(ref mut e) = frame.kind {
                            // SAFETY: we just copied a valid enum value, so discriminant is valid
                            let discriminant = unsafe {
                                read_discriminant(frame.data.assume_init().as_const(), enum_type)
                            };
                            // Handle error after releasing mutable borrow
                            let discriminant = match discriminant {
                                Ok(d) => d,
                                Err(kind) => {
                                    return Err(ReflectError::new(
                                        frame.shape,
                                        absolute_path(&self.arena, self.current),
                                        kind,
                                    ));
                                }
                            };

                            if let Some(variant_idx) =
                                variant_index_from_discriminant(enum_type, discriminant)
                            {
                                // Mark the variant as complete (the whole value was moved in)
                                e.selected = Some((variant_idx, Idx::COMPLETE));
                            }
                        }
                    }
                } else {
                    // Mark child as complete
                    let field_idx = path.as_slice()[0] as usize;
                    frame.kind.mark_field_complete(field_idx);
                }
            }
            Source::Build(_build) => {
                // Build pushes a new frame for incremental construction
                // Path must be non-empty (can't "build" at current position)
                if path.is_empty() {
                    return Err(self.error(ReflectErrorKind::BuildAtEmptyPath));
                }

                // Resolve path to get target shape and pointer
                let frame = self.arena.get(self.current);
                let target = self.resolve_path(frame, path)?;

                // Create a new frame for the nested value
                let field_idx = path.as_slice()[0];
                let mut new_frame = match target.shape.ty {
                    Type::User(UserType::Struct(ref s)) => {
                        Frame::new_struct(target.data, target.shape, s.fields.len())
                    }
                    Type::User(UserType::Enum(_)) => Frame::new_enum(target.data, target.shape),
                    _ => Frame::new(target.data, target.shape),
                };
                new_frame.parent = Some((self.current, field_idx));

                // Store in arena and make it current
                let new_idx = self.arena.alloc(new_frame);
                self.current = new_idx;
            }
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
                    frame.kind.mark_field_complete(field_idx);
                }
            }
        }
        Ok(())
    }

    /// Apply a Set operation for enum variant selection.
    fn apply_enum_variant_set(
        &mut self,
        path: &Path,
        source: &Source<'_>,
    ) -> Result<(), ReflectError> {
        let indices = path.as_slice();
        if indices.len() != 1 {
            return Err(self.error(ReflectErrorKind::MultiLevelPathNotSupported {
                depth: indices.len(),
            }));
        }
        let variant_idx = indices[0];

        // Get enum type and variant
        let frame = self.arena.get(self.current);
        let Type::User(UserType::Enum(ref enum_type)) = frame.shape.ty else {
            return Err(self.error(ReflectErrorKind::NotAnEnum));
        };
        let new_variant = self.get_enum_variant(enum_type, variant_idx)?;

        // Drop any existing value before switching variants.
        // If INIT is set, the whole enum was initialized (e.g., via Move at []),
        // so we use uninit() which calls drop_in_place on the whole shape.
        // If INIT is not set but selected has a complete variant, we drop just that
        // variant's fields (the variant was set via apply_enum_variant_set).
        let frame = self.arena.get_mut(self.current);
        if frame.flags.contains(FrameFlags::INIT) {
            frame.uninit();
        } else if let FrameKind::Enum(e) = &frame.kind {
            if let Some((old_variant_idx, status)) = e.selected {
                if status.is_complete() {
                    let old_variant = &enum_type.variants[old_variant_idx as usize];
                    // SAFETY: the variant was marked complete, so its fields are initialized
                    unsafe {
                        drop_variant_fields(frame.data.assume_init().as_const(), old_variant);
                    }
                }
                // TODO: handle partially initialized variants (status is a valid frame idx)
            }
        }

        // Re-get frame after potential drop/uninit
        let frame = self.arena.get(self.current);

        // Write the discriminant
        // SAFETY: frame.data points to valid enum memory
        unsafe {
            write_discriminant(frame.data, enum_type, new_variant)
                .map_err(|kind| self.error(kind))?;
        }

        match source {
            Source::Default => {
                // For unit variants, just writing the discriminant is enough
                // For struct variants with Default, we'd need to default-initialize fields
                // For now, only support unit variants with Default
                if !new_variant.data.fields.is_empty() {
                    return Err(self.error(ReflectErrorKind::NoDefault { shape: frame.shape }));
                }

                // Mark variant as complete
                let frame = self.arena.get_mut(self.current);
                let Some(e) = frame.kind.as_enum_mut() else {
                    return Err(self.error(ReflectErrorKind::NotAnEnum));
                };
                e.selected = Some((variant_idx, Idx::COMPLETE));
            }
            Source::Imm(mov) => {
                // For tuple variants with a single field, copy the field value
                // The Move shape should match the tuple field's shape
                if new_variant.data.fields.len() != 1 {
                    return Err(self.error(ReflectErrorKind::ShapeMismatch {
                        expected: frame.shape,
                        actual: mov.shape(),
                    }));
                }

                let field = &new_variant.data.fields[0];
                if !field.shape().is_shape(mov.shape()) {
                    return Err(self.error(ReflectErrorKind::ShapeMismatch {
                        expected: field.shape(),
                        actual: mov.shape(),
                    }));
                }

                // Copy the value into the field
                let field_ptr =
                    unsafe { PtrUninit::new(frame.data.as_mut_byte_ptr().add(field.offset)) };
                unsafe {
                    field_ptr.copy_from(mov.ptr(), mov.shape()).unwrap();
                }

                // Mark variant as complete
                let frame = self.arena.get_mut(self.current);
                let Some(e) = frame.kind.as_enum_mut() else {
                    return Err(self.error(ReflectErrorKind::NotAnEnum));
                };
                e.selected = Some((variant_idx, Idx::COMPLETE));
            }
            Source::Build(_build) => {
                // Push a frame for the variant's fields
                let frame = self.arena.get(self.current);
                let mut new_frame = Frame::new_variant(frame.data, frame.shape, new_variant);
                new_frame.parent = Some((self.current, variant_idx));

                // Store in arena and make it current
                let new_idx = self.arena.alloc(new_frame);

                // Record the frame in enum's selected variant
                let frame = self.arena.get_mut(self.current);
                let Some(e) = frame.kind.as_enum_mut() else {
                    return Err(self.error(ReflectErrorKind::NotAnEnum));
                };
                e.selected = Some((variant_idx, new_idx));

                self.current = new_idx;
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
            _ => Err(self.error(ReflectErrorKind::NotAStruct)),
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
