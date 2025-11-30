use super::*;

////////////////////////////////////////////////////////////////////////////////////////////////////
// Option / inner
////////////////////////////////////////////////////////////////////////////////////////////////////
impl Partial<'_> {
    /// Begin building the Some variant of an Option
    pub fn begin_some(mut self) -> Result<Self, ReflectError> {
        // Verify we're working with an Option and get the def
        let option_def = {
            let frame = self.frames().last().unwrap();
            match frame.shape.def {
                Def::Option(def) => def,
                _ => {
                    return Err(ReflectError::WasNotA {
                        expected: "Option",
                        actual: frame.shape,
                    });
                }
            }
        };

        // Check if we need to handle re-initialization.
        // For Options, also check if tracker is Option{building_inner:false} which means
        // a previous begin_some/end cycle completed.
        let needs_reinit = {
            let frame = self.frames().last().unwrap();
            frame.is_init
                || matches!(
                    frame.tracker,
                    Tracker::Option {
                        building_inner: false
                    }
                )
        };

        if needs_reinit {
            self.prepare_for_reinitialization();
        }

        // Set tracker to indicate we're building the inner value
        let frame = self.frames_mut().last_mut().unwrap();
        frame.tracker = Tracker::Option {
            building_inner: true,
        };

        // Get the inner type shape
        let inner_shape = option_def.t;

        // Allocate memory for the inner value
        let inner_layout =
            inner_shape
                .layout
                .sized_layout()
                .map_err(|_| ReflectError::Unsized {
                    shape: inner_shape,
                    operation: "begin_some, allocating Option inner value",
                })?;

        let inner_data = if inner_layout.size() == 0 {
            // For ZST, use a non-null but unallocated pointer
            PtrUninit::new(NonNull::<u8>::dangling())
        } else {
            // Allocate memory for the inner value
            let ptr = unsafe { ::alloc::alloc::alloc(inner_layout) };
            let Some(ptr) = NonNull::new(ptr) else {
                ::alloc::alloc::handle_alloc_error(inner_layout);
            };
            PtrUninit::new(ptr)
        };

        // Create a new frame for the inner value
        let inner_frame = Frame::new(inner_data, inner_shape, FrameOwnership::Owned);
        self.frames_mut().push(inner_frame);

        Ok(self)
    }

    /// Begin building the inner value of a wrapper type
    pub fn begin_inner(mut self) -> Result<Self, ReflectError> {
        // Get the inner shape and check for try_from
        let (inner_shape, has_try_from, parent_shape, is_option) = {
            let frame = self.frames().last().unwrap();
            if let Some(inner_shape) = frame.shape.inner {
                let has_try_from = frame.shape.vtable.try_from.is_some();
                let is_option = matches!(frame.shape.def, Def::Option(_));
                (Some(inner_shape), has_try_from, frame.shape, is_option)
            } else {
                (None, false, frame.shape, false)
            }
        };

        // Handle re-initialization if needed
        self.prepare_for_reinitialization();

        if let Some(inner_shape) = inner_shape {
            if has_try_from {
                // For Option types, use begin_some behavior to properly track building_inner
                // This ensures end() knows how to handle the popped frame
                if is_option {
                    return self.begin_some();
                }

                // Create a conversion frame with the inner shape
                // For non-Option types with try_from, we leave the parent tracker unchanged
                // and the conversion will happen in end()

                // Allocate memory for the inner value (conversion source)
                let inner_layout =
                    inner_shape
                        .layout
                        .sized_layout()
                        .map_err(|_| ReflectError::Unsized {
                            shape: inner_shape,
                            operation: "begin_inner, getting inner layout",
                        })?;

                let inner_data = if inner_layout.size() == 0 {
                    // For ZST, use a non-null but unallocated pointer
                    PtrUninit::new(NonNull::<u8>::dangling())
                } else {
                    // Allocate memory for the inner value
                    let ptr = unsafe { ::alloc::alloc::alloc(inner_layout) };
                    let Some(ptr) = NonNull::new(ptr) else {
                        ::alloc::alloc::handle_alloc_error(inner_layout);
                    };
                    PtrUninit::new(ptr)
                };

                // For conversion frames, we create a frame directly with the inner shape
                // This allows setting values of the inner type which will be converted
                // The automatic conversion detection in end() will handle the conversion
                trace!(
                    "begin_inner: Creating frame for inner type {inner_shape} (parent is {parent_shape})"
                );
                self.frames_mut()
                    .push(Frame::new(inner_data, inner_shape, FrameOwnership::Owned));

                Ok(self)
            } else {
                // For wrapper types without try_from, navigate to the first field
                // This is a common pattern for newtype wrappers
                trace!("begin_inner: No try_from for {parent_shape}, using field navigation");
                self.begin_nth_field(0)
            }
        } else {
            Err(ReflectError::OperationFailed {
                shape: parent_shape,
                operation: "type does not have an inner value",
            })
        }
    }

    /// Begin bulding the source shape for custom deserialization, calling end() for this frame will
    /// call the deserialize_with function provided by the field and set the field using the result.
    pub fn begin_custom_deserialization(mut self) -> Result<Self, ReflectError> {
        let current_frame = self.frames().last().unwrap();
        let target_shape = current_frame.shape;
        trace!("begin_custom_deserialization: target_shape={target_shape}");
        if let Some(field) = self.parent_field() {
            trace!("begin_custom_deserialization: field name={}", field.name);
            if field.proxy_convert_in_fn().is_some() {
                // Get the source shape from the proxy attribute
                let Some(source_shape) = field.proxy_shape() else {
                    panic!("expected proxy attribute to be present with deserialize_with");
                };
                let source_data = source_shape.allocate().map_err(|_| ReflectError::Unsized {
                    shape: target_shape,
                    operation: "Not a Sized type",
                })?;

                trace!(
                    "begin_custom_deserialization: Creating frame for deserialization type {source_shape}"
                );
                let mut new_frame = Frame::new(source_data, source_shape, FrameOwnership::Owned);
                new_frame.using_custom_deserialization = true;
                self.frames_mut().push(new_frame);

                Ok(self)
            } else {
                Err(ReflectError::OperationFailed {
                    shape: target_shape,
                    operation: "field does not have a deserialize_with function",
                })
            }
        } else {
            Err(ReflectError::OperationFailed {
                shape: target_shape,
                operation: "not currently processing a field",
            })
        }
    }
}
