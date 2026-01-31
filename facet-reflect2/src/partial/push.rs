use super::Partial;
use crate::errors::{ReflectError, ReflectErrorKind};
use crate::frame::{Frame, FrameFlags, FrameKind, ParentLink};
use crate::ops::Source;
use facet_core::{Def, PtrUninit, Shape, Type, UserType};

impl<'facet> Partial<'facet> {
    /// Apply a Push operation to add an element to the current list or set.
    pub(crate) fn apply_push(&mut self, source: &Source<'_>) -> Result<(), ReflectError> {
        // Ensure collection is initialized (lazy init on first push)
        self.ensure_collection_initialized()?;

        // Get the collection info from the frame kind
        let frame = self.arena.get(self.current);
        let collection_ptr = unsafe { frame.data.assume_init() };

        enum CollectionKind {
            List {
                push_fn: facet_core::ListPushFn,
                element_shape: &'static Shape,
            },
            Set {
                insert_fn: facet_core::SetInsertFn,
                element_shape: &'static Shape,
            },
        }

        let collection = match &frame.kind {
            FrameKind::List(list_frame) => {
                let push_fn = list_frame.def.push().ok_or_else(|| {
                    self.error(ReflectErrorKind::ListDoesNotSupportOp { shape: frame.shape })
                })?;
                CollectionKind::List {
                    push_fn,
                    element_shape: list_frame.def.t,
                }
            }
            FrameKind::Set(set_frame) => CollectionKind::Set {
                insert_fn: set_frame.def.vtable.insert,
                element_shape: set_frame.def.t,
            },
            _ => return Err(self.error(ReflectErrorKind::NotAList)),
        };

        match source {
            Source::Imm(mov) => {
                // Verify element shape matches and push
                match collection {
                    CollectionKind::List {
                        push_fn,
                        element_shape,
                    } => {
                        if !element_shape.is_shape(mov.shape()) {
                            return Err(self.error(ReflectErrorKind::ShapeMismatch {
                                expected: element_shape,
                                actual: mov.shape(),
                            }));
                        }
                        // SAFETY: mov.ptr() points to valid initialized data of the element type
                        unsafe { push_fn(collection_ptr, mov.ptr()) };
                        let frame = self.arena.get_mut(self.current);
                        if let FrameKind::List(ref mut l) = frame.kind {
                            l.len += 1;
                        }
                    }
                    CollectionKind::Set {
                        insert_fn,
                        element_shape,
                    } => {
                        if !element_shape.is_shape(mov.shape()) {
                            return Err(self.error(ReflectErrorKind::ShapeMismatch {
                                expected: element_shape,
                                actual: mov.shape(),
                            }));
                        }
                        // SAFETY: mov.ptr() points to valid initialized data of the element type
                        unsafe { insert_fn(collection_ptr, mov.ptr_mut()) };
                        let frame = self.arena.get_mut(self.current);
                        if let FrameKind::Set(ref mut s) = frame.kind {
                            s.len += 1;
                        }
                    }
                }
            }
            Source::Stage(_capacity) => {
                let element_shape = match &collection {
                    CollectionKind::List { element_shape, .. } => *element_shape,
                    CollectionKind::Set { element_shape, .. } => *element_shape,
                };

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
                let mut element_frame = match &element_shape.def {
                    Def::List(list_def) => Frame::new_list(temp_ptr, element_shape, list_def),
                    Def::Map(map_def) => Frame::new_map(temp_ptr, element_shape, map_def),
                    Def::Set(set_def) => Frame::new_set(temp_ptr, element_shape, set_def),
                    Def::Option(_) => Frame::new_option(temp_ptr, element_shape),
                    Def::Result(_) => Frame::new_result(temp_ptr, element_shape),
                    _ => match element_shape.ty {
                        Type::User(UserType::Struct(ref s)) => {
                            Frame::new_struct(temp_ptr, element_shape, s.fields.len())
                        }
                        Type::User(UserType::Enum(_)) => Frame::new_enum(temp_ptr, element_shape),
                        _ => Frame::new(temp_ptr, element_shape),
                    },
                };

                // Mark that this frame owns its allocation (for cleanup on End)
                element_frame.flags |= FrameFlags::OWNS_ALLOC;

                // Set parent link based on collection type
                element_frame.parent_link = match &collection {
                    CollectionKind::List { .. } => ParentLink::ListElement {
                        parent: self.current,
                    },
                    CollectionKind::Set { .. } => ParentLink::SetElement {
                        parent: self.current,
                    },
                };

                // Push frame and make it current
                let element_idx = self.arena.alloc(element_frame);
                self.current = element_idx;
            }
            Source::Default => {
                let element_shape = match &collection {
                    CollectionKind::List { element_shape, .. } => *element_shape,
                    CollectionKind::Set { element_shape, .. } => *element_shape,
                };

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

                // Push/insert the element
                // SAFETY: temp_ptr now contains initialized data
                match collection {
                    CollectionKind::List { push_fn, .. } => {
                        unsafe { push_fn(collection_ptr, temp_ptr.assume_init().as_const()) };
                        let frame = self.arena.get_mut(self.current);
                        if let FrameKind::List(ref mut l) = frame.kind {
                            l.len += 1;
                        }
                    }
                    CollectionKind::Set { insert_fn, .. } => {
                        unsafe { insert_fn(collection_ptr, temp_ptr.assume_init()) };
                        let frame = self.arena.get_mut(self.current);
                        if let FrameKind::Set(ref mut s) = frame.kind {
                            s.len += 1;
                        }
                    }
                }

                // Deallocate temp storage (value was moved out by push/insert)
                if layout.size() > 0 {
                    unsafe { std::alloc::dealloc(temp_ptr.as_mut_byte_ptr(), layout) };
                }
            }
        }

        Ok(())
    }
}
