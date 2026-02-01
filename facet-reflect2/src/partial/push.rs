use super::Partial;
use crate::errors::{ReflectError, ReflectErrorKind};
use crate::frame::{Frame, FrameKind, ParentLink};
use crate::ops::Source;
use crate::shape_desc::ShapeDesc;
use facet_core::{Def, PtrUninit, Shape, Type, UserType};

impl<'facet> Partial<'facet> {
    /// Apply a Push operation to add an element to the current list or set.
    pub(crate) fn apply_push(&mut self, source: &Source<'_>) -> Result<(), ReflectError> {
        // Ensure collection is initialized (lazy init on first push)
        self.ensure_collection_initialized()?;

        // Check if this is a list or set
        let frame = self.arena.get(self.current);
        match &frame.kind {
            FrameKind::List(_) => self.list_append(source),
            FrameKind::Set(_) => self.apply_push_to_set(source),
            _ => Err(self.error(ReflectErrorKind::NotAList)),
        }
    }

    /// Push to a set using slab-based collection.
    ///
    /// Sets use a slab to collect elements, then build the set at End via `from_slice`.
    fn apply_push_to_set(&mut self, source: &Source<'_>) -> Result<(), ReflectError> {
        let frame = self.arena.get(self.current);
        let FrameKind::Set(set_frame) = &frame.kind else {
            unreachable!()
        };
        let def = set_frame.def;
        let element_shape = def.t;
        let current_len = set_frame.len;

        match source {
            Source::Imm(mov) => {
                if !element_shape.is_shape(mov.shape()) {
                    return Err(self.error(ReflectErrorKind::ShapeMismatch {
                        expected: ShapeDesc::Static(element_shape),
                        actual: ShapeDesc::Static(mov.shape()),
                    }));
                }

                // Get element size for copying
                let element_size = element_shape
                    .layout
                    .sized_layout()
                    .map(|l| l.size())
                    .unwrap_or(0);

                // Get a slot from the slab
                let frame = self.arena.get_mut(self.current);
                let FrameKind::Set(ref mut set_frame) = frame.kind else {
                    unreachable!()
                };
                let slab = set_frame
                    .slab
                    .as_mut()
                    .expect("slab must exist after ensure_collection_initialized");
                let slot = slab.nth_slot(current_len);

                // Copy the element into the slab
                if element_size > 0 {
                    unsafe {
                        std::ptr::copy_nonoverlapping(
                            mov.ptr().as_byte_ptr(),
                            slot.as_mut_byte_ptr(),
                            element_size,
                        );
                    }
                }

                // Increment element count
                set_frame.len += 1;
            }
            Source::Stage(_capacity) => {
                // Get a slot from the slab
                let frame = self.arena.get_mut(self.current);
                let FrameKind::Set(ref mut set_frame) = frame.kind else {
                    unreachable!()
                };
                let slab = set_frame
                    .slab
                    .as_mut()
                    .expect("slab must exist after ensure_collection_initialized");
                let slot = slab.nth_slot(current_len);

                // Create frame pointing into the slab
                // Do NOT set OWNS_ALLOC - the slab owns this memory
                let mut element_frame = Self::create_frame_for_shape(slot, element_shape);
                element_frame.parent_link = ParentLink::SetElement {
                    parent: self.current,
                };

                let element_idx = self.arena.alloc(element_frame);
                self.current = element_idx;
            }
            Source::Default => {
                // Get a slot from the slab
                let frame = self.arena.get_mut(self.current);
                let FrameKind::Set(ref mut set_frame) = frame.kind else {
                    unreachable!()
                };
                let slab = set_frame
                    .slab
                    .as_mut()
                    .expect("slab must exist after ensure_collection_initialized");
                let slot = slab.nth_slot(current_len);

                // Initialize with default directly into slab
                let ok = unsafe { element_shape.call_default_in_place(slot) };
                if ok.is_none() {
                    return Err(self.error(ReflectErrorKind::NoDefault {
                        shape: ShapeDesc::Static(element_shape),
                    }));
                }

                // Increment element count
                set_frame.len += 1;
            }
        }

        Ok(())
    }

    /// Create a frame appropriate for the given shape.
    pub(crate) fn create_frame_for_shape(ptr: PtrUninit, shape: &'static Shape) -> Frame {
        match shape.def {
            Def::List(list_def) => Frame::new_list(ptr, shape, list_def),
            Def::Map(map_def) => Frame::new_map(ptr, shape, map_def),
            Def::Set(set_def) => Frame::new_set(ptr, shape, set_def),
            Def::Option(_) => Frame::new_option(ptr, shape),
            Def::Result(_) => Frame::new_result(ptr, shape),
            _ => match shape.ty {
                Type::User(UserType::Struct(ref s)) => {
                    Frame::new_struct(ptr, shape, s.fields.len())
                }
                Type::User(UserType::Enum(_)) => Frame::new_enum(ptr, shape),
                _ => Frame::new(ptr, shape),
            },
        }
    }
}
