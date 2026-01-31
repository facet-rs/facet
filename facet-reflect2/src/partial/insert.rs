use super::Partial;
use crate::errors::{ReflectError, ReflectErrorKind};
use crate::frame::{Frame, FrameFlags, FrameKind};
use crate::ops::{Imm, Source};
use facet_core::{Def, PtrMut, Type, UserType};

impl<'facet> Partial<'facet> {
    /// Apply an Insert operation to add a key-value pair to the current map.
    pub(crate) fn apply_insert(
        &mut self,
        key: &Imm<'_>,
        value: &Source<'_>,
    ) -> Result<(), ReflectError> {
        // Ensure map is initialized (lazy init on first insert)
        self.ensure_collection_initialized()?;

        // Verify we're in a map frame and get the def
        let frame = self.arena.get(self.current);
        let FrameKind::Map(ref map_frame) = frame.kind else {
            return Err(self.error(ReflectErrorKind::NotAMap));
        };

        let map_def = map_frame.def;
        let key_shape = map_def.k;
        let value_shape = map_def.v;
        let insert_fn = map_def.vtable.insert;

        // Verify key shape matches
        if !key_shape.is_shape(key.shape()) {
            return Err(self.error(ReflectErrorKind::KeyShapeMismatch {
                expected: key_shape,
                actual: key.shape(),
            }));
        }

        // SAFETY: we just ensured the map is initialized
        let map_ptr = unsafe { frame.data.assume_init() };

        match value {
            Source::Imm(mov) => {
                use crate::temp_alloc::TempAlloc;

                // Verify value shape matches
                if !value_shape.is_shape(mov.shape()) {
                    return Err(self.error(ReflectErrorKind::ValueShapeMismatch {
                        expected: value_shape,
                        actual: mov.shape(),
                    }));
                }

                // Allocate and copy key
                let mut key_alloc = TempAlloc::new(key_shape).map_err(|kind| self.error(kind))?;
                unsafe {
                    key_alloc.copy_from(key.ptr());
                }

                // Allocate and copy value
                let mut value_alloc =
                    TempAlloc::new(value_shape).map_err(|kind| self.error(kind))?;
                unsafe {
                    value_alloc.copy_from(mov.ptr());
                }

                // Insert the key-value pair (moves both out)
                // SAFETY: both pointers point to valid initialized data
                unsafe {
                    insert_fn(
                        map_ptr,
                        PtrMut::new(key_alloc.ptr().as_mut_byte_ptr()),
                        PtrMut::new(value_alloc.ptr().as_mut_byte_ptr()),
                    );
                }

                // Mark as moved so TempAlloc doesn't drop the values
                key_alloc.mark_moved();
                value_alloc.mark_moved();
                // TempAlloc drops here, deallocating storage

                // Increment entry count
                let frame = self.arena.get_mut(self.current);
                if let FrameKind::Map(ref mut m) = frame.kind {
                    m.len += 1;
                }
            }
            Source::Build(_build) => {
                use crate::temp_alloc::TempAlloc;

                // Allocate temp storage for key and copy it
                let mut key_alloc = TempAlloc::new(key_shape).map_err(|kind| self.error(kind))?;
                unsafe {
                    key_alloc.copy_from(key.ptr());
                }

                // Allocate temporary space for the value
                let value_alloc = TempAlloc::new(value_shape).map_err(|kind| self.error(kind))?;
                let value_ptr = value_alloc.ptr();

                // Create appropriate frame based on value shape
                let mut value_frame = match &value_shape.def {
                    Def::List(list_def) => Frame::new_list(value_ptr, value_shape, list_def),
                    Def::Map(map_def) => Frame::new_map(value_ptr, value_shape, map_def),
                    Def::Set(set_def) => Frame::new_set(value_ptr, value_shape, set_def),
                    Def::Option(_) => Frame::new_option(value_ptr, value_shape),
                    Def::Result(_) => Frame::new_result(value_ptr, value_shape),
                    _ => match value_shape.ty {
                        Type::User(UserType::Struct(ref s)) => {
                            Frame::new_struct(value_ptr, value_shape, s.fields.len())
                        }
                        Type::User(UserType::Enum(_)) => Frame::new_enum(value_ptr, value_shape),
                        _ => Frame::new(value_ptr, value_shape),
                    },
                };

                // Mark that this frame owns its allocation (for cleanup on End)
                value_frame.flags |= FrameFlags::OWNS_ALLOC;

                // Set parent to current map frame
                value_frame.parent = Some((self.current, 0));

                // Store the pending key (transfer ownership to the frame)
                value_frame.pending_key = Some(key_alloc);

                // Transfer value allocation ownership to frame (don't drop/dealloc here)
                std::mem::forget(value_alloc);

                // Push frame and make it current
                let value_idx = self.arena.alloc(value_frame);
                self.current = value_idx;
            }
            Source::Default => {
                use crate::temp_alloc::TempAlloc;

                // Allocate and copy key
                let mut key_alloc = TempAlloc::new(key_shape).map_err(|kind| self.error(kind))?;
                unsafe {
                    key_alloc.copy_from(key.ptr());
                }

                // Allocate and initialize value with default
                let mut value_alloc =
                    TempAlloc::new(value_shape).map_err(|kind| self.error(kind))?;
                if value_alloc.init_default().is_none() {
                    return Err(self.error(ReflectErrorKind::NoDefault { shape: value_shape }));
                }

                // Insert the key-value pair (moves both out)
                // SAFETY: both pointers point to valid initialized data
                unsafe {
                    insert_fn(
                        map_ptr,
                        PtrMut::new(key_alloc.ptr().as_mut_byte_ptr()),
                        PtrMut::new(value_alloc.ptr().as_mut_byte_ptr()),
                    );
                }

                // Mark as moved so TempAlloc doesn't drop the values
                key_alloc.mark_moved();
                value_alloc.mark_moved();
                // TempAlloc drops here, deallocating storage

                // Increment entry count
                let frame = self.arena.get_mut(self.current);
                if let FrameKind::Map(ref mut m) = frame.kind {
                    m.len += 1;
                }
            }
        }

        Ok(())
    }
}
