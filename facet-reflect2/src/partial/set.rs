use std::alloc::alloc;
use std::ptr::NonNull;

use super::Partial;
use crate::arena::Idx;
use crate::enum_helpers::{read_discriminant, variant_index_from_discriminant};
use crate::errors::{ReflectError, ReflectErrorKind};
use crate::frame::{
    Frame, FrameFlags, FrameKind, ListFrame, MapFrame, ParentLink, PointerFrame, SetFrame,
    StructFrame, absolute_path,
};
use crate::ops::{Path, PathSegment, Source};
use crate::shape_desc::ShapeDesc;

/// Helper to extract the first field index from a path.
/// Returns the index if the path starts with a Field segment.
fn first_field_idx(path: &Path) -> Option<u32> {
    match path.segments().first() {
        Some(PathSegment::Field(n)) => Some(*n),
        _ => None,
    }
}

/// Determine the appropriate parent link for a field being set.
/// Returns MapEntryField if parent is a MapEntry frame, otherwise StructField.
fn make_field_parent_link(
    parent_kind: &FrameKind,
    parent_idx: Idx<Frame>,
    field_idx: u32,
) -> ParentLink {
    if matches!(parent_kind, FrameKind::MapEntry(_)) {
        ParentLink::MapEntryField {
            parent: parent_idx,
            field_idx,
        }
    } else {
        ParentLink::StructField {
            parent: parent_idx,
            field_idx,
        }
    }
}

use facet_core::{Def, PtrUninit, SequenceType, Type, UserType};

impl<'facet> Partial<'facet> {
    /// Apply a Set operation for regular (non-enum-variant) targets.
    pub(crate) fn apply_regular_set(
        &mut self,
        path: &Path,
        source: &Source<'_>,
    ) -> Result<(), ReflectError> {
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
                target.assert_shape(ShapeDesc::Static(mov.shape()), path)?;

                // Drop any existing value before overwriting
                if path.is_empty() {
                    let frame = self.arena.get_mut(self.current);
                    frame.uninit();
                } else {
                    // Setting a field - need to handle already-INIT structs/tuples
                    let frame = self.arena.get_mut(self.current);
                    let field_idx =
                        first_field_idx(path).expect("path must have field index") as usize;
                    frame.prepare_field_for_overwrite(field_idx);
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

                // Mark as initialized
                let is_empty_path = path.is_empty();
                {
                    let frame = self.arena.get_mut(self.current);

                    if is_empty_path {
                        frame.flags |= FrameFlags::INIT;

                        // For enums, read the discriminant and update selected variant
                        if let Type::User(UserType::Enum(ref enum_type)) = *frame.shape.ty()
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
                        let field_idx =
                            first_field_idx(path).expect("path must have field index") as usize;
                        frame.kind.mark_field_complete(field_idx);
                    }
                }

                // Sync collection frame states after Imm/Default at empty path
                // (called outside the scope to avoid borrow conflict)
                if is_empty_path {
                    self.list_sync_after_set();
                    self.map_sync_after_set();
                    self.set_sync_after_set();
                }
            }
            Source::Stage(_capacity) => {
                // Build pushes a new frame for incremental construction
                let frame = self.arena.get(self.current);
                // Copy shape to break borrow - ShapeDesc is Copy
                let shape = frame.shape;

                // Check for special types at empty path
                if path.is_empty() {
                    // Drop any existing value before re-staging
                    let frame = self.arena.get_mut(self.current);
                    frame.uninit();

                    // Handle list types (Vec, etc.)
                    // Just switch to list frame - initialization is deferred to first Push
                    if let Def::List(list_def) = *shape.def() {
                        let frame = self.arena.get_mut(self.current);
                        frame.kind = FrameKind::List(ListFrame::new(list_def));
                        return Ok(());
                    }

                    // Handle map types (HashMap, BTreeMap, etc.)
                    // Just switch to map frame - initialization is deferred to first Insert
                    if let Def::Map(map_def) = *shape.def() {
                        let frame = self.arena.get_mut(self.current);
                        frame.kind = FrameKind::Map(MapFrame::new(map_def));
                        return Ok(());
                    }

                    // Handle set types (HashSet, BTreeSet, etc.)
                    // Just switch to set frame - initialization is deferred to first Push
                    if let Def::Set(set_def) = *shape.def() {
                        let frame = self.arena.get_mut(self.current);
                        frame.kind = FrameKind::Set(SetFrame::new(set_def));
                        return Ok(());
                    }

                    // Handle pointer types (Box/Rc/Arc)
                    if let Def::Pointer(ptr_def) = *shape.def() {
                        // Get pointee shape
                        let pointee_shape = ptr_def
                            .pointee
                            .ok_or_else(|| self.error(ReflectErrorKind::UnsupportedPointerType))?;

                        // Allocate memory for the pointee
                        let pointee_layout = pointee_shape.layout.sized_layout().map_err(|_| {
                            self.error(ReflectErrorKind::Unsized {
                                shape: ShapeDesc::Static(pointee_shape),
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
                        // Check Def first because Option/Result have Def::Option/Result
                        // but are also UserType::Enum at the ty level
                        let mut new_frame = match &pointee_shape.def {
                            Def::Option(_) => Frame::new_option(pointee_data, pointee_shape),
                            Def::Result(_) => Frame::new_result(pointee_data, pointee_shape),
                            _ => match pointee_shape.ty {
                                Type::User(UserType::Struct(ref s)) => {
                                    Frame::new_struct(pointee_data, pointee_shape, s.fields.len())
                                }
                                Type::User(UserType::Enum(_)) => {
                                    Frame::new_enum(pointee_data, pointee_shape)
                                }
                                _ => Frame::new_pointer(pointee_data, pointee_shape),
                            },
                        };
                        // Mark that this frame owns its allocation (for cleanup on error)
                        new_frame.flags |= FrameFlags::OWNS_ALLOC;
                        // Link to parent as a pointer inner
                        new_frame.parent_link = ParentLink::PointerInner {
                            parent: self.current,
                        };

                        // Record the frame in parent's pointer state
                        let new_idx = self.arena.alloc(new_frame);

                        // Update parent to track this as a pointer frame
                        let frame = self.arena.get_mut(self.current);
                        frame.kind = FrameKind::Pointer(PointerFrame { inner: new_idx });

                        self.current = new_idx;
                        return Ok(());
                    }

                    // Handle array types
                    if let Def::Array(array_def) = *shape.def() {
                        // Arrays don't need initialization - memory is already allocated
                        // Just convert to struct frame for element tracking (arrays are like structs)
                        let frame = self.arena.get_mut(self.current);
                        frame.kind = FrameKind::Struct(StructFrame::new(array_def.n));
                        return Ok(());
                    }

                    return Err(self.error(ReflectErrorKind::BuildAtEmptyPath));
                }

                // Drop any existing value at the field before overwriting
                let field_idx = first_field_idx(path).expect("path must have field index");
                let frame = self.arena.get_mut(self.current);
                frame.prepare_field_for_overwrite(field_idx as usize);

                // Resolve path to get target shape and pointer
                let frame = self.arena.get(self.current);
                let target = self.resolve_path(frame, path)?;

                // Get parent link based on current frame kind
                let parent_link = make_field_parent_link(&frame.kind, self.current, field_idx);

                // Check if target is a list - create frame, lazy init on first Push
                let target_def = *target.shape.def();
                if let Def::List(list_def) = target_def {
                    let mut new_frame = Frame::new_list(target.data, target.shape, list_def);
                    new_frame.parent_link = parent_link;

                    let new_idx = self.arena.alloc(new_frame);
                    self.current = new_idx;
                } else if let Def::Map(map_def) = target_def {
                    // Create frame, lazy init on first Insert
                    let mut new_frame = Frame::new_map(target.data, target.shape, map_def);
                    new_frame.parent_link = parent_link;

                    let new_idx = self.arena.alloc(new_frame);
                    self.current = new_idx;
                } else if let Def::Set(set_def) = target_def {
                    // Create frame, lazy init on first Push
                    let mut new_frame = Frame::new_set(target.data, target.shape, set_def);
                    new_frame.parent_link = parent_link;

                    let new_idx = self.arena.alloc(new_frame);
                    self.current = new_idx;
                } else if let Def::Option(_) = target_def {
                    // Create Option frame
                    let mut new_frame = Frame::new_option(target.data, target.shape);
                    new_frame.parent_link = parent_link;

                    let new_idx = self.arena.alloc(new_frame);
                    self.current = new_idx;
                } else if let Def::Result(_) = target_def {
                    // Create Result frame
                    let mut new_frame = Frame::new_result(target.data, target.shape);
                    new_frame.parent_link = parent_link;

                    let new_idx = self.arena.alloc(new_frame);
                    self.current = new_idx;
                } else {
                    let mut new_frame = match *target.shape.ty() {
                        Type::User(UserType::Struct(ref s)) => {
                            Frame::new_struct(target.data, target.shape, s.fields.len())
                        }
                        Type::User(UserType::Enum(_)) => Frame::new_enum(target.data, target.shape),
                        Type::Sequence(SequenceType::Array(ref a)) => {
                            Frame::new_struct(target.data, target.shape, a.n)
                        }
                        _ => Frame::new(target.data, target.shape),
                    };
                    new_frame.parent_link = parent_link;

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
                    let field_idx =
                        first_field_idx(path).expect("path must have field index") as usize;
                    frame.prepare_field_for_overwrite(field_idx);
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

                // Mark as initialized
                let is_empty_path = path.is_empty();
                {
                    let frame = self.arena.get_mut(self.current);

                    if is_empty_path {
                        frame.flags |= FrameFlags::INIT;

                        // For maps, update the MapFrame state
                    } else {
                        // Mark child as complete
                        let field_idx =
                            first_field_idx(path).expect("path must have field index") as usize;
                        frame.kind.mark_field_complete(field_idx);
                    }
                }

                // Sync collection frame states after Default at empty path
                // (called outside the scope to avoid borrow conflict)
                if is_empty_path {
                    self.list_sync_after_set();
                    self.map_sync_after_set();
                    self.set_sync_after_set();
                }
            }
        }
        Ok(())
    }
}
