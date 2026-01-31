use super::Partial;
use crate::arena::Idx;
use crate::errors::{ReflectError, ReflectErrorKind};
use crate::frame::{FrameFlags, FrameKind};
use facet_core::{Def, PtrMut};

impl<'facet> Partial<'facet> {
    /// Apply an End operation - pop back to parent frame.
    pub(crate) fn apply_end(&mut self) -> Result<(), ReflectError> {
        // If current frame is a collection, ensure it's initialized before ending
        // (handles empty collections that never had Push/Insert called)
        self.ensure_collection_initialized()?;

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

        // Check if parent is a pointer, list, map, set, option, or result frame - special finalization needed
        let parent = self.arena.get(parent_idx);
        let is_pointer_parent = matches!(parent.kind, FrameKind::Pointer(_));
        let is_list_parent = matches!(parent.kind, FrameKind::List(_));
        let is_map_parent = matches!(parent.kind, FrameKind::Map(_));
        let is_set_parent = matches!(parent.kind, FrameKind::Set(_));
        let is_option_parent = matches!(parent.kind, FrameKind::Option(_));
        let is_result_parent = matches!(parent.kind, FrameKind::Result(_));

        // Check if current frame has a pending key (value frame for map insert)
        let frame = self.arena.get(self.current);
        let has_pending_key = frame.pending_key.is_some();

        if has_pending_key && is_map_parent {
            // We're completing a value frame that was started by Insert with Build
            // Get map def and insert function from parent's shape
            let parent = self.arena.get(parent_idx);
            // Get insert function from parent's MapFrame
            let FrameKind::Map(ref map_frame) = parent.kind else {
                unreachable!()
            };
            let insert_fn = map_frame.def.vtable.insert;

            // Get the value data pointer (our current frame's data, now initialized)
            let frame = self.arena.get(self.current);
            let value_ptr = unsafe { frame.data.assume_init() };

            // Get the pending key
            let pending_key = frame.pending_key.as_ref().unwrap();
            let key_ptr = pending_key.ptr();

            // Get the map pointer from parent's data (it's initialized)
            let parent = self.arena.get(parent_idx);
            // SAFETY: parent map is initialized (we pushed elements into it)
            let map_ptr = unsafe { parent.data.assume_init() };

            // Insert the key-value pair (moves both out)
            // SAFETY: both pointers point to initialized data
            unsafe {
                insert_fn(
                    map_ptr,
                    PtrMut::new(key_ptr.as_mut_byte_ptr()),
                    PtrMut::new(value_ptr.as_mut_byte_ptr()),
                );
            }

            // The value has been moved into the map. Now deallocate temp memory.
            let frame = self.arena.get_mut(self.current);
            // Don't drop the value - it was moved out by insert_fn
            frame.flags.remove(FrameFlags::INIT);
            // Clear the pending key and mark it moved (TempAlloc will dealloc but not drop)
            let mut pending_key = frame.pending_key.take().unwrap();
            pending_key.mark_moved();
            // pending_key drops here, deallocating storage

            // Deallocate our staging memory for the value
            let freed_frame = self.arena.free(self.current);
            freed_frame.dealloc_if_owned();

            // Increment entry count in parent map
            let parent = self.arena.get_mut(parent_idx);
            if let FrameKind::Map(ref mut m) = parent.kind {
                m.len += 1;
            }

            // Pop back to parent
            self.current = parent_idx;
        } else if is_list_parent {
            // Get push function from parent's ListFrame
            let parent = self.arena.get(parent_idx);
            let FrameKind::List(ref list_frame) = parent.kind else {
                unreachable!()
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
            // SAFETY: parent list is initialized (we pushed elements into it)
            let list_ptr = unsafe { parent.data.assume_init() };

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
        } else if is_set_parent {
            // Get insert function from parent's SetFrame
            let parent = self.arena.get(parent_idx);
            let FrameKind::Set(ref set_frame) = parent.kind else {
                unreachable!()
            };
            let insert_fn = set_frame.def.vtable.insert;

            // Get the element data pointer (our current frame's data, now initialized)
            let frame = self.arena.get(self.current);
            let element_ptr = unsafe { frame.data.assume_init() };

            // Get the set pointer from parent's data (it's initialized)
            let parent = self.arena.get(parent_idx);
            // SAFETY: parent set is initialized (we pushed elements into it)
            let set_ptr = unsafe { parent.data.assume_init() };

            // Insert the element into the set (moves the value)
            // SAFETY: element_ptr points to initialized data of the correct element type
            unsafe {
                insert_fn(set_ptr, element_ptr);
            }

            // The value has been moved into the set. Now deallocate our temp memory.
            let frame = self.arena.get_mut(self.current);
            // Don't drop the value - it was moved out by insert_fn
            frame.flags.remove(FrameFlags::INIT);
            // Deallocate our staging memory
            let freed_frame = self.arena.free(self.current);
            freed_frame.dealloc_if_owned();

            // Increment element count in parent set
            let parent = self.arena.get_mut(parent_idx);
            if let FrameKind::Set(ref mut s) = parent.kind {
                s.len += 1;
            }

            // Pop back to parent
            self.current = parent_idx;
        } else if is_pointer_parent {
            // Get pointer vtable from parent's shape
            let parent = self.arena.get(parent_idx);
            let Def::Pointer(ptr_def) = &parent.shape.def else {
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
        } else if is_option_parent {
            // Get Option def and init_some function from parent's shape
            let parent = self.arena.get(parent_idx);
            let Def::Option(option_def) = &parent.shape.def else {
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
            // SAFETY: inner_ptr points to initialized data, option_dest points to Option memory
            unsafe {
                init_some_fn(option_dest, inner_ptr);
            }

            // The value has been moved into the Option. Now deallocate our temp memory.
            let frame = self.arena.get_mut(self.current);
            // Don't drop the value - it was moved out by init_some_fn
            frame.flags.remove(FrameFlags::INIT);
            // Deallocate our staging memory
            let freed_frame = self.arena.free(self.current);
            freed_frame.dealloc_if_owned();

            // Mark parent as initialized and complete
            let parent = self.arena.get_mut(parent_idx);
            parent.flags |= FrameFlags::INIT;
            if let FrameKind::Option(ref mut o) = parent.kind {
                o.inner = Idx::COMPLETE;
            }

            // Pop back to parent
            self.current = parent_idx;
        } else if is_result_parent {
            // Get Result def from parent's shape
            let parent = self.arena.get(parent_idx);
            let Def::Result(result_def) = &parent.shape.def else {
                return Err(self.error_at(parent_idx, ReflectErrorKind::NotAResult));
            };

            // Get the selected variant (Ok=0, Err=1) from parent's ResultFrame
            let FrameKind::Result(ref result_frame) = parent.kind else {
                return Err(self.error_at(parent_idx, ReflectErrorKind::NotAResult));
            };
            let variant_idx = result_frame
                .selected
                .ok_or_else(|| self.error_at(parent_idx, ReflectErrorKind::NotAResult))?;

            // Get the appropriate init function based on variant
            let init_fn = if variant_idx == 0 {
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
            // SAFETY: inner_ptr points to initialized data, result_dest points to Result memory
            unsafe {
                init_fn(result_dest, inner_ptr);
            }

            // The value has been moved into the Result. Now deallocate our temp memory.
            let frame = self.arena.get_mut(self.current);
            // Don't drop the value - it was moved out by init_fn
            frame.flags.remove(FrameFlags::INIT);
            // Deallocate our staging memory
            let freed_frame = self.arena.free(self.current);
            freed_frame.dealloc_if_owned();

            // Mark parent as initialized and complete
            let parent = self.arena.get_mut(parent_idx);
            parent.flags |= FrameFlags::INIT;
            if let FrameKind::Result(ref mut r) = parent.kind {
                r.inner = Idx::COMPLETE;
            }

            // Pop back to parent
            self.current = parent_idx;
        } else {
            // Normal (non-special) End handling
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
                    return Err(self.error_at(parent_idx, ReflectErrorKind::NotIndexedChildren));
                }
                FrameKind::Map(_) => {
                    // Map entries should have pending_key set and use the map insert path
                    // This shouldn't happen with current implementation
                    return Err(self.error_at(parent_idx, ReflectErrorKind::NotIndexedChildren));
                }
                FrameKind::Set(_) => {
                    // Set elements are inserted directly via the set insert path
                    // This shouldn't happen with current implementation
                    return Err(self.error_at(parent_idx, ReflectErrorKind::NotIndexedChildren));
                }
                FrameKind::Option(_) | FrameKind::Result(_) => {
                    // Option/Result should have been handled above
                    return Err(self.error_at(parent_idx, ReflectErrorKind::NotIndexedChildren));
                }
                FrameKind::Scalar => {
                    return Err(self.error_at(parent_idx, ReflectErrorKind::NotIndexedChildren));
                }
            }

            // Pop back to parent
            self.current = parent_idx;
        }

        Ok(())
    }
}
