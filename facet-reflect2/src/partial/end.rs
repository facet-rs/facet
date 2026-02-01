use super::Partial;
use crate::arena::Idx;
use crate::errors::{ReflectError, ReflectErrorKind};
use crate::frame::{FrameFlags, FrameKind, ParentLink};
use facet_core::Def;

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
                        // Root is now complete, mark current as invalid
                        self.current = Idx::COMPLETE;
                        return Ok(());
                    }
                }
            }
        };

        // Check if current frame is complete
        let frame = self.arena.get(self.current);
        let is_complete = if frame.flags.contains(FrameFlags::INIT) {
            true
        } else {
            frame.kind.is_complete()
        };

        if !is_complete {
            return Err(self.error(ReflectErrorKind::EndWithIncomplete));
        }

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
                // Get insert function from parent's SetFrame
                let parent = self.arena.get(parent_idx);
                let FrameKind::Set(ref set_frame) = parent.kind else {
                    unreachable!("SetElement parent must be a Set frame")
                };
                let insert_fn = set_frame.def.vtable.insert;

                // Get the element data pointer (our current frame's data, now initialized)
                let frame = self.arena.get(self.current);
                let element_ptr = unsafe { frame.data.assume_init() };

                // Get the set pointer from parent's data (it's initialized)
                let parent = self.arena.get(parent_idx);
                let set_ptr = unsafe { parent.data.assume_init() };

                // Insert the element into the set (moves the value)
                unsafe {
                    insert_fn(set_ptr, element_ptr);
                }

                // The value has been moved into the set. Now deallocate our temp memory.
                let frame = self.arena.get_mut(self.current);
                frame.flags.remove(FrameFlags::INIT);
                let freed_frame = self.arena.free(self.current);
                freed_frame.dealloc_if_owned();

                // Increment element count in parent set
                let parent = self.arena.get_mut(parent_idx);
                if let FrameKind::Set(ref mut s) = parent.kind {
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
}
