use super::Partial;
use crate::arena::Idx;
use crate::errors::{ReflectError, ReflectErrorKind};
use crate::frame::{FrameFlags, FrameKind, ParentLink};
use facet_core::{Def, PtrMut};

impl<'facet> Partial<'facet> {
    /// Apply an End operation - pop back to parent frame.
    pub(crate) fn apply_end(&mut self) -> Result<(), ReflectError> {
        // If current frame is a collection, ensure it's initialized before ending
        // (handles empty collections that never had Push/Insert called)
        self.ensure_collection_initialized()?;

        // First check if we're at root - can't end at root regardless of completeness
        let frame = self.arena.get(self.current);
        let parent_idx = match frame.parent_link.parent_idx() {
            Some(idx) => idx,
            None => return Err(self.error(ReflectErrorKind::EndAtRoot)),
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
                // Get push function from parent's ListFrame
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
                // The parent is a Map frame. We need to insert the (key, value) pair.

                // Get the MapEntryFrame to access the map_def
                let frame = self.arena.get(self.current);
                let FrameKind::MapEntry(ref entry_frame) = frame.kind else {
                    unreachable!("MapEntry parent_link must have MapEntry frame kind")
                };
                let insert_fn = entry_frame.map_def.vtable.insert;
                let key_shape = entry_frame.map_def.k;
                let value_shape = entry_frame.map_def.v;

                // Get the key and value pointers from the entry's staging buffer
                // The entry frame's data points to contiguous memory for key + value
                let key_layout = key_shape.layout.sized_layout().unwrap();
                let value_layout = value_shape.layout.sized_layout().unwrap();

                // Calculate offsets - key is at offset 0, value at aligned offset after key
                let key_ptr = frame.data;
                let value_offset = key_layout.size().max(value_layout.align());
                let value_ptr =
                    unsafe { PtrMut::new(frame.data.as_mut_byte_ptr().add(value_offset)) };

                // Get the map pointer from parent's data (it's initialized)
                let parent = self.arena.get(parent_idx);
                let map_ptr = unsafe { parent.data.assume_init() };

                // Insert the key-value pair (moves both out)
                unsafe {
                    insert_fn(map_ptr, PtrMut::new(key_ptr.as_mut_byte_ptr()), value_ptr);
                }

                // Free the entry frame and deallocate its staging memory
                let frame = self.arena.get_mut(self.current);
                frame.flags.remove(FrameFlags::INIT);
                let freed_frame = self.arena.free(self.current);
                freed_frame.dealloc_if_owned();

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
}
