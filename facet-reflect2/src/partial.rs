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
use facet_core::{Def, EnumType, Facet, Field, PtrUninit, Shape, Type, UserType, Variant};

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
            _ => {
                // Check for list type
                if let Def::List(_) = &shape.def {
                    // Lists start uninitialized - Build will initialize them
                    Frame::new(data, shape)
                } else {
                    Frame::new(data, shape)
                }
            }
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

                    if is_enum_variant_selection {
                        self.apply_enum_variant_set(path, source)?;
                    } else {
                        self.apply_regular_set(path, source)?;
                    }
                }
                Op::Push { src } => {
                    self.apply_push(src)?;
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

                    // Check if parent is a pointer or list frame - special finalization needed
                    let parent = self.arena.get(parent_idx);
                    let is_pointer_parent = matches!(parent.kind, FrameKind::Pointer(_));
                    let is_list_parent = matches!(parent.kind, FrameKind::List(_));

                    if is_list_parent {
                        // Get list def and push function from parent's shape
                        let parent = self.arena.get(parent_idx);
                        let Def::List(list_def) = &parent.shape.def else {
                            return Err(self.error_at(parent_idx, ReflectErrorKind::NotAList));
                        };
                        let push_fn = list_def.push().ok_or_else(|| {
                            self.error_at(
                                parent_idx,
                                ReflectErrorKind::ListDoesNotSupportOp {
                                    shape: parent.shape,
                                },
                            )
                        })?;

                        // Get the element data pointer (our current frame's data, now initialized)
                        let frame = self.arena.get(self.current);
                        let element_ptr = unsafe { frame.data.assume_init() };

                        // Get the list pointer from parent's ListFrame
                        let parent = self.arena.get(parent_idx);
                        let FrameKind::List(ref list_frame) = parent.kind else {
                            unreachable!()
                        };
                        let list_ptr = list_frame.list_ptr;

                        // Push the element to the list (moves the value)
                        // SAFETY: element_ptr points to initialized data of the correct element type
                        unsafe {
                            push_fn(list_ptr, element_ptr.as_const());
                        }

                        // The value has been moved into the list. Now deallocate our temp memory.
                        let frame = self.arena.get_mut(self.current);
                        // Don't drop the value - it was moved out by push_fn
                        frame.flags.remove(FrameFlags::INIT);
                        // Deallocate our staging memory
                        let freed_frame = self.arena.free(self.current);
                        freed_frame.dealloc_if_owned();

                        // Increment element count in parent list
                        let parent = self.arena.get_mut(parent_idx);
                        if let FrameKind::List(ref mut l) = parent.kind {
                            l.len += 1;
                        }

                        // Pop back to parent
                        self.current = parent_idx;
                    } else if is_pointer_parent {
                        // Get pointer vtable from parent's shape
                        let parent = self.arena.get(parent_idx);
                        let Def::Pointer(ptr_def) = &parent.shape.def else {
                            return Err(
                                self.error_at(parent_idx, ReflectErrorKind::UnsupportedPointerType)
                            );
                        };
                        let new_into_fn = ptr_def.vtable.new_into_fn.ok_or_else(|| {
                            self.error_at(parent_idx, ReflectErrorKind::UnsupportedPointerType)
                        })?;

                        // Get the pointee data pointer (our current frame's data, now initialized)
                        let frame = self.arena.get(self.current);
                        let pointee_ptr = unsafe { frame.data.assume_init() };

                        // Get the parent's data pointer (where the pointer will be written)
                        let parent = self.arena.get(parent_idx);
                        let ptr_dest = parent.data;

                        // Call new_into_fn to create the pointer (Box/Rc/Arc) from the pointee.
                        // This reads the value from pointee_ptr and creates a proper pointer.
                        // SAFETY: pointee_ptr points to initialized data of the correct type.
                        let _result = unsafe { new_into_fn(ptr_dest, pointee_ptr) };

                        // The value has been moved into the pointer. Now we need to deallocate
                        // our temporary staging memory (the pointer now owns its own allocation).
                        let frame = self.arena.get_mut(self.current);
                        // Don't drop the value - it was moved out by new_into_fn
                        frame.flags.remove(FrameFlags::INIT);
                        // But DO deallocate our staging memory
                        let freed_frame = self.arena.free(self.current);
                        freed_frame.dealloc_if_owned();

                        // Mark parent as initialized and complete
                        let parent = self.arena.get_mut(parent_idx);
                        parent.flags |= FrameFlags::INIT;
                        if let FrameKind::Pointer(ref mut p) = parent.kind {
                            p.inner = Idx::COMPLETE;
                        }

                        // Pop back to parent
                        self.current = parent_idx;
                    } else {
                        // Normal (non-pointer) End handling
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
                            FrameKind::Pointer(p) => {
                                p.inner = Idx::COMPLETE;
                            }
                            FrameKind::List(_) => {
                                // List elements don't have indexed tracking - the list itself tracks length
                                // This shouldn't happen with current implementation (list elements are pushed directly)
                                return Err(
                                    self.error_at(parent_idx, ReflectErrorKind::NotIndexedChildren)
                                );
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
        }
        Ok(())
    }

    /// Apply a Set operation for regular (non-enum-variant) targets.
    fn apply_regular_set(&mut self, path: &Path, source: &Source<'_>) -> Result<(), ReflectError> {
        // Disallow Set at [] when inside a variant frame - must End first
        let frame = self.arena.get(self.current);
        if path.is_empty() && matches!(frame.kind, FrameKind::VariantData(_)) {
            return Err(self.error(ReflectErrorKind::SetAtRootOfVariant));
        }

        // Resolve path to a temporary frame for the target
        let target = self.resolve_path(frame, path)?;

        match source {
            Source::Imm(mov) => {
                // Verify shape matches
                target.assert_shape(mov.shape(), path)?;

                // Drop any existing value before overwriting
                if path.is_empty() {
                    let frame = self.arena.get_mut(self.current);
                    frame.uninit();
                } else {
                    // Setting a field - need to handle already-INIT structs/tuples
                    let frame = self.arena.get_mut(self.current);
                    let field_idx = path.as_slice()[0] as usize;

                    if frame.flags.contains(FrameFlags::INIT) {
                        // The whole struct was previously initialized via Imm.
                        // We need to:
                        // 1. Drop the old field value
                        // 2. Clear INIT flag
                        // 3. Mark all OTHER fields as complete (they're still valid)

                        // Get the struct type to access field info
                        if let Type::User(UserType::Struct(ref struct_type)) = frame.shape.ty {
                            // Drop the old field value
                            let field = &struct_type.fields[field_idx];
                            // SAFETY: INIT means field is initialized
                            unsafe {
                                let field_ptr = frame.data.assume_init().field(field.offset);
                                field.shape().call_drop_in_place(field_ptr);
                            }

                            // Clear INIT and switch to field tracking
                            frame.flags.remove(FrameFlags::INIT);

                            // Mark all OTHER fields as complete
                            if let FrameKind::Struct(ref mut s) = frame.kind {
                                for i in 0..struct_type.fields.len() {
                                    if i != field_idx {
                                        s.mark_field_complete(i);
                                    }
                                }
                            }
                        }
                    } else if frame.kind.is_field_complete(field_idx) {
                        // Field was previously set individually - drop the old value
                        if let Type::User(UserType::Struct(ref struct_type)) = frame.shape.ty {
                            let field = &struct_type.fields[field_idx];
                            // SAFETY: field is marked complete, so it's initialized
                            unsafe {
                                let field_ptr = frame.data.assume_init().field(field.offset);
                                field.shape().call_drop_in_place(field_ptr);
                            }
                        }
                    }
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
                    if let Type::User(UserType::Enum(ref enum_type)) = frame.shape.ty
                        && let FrameKind::Enum(ref mut e) = frame.kind
                    {
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
                } else {
                    // Mark child as complete
                    let field_idx = path.as_slice()[0] as usize;
                    frame.kind.mark_field_complete(field_idx);
                }
            }
            Source::Build(build) => {
                // Build pushes a new frame for incremental construction
                let frame = self.arena.get(self.current);

                // Check for special types at empty path
                if path.is_empty() {
                    // Handle list types (Vec, etc.)
                    if let Def::List(list_def) = &frame.shape.def {
                        // Get the init function
                        let init_fn = list_def.init_in_place_with_capacity().ok_or_else(|| {
                            self.error(ReflectErrorKind::ListDoesNotSupportOp {
                                shape: frame.shape,
                            })
                        })?;

                        // Initialize the list with capacity hint
                        let capacity = build.len_hint.unwrap_or(0);
                        // SAFETY: frame.data points to uninitialized memory of the correct layout
                        let list_ptr = unsafe { init_fn(frame.data, capacity) };

                        // Convert to list frame
                        let frame = self.arena.get_mut(self.current);
                        frame.kind = FrameKind::List(crate::frame::ListFrame::new(list_ptr));
                        // The list is now initialized (empty, but valid)
                        frame.flags |= FrameFlags::INIT;

                        return Ok(());
                    }

                    // Handle pointer types (Box/Rc/Arc)
                    if let Def::Pointer(ptr_def) = &frame.shape.def {
                        // Get pointee shape
                        let pointee_shape = ptr_def
                            .pointee
                            .ok_or_else(|| self.error(ReflectErrorKind::UnsupportedPointerType))?;

                        // Allocate memory for the pointee
                        let pointee_layout = pointee_shape.layout.sized_layout().map_err(|_| {
                            self.error(ReflectErrorKind::Unsized {
                                shape: pointee_shape,
                            })
                        })?;

                        let pointee_data = if pointee_layout.size() == 0 {
                            PtrUninit::new(NonNull::<u8>::dangling().as_ptr())
                        } else {
                            // SAFETY: layout has non-zero size and is valid
                            let ptr = unsafe { alloc(pointee_layout) };
                            if ptr.is_null() {
                                return Err(self.error(ReflectErrorKind::AllocFailed {
                                    layout: pointee_layout,
                                }));
                            }
                            PtrUninit::new(ptr)
                        };

                        // Create the appropriate frame type for the pointee
                        // If the pointee is a struct, use struct tracking; if enum, use enum tracking
                        let mut new_frame = match pointee_shape.ty {
                            Type::User(UserType::Struct(ref s)) => {
                                Frame::new_struct(pointee_data, pointee_shape, s.fields.len())
                            }
                            Type::User(UserType::Enum(_)) => {
                                Frame::new_enum(pointee_data, pointee_shape)
                            }
                            _ => Frame::new_pointer(pointee_data, pointee_shape),
                        };
                        // For pointer frames, parent is current and index is 0 (the pointee)
                        new_frame.parent = Some((self.current, 0));
                        // Mark that this frame owns its allocation (for cleanup on error)
                        new_frame.flags |= FrameFlags::OWNS_ALLOC;

                        // Record the frame in parent's pointer state
                        let new_idx = self.arena.alloc(new_frame);

                        // Update parent to track this as a pointer frame
                        let frame = self.arena.get_mut(self.current);
                        frame.kind =
                            FrameKind::Pointer(crate::frame::PointerFrame { inner: new_idx });

                        self.current = new_idx;
                        return Ok(());
                    } else {
                        return Err(self.error(ReflectErrorKind::BuildAtEmptyPath));
                    }
                }

                // Resolve path to get target shape and pointer
                let frame = self.arena.get(self.current);
                let target = self.resolve_path(frame, path)?;

                // Create a new frame for the nested value
                let field_idx = path.as_slice()[0];

                // Check if target is a list - needs special initialization
                if let Def::List(list_def) = &target.shape.def {
                    // Get the init function
                    let init_fn = list_def.init_in_place_with_capacity().ok_or_else(|| {
                        self.error(ReflectErrorKind::ListDoesNotSupportOp {
                            shape: target.shape,
                        })
                    })?;

                    // Initialize the list with capacity hint
                    let capacity = build.len_hint.unwrap_or(0);
                    // SAFETY: target.data points to uninitialized memory of the correct layout
                    let list_ptr = unsafe { init_fn(target.data, capacity) };

                    // Create list frame
                    let mut new_frame = Frame::new_list(target.data, target.shape, list_ptr);
                    new_frame.parent = Some((self.current, field_idx));
                    new_frame.flags |= FrameFlags::INIT; // List is initialized (empty but valid)

                    let new_idx = self.arena.alloc(new_frame);
                    self.current = new_idx;
                } else {
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
            }
            Source::Default => {
                // Drop any existing value before overwriting
                if path.is_empty() {
                    let frame = self.arena.get_mut(self.current);
                    frame.uninit();
                } else {
                    // Setting a field - need to handle already-INIT structs/tuples
                    let frame = self.arena.get_mut(self.current);
                    let field_idx = path.as_slice()[0] as usize;

                    if frame.flags.contains(FrameFlags::INIT) {
                        // The whole struct was previously initialized via Imm.
                        // We need to:
                        // 1. Drop the old field value
                        // 2. Clear INIT flag
                        // 3. Mark all OTHER fields as complete (they're still valid)

                        // Get the struct type to access field info
                        if let Type::User(UserType::Struct(ref struct_type)) = frame.shape.ty {
                            // Drop the old field value
                            let field = &struct_type.fields[field_idx];
                            // SAFETY: INIT means field is initialized
                            unsafe {
                                let field_ptr = frame.data.assume_init().field(field.offset);
                                field.shape().call_drop_in_place(field_ptr);
                            }

                            // Clear INIT and switch to field tracking
                            frame.flags.remove(FrameFlags::INIT);

                            // Mark all OTHER fields as complete
                            if let FrameKind::Struct(ref mut s) = frame.kind {
                                for i in 0..struct_type.fields.len() {
                                    if i != field_idx {
                                        s.mark_field_complete(i);
                                    }
                                }
                            }
                        }
                    } else if frame.kind.is_field_complete(field_idx) {
                        // Field was previously set individually - drop the old value
                        if let Type::User(UserType::Struct(ref struct_type)) = frame.shape.ty {
                            let field = &struct_type.fields[field_idx];
                            // SAFETY: field is marked complete, so it's initialized
                            unsafe {
                                let field_ptr = frame.data.assume_init().field(field.offset);
                                field.shape().call_drop_in_place(field_ptr);
                            }
                        }
                    }
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
        } else if let FrameKind::Enum(e) = &mut frame.kind {
            if let Some((old_variant_idx, status)) = e.selected
                && status.is_complete()
            {
                let old_variant = &enum_type.variants[old_variant_idx as usize];
                // SAFETY: the variant was marked complete, so its fields are initialized
                unsafe {
                    drop_variant_fields(frame.data.assume_init().as_const(), old_variant);
                }
                // TODO: handle partially initialized variants (status is a valid frame idx)
            }
            // Clear selected so uninit() won't try to drop again if we error later
            e.selected = None;
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

    /// Apply a Push operation to add an element to the current list.
    fn apply_push(&mut self, source: &Source<'_>) -> Result<(), ReflectError> {
        // Verify we're in a list frame
        let frame = self.arena.get(self.current);
        let FrameKind::List(ref list_frame) = frame.kind else {
            return Err(self.error(ReflectErrorKind::NotAList));
        };

        // Get the list def and element shape
        let Def::List(list_def) = &frame.shape.def else {
            return Err(self.error(ReflectErrorKind::NotAList));
        };
        let element_shape = list_def.t;

        // Get push function
        let push_fn = list_def.push().ok_or_else(|| {
            self.error(ReflectErrorKind::ListDoesNotSupportOp { shape: frame.shape })
        })?;

        let list_ptr = list_frame.list_ptr;

        match source {
            Source::Imm(mov) => {
                // Verify element shape matches
                if !element_shape.is_shape(mov.shape()) {
                    return Err(self.error(ReflectErrorKind::ShapeMismatch {
                        expected: element_shape,
                        actual: mov.shape(),
                    }));
                }

                // Push the element - the push_fn moves the value out
                // SAFETY: mov.ptr() points to valid initialized data of the element type
                unsafe {
                    push_fn(list_ptr, mov.ptr());
                }

                // Increment element count
                let frame = self.arena.get_mut(self.current);
                if let FrameKind::List(ref mut l) = frame.kind {
                    l.len += 1;
                }
            }
            Source::Build(build) => {
                // Allocate temporary space for the element
                let layout = element_shape.layout.sized_layout().map_err(|_| {
                    self.error(ReflectErrorKind::Unsized {
                        shape: element_shape,
                    })
                })?;

                let temp_ptr = if layout.size() == 0 {
                    PtrUninit::new(std::ptr::NonNull::<u8>::dangling().as_ptr())
                } else {
                    let ptr = unsafe { std::alloc::alloc(layout) };
                    if ptr.is_null() {
                        return Err(self.error(ReflectErrorKind::AllocFailed { layout }));
                    }
                    PtrUninit::new(ptr)
                };

                // Create appropriate frame based on element shape
                let mut element_frame = if let Def::List(inner_list_def) = &element_shape.def {
                    // Element is itself a list - initialize it
                    let init_fn =
                        inner_list_def
                            .init_in_place_with_capacity()
                            .ok_or_else(|| {
                                self.error(ReflectErrorKind::ListDoesNotSupportOp {
                                    shape: element_shape,
                                })
                            })?;
                    let capacity = build.len_hint.unwrap_or(0);
                    // SAFETY: temp_ptr points to uninitialized memory of the correct layout
                    let inner_list_ptr = unsafe { init_fn(temp_ptr, capacity) };
                    let mut frame = Frame::new_list(temp_ptr, element_shape, inner_list_ptr);
                    frame.flags |= FrameFlags::INIT; // List is initialized (empty but valid)
                    frame
                } else {
                    match element_shape.ty {
                        Type::User(UserType::Struct(ref s)) => {
                            Frame::new_struct(temp_ptr, element_shape, s.fields.len())
                        }
                        Type::User(UserType::Enum(_)) => Frame::new_enum(temp_ptr, element_shape),
                        _ => Frame::new(temp_ptr, element_shape),
                    }
                };

                // Mark that this frame owns its allocation (for cleanup on End)
                element_frame.flags |= FrameFlags::OWNS_ALLOC;

                // Set parent to current list frame (field_idx doesn't matter for lists)
                element_frame.parent = Some((self.current, 0));

                // Push frame and make it current
                let element_idx = self.arena.alloc(element_frame);
                self.current = element_idx;
            }
            Source::Default => {
                // Allocate temporary space for the default value
                let layout = element_shape.layout.sized_layout().map_err(|_| {
                    self.error(ReflectErrorKind::Unsized {
                        shape: element_shape,
                    })
                })?;

                let temp_ptr = if layout.size() == 0 {
                    PtrUninit::new(std::ptr::NonNull::<u8>::dangling().as_ptr())
                } else {
                    let ptr = unsafe { std::alloc::alloc(layout) };
                    if ptr.is_null() {
                        return Err(self.error(ReflectErrorKind::AllocFailed { layout }));
                    }
                    PtrUninit::new(ptr)
                };

                // Initialize with default
                let ok = unsafe { element_shape.call_default_in_place(temp_ptr) };
                if ok.is_none() {
                    // Deallocate on failure
                    if layout.size() > 0 {
                        unsafe { std::alloc::dealloc(temp_ptr.as_mut_byte_ptr(), layout) };
                    }
                    return Err(self.error(ReflectErrorKind::NoDefault {
                        shape: element_shape,
                    }));
                }

                // Push the element
                // SAFETY: temp_ptr now contains initialized data
                unsafe {
                    push_fn(list_ptr, temp_ptr.assume_init().as_const());
                }

                // Deallocate temp storage (value was moved out by push_fn)
                if layout.size() > 0 {
                    unsafe { std::alloc::dealloc(temp_ptr.as_mut_byte_ptr(), layout) };
                }

                // Increment element count
                let frame = self.arena.get_mut(self.current);
                if let FrameKind::List(ref mut l) = frame.kind {
                    l.len += 1;
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
