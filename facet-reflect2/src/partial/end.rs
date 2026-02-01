use super::Partial;
use crate::arena::Idx;
use crate::errors::{ReflectError, ReflectErrorKind};
use crate::frame::{FrameFlags, FrameKind, ParentLink};
use facet_core::{Def, DefaultSource, PtrUninit, SequenceType, Type, UserType};

/// Who initiated the End operation.
#[derive(Clone, Copy)]
pub(crate) enum EndInitiator {
    /// User called Op::End - not allowed to end root frame
    Op,
    /// build() is ending frames - allowed to end root frame
    Build,
}

impl<'facet> Partial<'facet> {
    /// Apply an End operation - pop back to parent frame.
    pub(crate) fn apply_end(&mut self, initiator: EndInitiator) -> Result<(), ReflectError> {
        // If current frame is a collection, ensure it's initialized before ending
        // (handles empty collections that never had Push/Insert called)
        self.ensure_collection_initialized()?;

        // Special handling for collections: commit staged elements before ending
        self.finalize_list_if_needed()?;
        self.finalize_set_if_needed()?;
        self.finalize_map_if_needed()?;

        // Check if we're at root
        let frame = self.arena.get(self.current);
        let parent_idx = match frame.parent_link.parent_idx() {
            Some(idx) => idx,
            None => {
                // At root - only Build is allowed to end root
                match initiator {
                    EndInitiator::Op => return Err(self.error(ReflectErrorKind::EndAtRoot)),
                    EndInitiator::Build => {
                        // For root frame, apply defaults for struct-like types
                        // build()'s validation will catch other issues
                        self.apply_defaults_for_struct_fields()?;
                        // Root is now complete, mark current as invalid
                        self.current = Idx::COMPLETE;
                        return Ok(());
                    }
                }
            }
        };

        // Apply defaults for incomplete fields, then verify completeness (non-root frames)
        self.apply_defaults_and_ensure_complete()?;

        // Now dispatch based on what kind of child this is
        // Re-get frame to check the parent_link variant
        let frame = self.arena.get(self.current);
        match &frame.parent_link {
            ParentLink::Root => {
                // Already handled above
                unreachable!()
            }

            ParentLink::StructField { field_idx, .. } => {
                let field_idx = *field_idx;
                // Normal struct field - just free frame and mark complete
                let _ = self.arena.free(self.current);

                let parent = self.arena.get_mut(parent_idx);
                match &mut parent.kind {
                    FrameKind::Struct(s) => {
                        s.mark_field_complete(field_idx as usize);
                    }
                    FrameKind::VariantData(v) => {
                        v.mark_field_complete(field_idx as usize);
                    }
                    _ => {
                        return Err(self.error_at(parent_idx, ReflectErrorKind::NotIndexedChildren));
                    }
                }

                self.current = parent_idx;
            }

            ParentLink::ListElement { .. } => {
                // Check if this is a direct-fill element (no OWNS_ALLOC = lives in Vec's buffer)
                let frame = self.arena.get(self.current);
                let is_direct_fill = !frame.flags.contains(FrameFlags::OWNS_ALLOC);

                if is_direct_fill {
                    // Direct-fill: element is already in Vec's buffer, just increment staged_len
                    let _ = self.arena.free(self.current);

                    let parent = self.arena.get_mut(parent_idx);
                    if let FrameKind::List(ref mut l) = parent.kind {
                        l.staged_len += 1;
                    }
                } else {
                    // Fallback: use push to add element
                    let parent = self.arena.get(parent_idx);
                    let FrameKind::List(ref list_frame) = parent.kind else {
                        unreachable!("ListElement parent must be a List frame")
                    };
                    let push_fn = list_frame.def.push().ok_or_else(|| {
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

                    // Get the list pointer from parent's data (it's initialized)
                    let parent = self.arena.get(parent_idx);
                    let list_ptr = unsafe { parent.data.assume_init() };

                    // Push the element to the list (moves the value)
                    unsafe {
                        push_fn(list_ptr, element_ptr.as_const());
                    }

                    // The value has been moved into the list. Now deallocate our temp memory.
                    let frame = self.arena.get_mut(self.current);
                    frame.flags.remove(FrameFlags::INIT);
                    let freed_frame = self.arena.free(self.current);
                    freed_frame.dealloc_if_owned();

                    // Increment element count in parent list
                    let parent = self.arena.get_mut(parent_idx);
                    if let FrameKind::List(ref mut l) = parent.kind {
                        l.len += 1;
                    }
                }

                self.current = parent_idx;
            }

            ParentLink::SetElement { .. } => {
                // Set element completing - element is in the slab.
                // Just increment count and free the frame.
                // The actual set construction happens when the Set frame ends
                // (via finalize_set_if_needed which calls from_slice).

                // Free the element frame - do NOT deallocate since slab owns the memory
                let _ = self.arena.free(self.current);

                // Increment element count in parent set
                let parent = self.arena.get_mut(parent_idx);
                if let FrameKind::Set(s) = &mut parent.kind {
                    s.len += 1;
                }

                self.current = parent_idx;
            }

            ParentLink::PointerInner { .. } => {
                // Get pointer vtable from parent's shape
                let parent = self.arena.get(parent_idx);
                let Def::Pointer(ptr_def) = parent.shape.def() else {
                    return Err(self.error_at(parent_idx, ReflectErrorKind::UnsupportedPointerType));
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

                // Call new_into_fn to create the pointer (Box/Rc/Arc) from the pointee
                let _result = unsafe { new_into_fn(ptr_dest, pointee_ptr) };

                // The value has been moved into the pointer. Deallocate our staging memory.
                let frame = self.arena.get_mut(self.current);
                frame.flags.remove(FrameFlags::INIT);
                let freed_frame = self.arena.free(self.current);
                freed_frame.dealloc_if_owned();

                // Mark parent as initialized and complete
                let parent = self.arena.get_mut(parent_idx);
                parent.flags |= FrameFlags::INIT;
                if let FrameKind::Pointer(ref mut p) = parent.kind {
                    p.inner = Idx::COMPLETE;
                }

                self.current = parent_idx;
            }

            ParentLink::OptionInner { .. } => {
                // Get Option def and init_some function from parent's shape
                let parent = self.arena.get(parent_idx);
                let Def::Option(option_def) = parent.shape.def() else {
                    return Err(self.error_at(parent_idx, ReflectErrorKind::NotAnOption));
                };
                let init_some_fn = option_def.vtable.init_some;

                // Get the inner value data pointer (our current frame's data, now initialized)
                let frame = self.arena.get(self.current);
                let inner_ptr = unsafe { frame.data.assume_init() };

                // Get the parent's data pointer (where the Option will be written)
                let parent = self.arena.get(parent_idx);
                let option_dest = parent.data;

                // Call init_some to create Some(inner_value)
                unsafe {
                    init_some_fn(option_dest, inner_ptr);
                }

                // The value has been moved into the Option. Deallocate our temp memory.
                let frame = self.arena.get_mut(self.current);
                frame.flags.remove(FrameFlags::INIT);
                let freed_frame = self.arena.free(self.current);
                freed_frame.dealloc_if_owned();

                // Mark parent as initialized and complete
                let parent = self.arena.get_mut(parent_idx);
                parent.flags |= FrameFlags::INIT;
                if let FrameKind::Option(ref mut o) = parent.kind {
                    o.inner = Idx::COMPLETE;
                }

                self.current = parent_idx;
            }

            ParentLink::ResultInner { is_ok, .. } => {
                let is_ok = *is_ok;

                // Get Result def from parent's shape
                let parent = self.arena.get(parent_idx);
                let Def::Result(result_def) = parent.shape.def() else {
                    return Err(self.error_at(parent_idx, ReflectErrorKind::NotAResult));
                };

                // Get the appropriate init function based on variant
                let init_fn = if is_ok {
                    result_def.vtable.init_ok
                } else {
                    result_def.vtable.init_err
                };

                // Get the inner value data pointer (our current frame's data, now initialized)
                let frame = self.arena.get(self.current);
                let inner_ptr = unsafe { frame.data.assume_init() };

                // Get the parent's data pointer (where the Result will be written)
                let parent = self.arena.get(parent_idx);
                let result_dest = parent.data;

                // Call init_ok/init_err to create Ok(value) or Err(value)
                unsafe {
                    init_fn(result_dest, inner_ptr);
                }

                // The value has been moved into the Result. Deallocate our temp memory.
                let frame = self.arena.get_mut(self.current);
                frame.flags.remove(FrameFlags::INIT);
                let freed_frame = self.arena.free(self.current);
                freed_frame.dealloc_if_owned();

                // Mark parent as initialized and complete
                let parent = self.arena.get_mut(parent_idx);
                parent.flags |= FrameFlags::INIT;
                if let FrameKind::Result(ref mut r) = parent.kind {
                    r.inner = Idx::COMPLETE;
                }

                self.current = parent_idx;
            }

            ParentLink::EnumVariant { variant_idx, .. } => {
                let variant_idx = *variant_idx;

                // Free the current frame (memory stays - it's part of parent's allocation)
                let _ = self.arena.free(self.current);

                // Mark the variant as complete in parent
                let parent = self.arena.get_mut(parent_idx);
                if let FrameKind::Enum(ref mut e) = parent.kind {
                    e.selected = Some((variant_idx, Idx::COMPLETE));
                }

                self.current = parent_idx;
            }

            ParentLink::MapEntry { .. } => {
                // This is a map entry frame completing.
                // The entry data is in the parent map's slab - we just need to
                // increment the count and free this frame.
                // The actual map construction happens when the Map frame ends
                // (via finalize_map_if_needed which calls from_pair_slice).

                // Free the entry frame - do NOT deallocate since slab owns the memory
                let _ = self.arena.free(self.current);

                // Increment entry count in parent map
                let parent = self.arena.get_mut(parent_idx);
                if let FrameKind::Map(ref mut m) = parent.kind {
                    m.len += 1;
                }

                self.current = parent_idx;
            }

            ParentLink::MapEntryField { field_idx, .. } => {
                let field_idx = *field_idx;

                // Free the current frame and mark the field complete in parent entry
                let _ = self.arena.free(self.current);

                let parent = self.arena.get_mut(parent_idx);
                if let FrameKind::MapEntry(ref mut e) = parent.kind {
                    e.mark_field_complete(field_idx as usize);
                }

                self.current = parent_idx;
            }
        }

        Ok(())
    }

    /// If current frame is a List with staged elements, commit them using set_len.
    pub(crate) fn finalize_list_if_needed(&mut self) -> Result<(), ReflectError> {
        let frame = self.arena.get(self.current);
        let FrameKind::List(ref list_frame) = frame.kind else {
            return Ok(()); // Not a list, nothing to do
        };

        // Check if there are staged elements to commit
        if list_frame.staged_len == 0 {
            return Ok(()); // Nothing staged
        }

        // Check if we have set_len (direct-fill support)
        let set_len_fn = match list_frame.def.set_len() {
            Some(f) => f,
            None => {
                // No set_len = not using direct-fill, staged_len shouldn't be > 0
                // This would be a bug if it happens
                debug_assert!(
                    false,
                    "staged_len > 0 but no set_len function - logic error"
                );
                return Ok(());
            }
        };

        // Get the data we need
        let list_ptr = unsafe { frame.data.assume_init() };
        let new_len = list_frame.len + list_frame.staged_len;

        // Commit the staged elements
        // SAFETY: list_ptr is initialized, new_len elements are initialized in buffer
        unsafe {
            set_len_fn(list_ptr, new_len);
        }

        // Update the frame
        let frame = self.arena.get_mut(self.current);
        if let FrameKind::List(ref mut l) = frame.kind {
            l.len = new_len;
            l.staged_len = 0;
        }

        Ok(())
    }

    /// If current frame is a Map with a slab, build the map using from_pair_slice.
    pub(crate) fn finalize_map_if_needed(&mut self) -> Result<(), ReflectError> {
        let frame = self.arena.get(self.current);
        let FrameKind::Map(ref map_frame) = frame.kind else {
            return Ok(()); // Not a map, nothing to do
        };

        // Check if there's a slab to finalize
        if map_frame.slab.is_none() {
            return Ok(()); // No slab = no entries were staged
        }

        // Get from_pair_slice function
        let from_pair_slice = map_frame.def.vtable.from_pair_slice.ok_or_else(|| {
            self.error(ReflectErrorKind::MapDoesNotSupportFromPairSlice { shape: frame.shape })
        })?;

        // Get the data we need before taking the slab
        let map_ptr = frame.data;
        let len = map_frame.len;

        // Take the slab out of the frame
        let frame = self.arena.get_mut(self.current);
        let FrameKind::Map(ref mut map_frame) = frame.kind else {
            unreachable!()
        };
        let slab = map_frame.slab.take().expect("slab exists");
        let pairs_ptr = slab.as_mut_ptr();

        // Build the map using from_pair_slice
        // SAFETY: map_ptr is uninitialized memory, pairs_ptr points to len initialized entries
        unsafe {
            from_pair_slice(map_ptr, pairs_ptr, len);
        }

        // Slab is dropped here - deallocates buffer but doesn't drop elements
        // (elements were moved out by from_pair_slice)
        drop(slab);

        // Mark map as initialized
        let frame = self.arena.get_mut(self.current);
        frame.flags |= FrameFlags::INIT;

        Ok(())
    }

    /// If current frame is a Set with a slab, build the set using from_slice.
    pub(crate) fn finalize_set_if_needed(&mut self) -> Result<(), ReflectError> {
        let frame = self.arena.get(self.current);
        let FrameKind::Set(ref set_frame) = frame.kind else {
            return Ok(()); // Not a set, nothing to do
        };

        // Check if there's a slab to finalize
        if set_frame.slab.is_none() {
            return Ok(()); // No slab = no elements were staged
        }

        // Get from_slice function
        let from_slice = set_frame.def.vtable.from_slice.ok_or_else(|| {
            self.error(ReflectErrorKind::SetDoesNotSupportFromSlice { shape: frame.shape })
        })?;

        // Get the data we need before taking the slab
        let set_ptr = frame.data;
        let len = set_frame.len;

        // Take the slab out of the frame
        let frame = self.arena.get_mut(self.current);
        let FrameKind::Set(ref mut set_frame) = frame.kind else {
            unreachable!()
        };
        let slab = set_frame.slab.take().expect("slab exists");
        let elements_ptr = slab.as_mut_ptr();

        // Build the set using from_slice
        // SAFETY: set_ptr is uninitialized memory, elements_ptr points to len initialized elements
        unsafe {
            from_slice(set_ptr, elements_ptr, len);
        }

        // Slab is dropped here - deallocates buffer but doesn't drop elements
        // (elements were moved out by from_slice)
        drop(slab);

        // Mark set as initialized
        let frame = self.arena.get_mut(self.current);
        frame.flags |= FrameFlags::INIT;

        Ok(())
    }

    /// Apply defaults only for struct-like root frames.
    ///
    /// For struct/variant fields that are NOT_STARTED, tries to apply defaults.
    /// Returns Ok(()) for non-struct types (scalars, options, etc.) - those are
    /// validated by build() which returns NotInitialized if incomplete.
    fn apply_defaults_for_struct_fields(&mut self) -> Result<(), ReflectError> {
        let frame = self.arena.get(self.current);

        // Only handle struct-like types here
        let (fields_slice, data_ptr) = match &frame.kind {
            FrameKind::Struct(s) => {
                if s.is_complete() {
                    return Ok(());
                }
                match frame.shape.ty() {
                    Type::User(UserType::Struct(struct_type)) => (struct_type.fields, frame.data),
                    _ => return Ok(()), // Not a struct type - let build() validate
                }
            }
            FrameKind::VariantData(v) => {
                if v.is_complete() {
                    return Ok(());
                }
                (v.variant.data.fields, frame.data)
            }
            // Non-struct types: nothing to do, let build() validate
            _ => return Ok(()),
        };

        // Apply defaults to incomplete fields
        for (i, field) in fields_slice.iter().enumerate() {
            let is_complete = {
                let frame = self.arena.get(self.current);
                match &frame.kind {
                    FrameKind::Struct(s) => s.fields.is_complete(i),
                    FrameKind::VariantData(v) => v.fields.is_complete(i),
                    _ => true,
                }
            };

            if is_complete {
                continue;
            }

            // Try to apply default
            let field_ptr = unsafe { PtrUninit::new(data_ptr.as_mut_byte_ptr().add(field.offset)) };

            let applied = if let Some(default_source) = field.default_source() {
                match default_source {
                    DefaultSource::FromTrait => {
                        unsafe { field.shape().call_default_in_place(field_ptr) }.is_some()
                    }
                    DefaultSource::Custom(f) => {
                        unsafe { f(field_ptr) };
                        true
                    }
                }
            } else if let Def::Option(opt_def) = field.shape().def {
                // Option<T> without explicit default - init to None
                unsafe { (opt_def.vtable.init_none)(field_ptr) };
                true
            } else {
                false
            };

            if applied {
                // Mark field as complete
                let frame = self.arena.get_mut(self.current);
                match &mut frame.kind {
                    FrameKind::Struct(s) => s.mark_field_complete(i),
                    FrameKind::VariantData(v) => v.mark_field_complete(i),
                    _ => {}
                }
            }
            // Note: if we couldn't apply a default, we don't error here.
            // build() will catch it when it checks if the frame is complete.
        }

        Ok(())
    }

    /// Apply defaults for incomplete struct/variant fields, then verify completeness.
    ///
    /// For fields that are NOT_STARTED:
    /// - If field has `#[facet(default)]` or `#[facet(default = expr)]`, apply that default
    /// - If field is `Option<T>`, initialize to None
    /// - Otherwise, return MissingRequiredField error
    fn apply_defaults_and_ensure_complete(&mut self) -> Result<(), ReflectError> {
        let frame = self.arena.get(self.current);

        // Already fully initialized - nothing to do
        if frame.flags.contains(FrameFlags::INIT) {
            return Ok(());
        }

        // Extract what we need without holding borrows
        enum FieldSource {
            Struct(&'static [facet_core::Field]),
            Variant(&'static [facet_core::Field]),
            AlreadyComplete,
            Incomplete,
        }

        let (field_source, data_ptr) = match &frame.kind {
            FrameKind::Struct(s) => match frame.shape.ty() {
                Type::User(UserType::Struct(struct_type)) => {
                    if s.is_complete() {
                        (FieldSource::AlreadyComplete, frame.data)
                    } else {
                        (FieldSource::Struct(struct_type.fields), frame.data)
                    }
                }
                Type::Sequence(SequenceType::Array(_)) => {
                    if s.is_complete() {
                        (FieldSource::AlreadyComplete, frame.data)
                    } else {
                        (FieldSource::Incomplete, frame.data)
                    }
                }
                _ => {
                    if s.is_complete() {
                        (FieldSource::AlreadyComplete, frame.data)
                    } else {
                        (FieldSource::Incomplete, frame.data)
                    }
                }
            },
            FrameKind::VariantData(v) => {
                if v.is_complete() {
                    (FieldSource::AlreadyComplete, frame.data)
                } else {
                    (FieldSource::Variant(v.variant.data.fields), frame.data)
                }
            }
            _ => {
                // Non-struct frames: use INIT flag or kind.is_complete()
                if frame.flags.contains(FrameFlags::INIT) || frame.kind.is_complete() {
                    (FieldSource::AlreadyComplete, frame.data)
                } else {
                    (FieldSource::Incomplete, frame.data)
                }
            }
        };

        // Handle simple cases
        let fields_slice = match field_source {
            FieldSource::AlreadyComplete => return Ok(()),
            FieldSource::Incomplete => {
                return Err(self.error(ReflectErrorKind::EndWithIncomplete));
            }
            FieldSource::Struct(fields) | FieldSource::Variant(fields) => fields,
        };

        // Process each field
        for (i, field) in fields_slice.iter().enumerate() {
            // Check if already complete (re-borrow each iteration)
            let is_complete = {
                let frame = self.arena.get(self.current);
                match &frame.kind {
                    FrameKind::Struct(s) => s.fields.is_complete(i),
                    FrameKind::VariantData(v) => v.fields.is_complete(i),
                    _ => true,
                }
            };

            if is_complete {
                continue;
            }

            // Try to apply default
            let field_ptr = unsafe { PtrUninit::new(data_ptr.as_mut_byte_ptr().add(field.offset)) };

            if let Some(default_source) = field.default_source() {
                match default_source {
                    DefaultSource::FromTrait => {
                        let ok = unsafe { field.shape().call_default_in_place(field_ptr) };
                        if ok.is_none() {
                            return Err(
                                self.error(ReflectErrorKind::MissingRequiredField { index: i })
                            );
                        }
                    }
                    DefaultSource::Custom(f) => {
                        unsafe { f(field_ptr) };
                    }
                }
            } else if let Def::Option(opt_def) = field.shape().def {
                // Option<T> without explicit default - init to None
                unsafe { (opt_def.vtable.init_none)(field_ptr) };
            } else {
                // No default available - this field is required
                return Err(self.error(ReflectErrorKind::MissingRequiredField { index: i }));
            }

            // Mark field as complete
            let frame = self.arena.get_mut(self.current);
            match &mut frame.kind {
                FrameKind::Struct(s) => s.mark_field_complete(i),
                FrameKind::VariantData(v) => v.mark_field_complete(i),
                _ => {}
            }
        }

        Ok(())
    }
}
