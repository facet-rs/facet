use super::*;

////////////////////////////////////////////////////////////////////////////////////////////////////
// Smart pointers
////////////////////////////////////////////////////////////////////////////////////////////////////
impl Partial<'_> {
    /// Pushes a frame to initialize the inner value of a smart pointer (`Box<T>`, `Arc<T>`, etc.)
    pub fn begin_smart_ptr(&mut self) -> Result<&mut Self, ReflectError> {
        crate::trace!("begin_smart_ptr()");
        self.require_active()?;
        let frame = self.frames.last_mut().unwrap();

        // Check that we have a SmartPointer
        match &frame.shape.def {
            Def::Pointer(smart_ptr_def) if smart_ptr_def.constructible_from_pointee() => {
                // Get the pointee shape
                let pointee_shape = match smart_ptr_def.pointee() {
                    Some(shape) => shape,
                    None => {
                        return Err(ReflectError::OperationFailed {
                            shape: frame.shape,
                            operation: "Smart pointer must have a pointee shape",
                        });
                    }
                };

                if pointee_shape.layout.sized_layout().is_ok() {
                    // pointee is sized, we can allocate it — for `Arc<T>` we'll be allocating a `T` and
                    // holding onto it. We'll build a new Arc with it when ending the smart pointer frame.

                    if matches!(frame.tracker, Tracker::Uninit) {
                        frame.tracker = Tracker::SmartPointer {
                            is_initialized: false,
                        };
                    }

                    let inner_layout = match pointee_shape.layout.sized_layout() {
                        Ok(layout) => layout,
                        Err(_) => {
                            return Err(ReflectError::Unsized {
                                shape: pointee_shape,
                                operation: "begin_smart_ptr, calculating inner value layout",
                            });
                        }
                    };
                    let inner_ptr: *mut u8 = unsafe { ::alloc::alloc::alloc(inner_layout) };
                    let Some(inner_ptr) = NonNull::new(inner_ptr) else {
                        return Err(ReflectError::OperationFailed {
                            shape: frame.shape,
                            operation: "failed to allocate memory for smart pointer inner value",
                        });
                    };

                    // Push a new frame for the inner value
                    self.frames.push(Frame::new(
                        PtrUninit::new(inner_ptr),
                        pointee_shape,
                        FrameOwnership::Owned,
                    ));
                } else {
                    // pointee is unsized, we only support a handful of cases there
                    if pointee_shape == str::SHAPE {
                        crate::trace!("Pointee is str");

                        // Allocate space for a String
                        let string_layout = String::SHAPE
                            .layout
                            .sized_layout()
                            .expect("String must have a sized layout");
                        let string_ptr: *mut u8 = unsafe { ::alloc::alloc::alloc(string_layout) };
                        let Some(string_ptr) = NonNull::new(string_ptr) else {
                            return Err(ReflectError::OperationFailed {
                                shape: frame.shape,
                                operation: "failed to allocate memory for string",
                            });
                        };
                        let mut frame = Frame::new(
                            PtrUninit::new(string_ptr),
                            String::SHAPE,
                            FrameOwnership::Owned,
                        );
                        frame.tracker = Tracker::Uninit;
                        self.frames.push(frame);
                    } else if let Type::Sequence(SequenceType::Slice(_st)) = pointee_shape.ty {
                        crate::trace!("Pointee is [{}]", _st.t);

                        // Get the slice builder vtable
                        let slice_builder_vtable = smart_ptr_def
                            .vtable
                            .slice_builder_vtable
                            .ok_or(ReflectError::OperationFailed {
                                shape: frame.shape,
                                operation: "smart pointer does not support slice building",
                            })?;

                        // Create a new builder
                        let builder_ptr = (slice_builder_vtable.new_fn)();

                        // Deallocate the original Arc allocation before replacing with slice builder
                        if let FrameOwnership::Owned = frame.ownership {
                            if let Ok(layout) = frame.shape.layout.sized_layout() {
                                if layout.size() > 0 {
                                    unsafe {
                                        ::alloc::alloc::dealloc(
                                            frame.data.as_mut_byte_ptr(),
                                            layout,
                                        )
                                    };
                                }
                            }
                        }

                        // Update the current frame to use the slice builder
                        frame.data = builder_ptr.as_uninit();
                        frame.tracker = Tracker::SmartPointerSlice {
                            vtable: slice_builder_vtable,
                            building_item: false,
                        };
                        // The slice builder memory is managed by the vtable, not by us
                        frame.ownership = FrameOwnership::ManagedElsewhere;
                    } else {
                        return Err(ReflectError::OperationFailed {
                            shape: frame.shape,
                            operation: "push_smart_ptr can only be called on pointers to supported pointee types",
                        });
                    }
                }

                Ok(self)
            }
            _ => Err(ReflectError::OperationFailed {
                shape: frame.shape,
                operation: "push_smart_ptr can only be called on compatible types",
            }),
        }
    }
}
