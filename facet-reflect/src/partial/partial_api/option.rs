use super::*;

////////////////////////////////////////////////////////////////////////////////////////////////////
// Option / inner
////////////////////////////////////////////////////////////////////////////////////////////////////
impl Partial<'_> {
    /// Begin building the Some variant of an Option
    pub fn begin_some(&mut self) -> Result<&mut Self, ReflectError> {
        self.require_active()?;

        // First, gather information and perform drops with proper borrow management
        let (option_def, was_initialized) = {
            let frame = self.frames.last().unwrap();

            // Verify we're working with an Option
            let option_def = match frame.shape.def {
                Def::Option(def) => def,
                _ => {
                    return Err(ReflectError::WasNotA {
                        expected: "Option",
                        actual: frame.shape,
                    });
                }
            };

            // If the Option was already initialized, we need to drop the old value first.
            // This handles cases where:
            // - Tracker is Init (re-entering an already initialized Option field)
            // - Tracker is Option{building_inner:false} (second call to begin_some after first completed)
            let was_initialized = matches!(
                frame.tracker,
                Tracker::Init | Tracker::Option { building_inner: false }
            );

            (option_def, was_initialized)
        };

        if was_initialized {
            // Drop the existing Option value before starting a new building cycle
            let frame = self.frames.last().unwrap();
            if let Some(drop_fn) = frame.shape.vtable.drop_in_place {
                unsafe { drop_fn(frame.data.assume_init()) };
            }

            // IMPORTANT: After dropping, we need to unmark this field in the parent's iset.
            // Otherwise, if the Partial is dropped before end() completes the new Option,
            // the parent struct's deinit will try to drop this field again (double-free).
            // When end() successfully completes, it will re-mark the field in the parent's iset.
            if self.frames.len() >= 2 {
                let parent_idx = self.frames.len() - 2;
                if let Some(parent_frame) = self.frames.get_mut(parent_idx) {
                    match &mut parent_frame.tracker {
                        Tracker::Struct { iset, current_child } => {
                            if let Some(idx) = *current_child {
                                iset.unset(idx);
                            }
                        }
                        Tracker::Enum { data, current_child, .. } => {
                            if let Some(idx) = *current_child {
                                data.unset(idx);
                            }
                        }
                        _ => {}
                    }
                }
            }
        }

        // Set tracker to indicate we're building the inner value
        let frame = self.frames.last_mut().unwrap();
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
        self.frames.push(inner_frame);

        Ok(self)
    }

    /// Begin building the inner value of a wrapper type
    pub fn begin_inner(&mut self) -> Result<&mut Self, ReflectError> {
        self.require_active()?;

        // Get the inner shape and check for try_from
        let (inner_shape, has_try_from, parent_shape, is_option) = {
            let frame = self.frames.last().unwrap();
            if let Some(inner_shape) = frame.shape.inner {
                let has_try_from = frame.shape.vtable.try_from.is_some();
                let is_option = matches!(frame.shape.def, Def::Option(_));
                (Some(inner_shape), has_try_from, frame.shape, is_option)
            } else {
                (None, false, frame.shape, false)
            }
        };

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
                self.frames
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
    pub fn begin_custom_deserialization(&mut self) -> Result<&mut Self, ReflectError> {
        self.require_active()?;

        let current_frame = self.frames.last().unwrap();
        let target_shape = current_frame.shape;
        if let Some(field) = self.parent_field() {
            if field.vtable.deserialize_with.is_some() {
                // TODO: can we assume that this is set if the vtable element is set?
                // TODO: can we get the shape some other way?
                let Some(FieldAttribute::DeserializeFrom(source_shape)) = field
                    .attributes
                    .iter()
                    .find(|&p| matches!(p, FieldAttribute::DeserializeFrom(_)))
                else {
                    panic!("expected field attribute to be present with deserialize_with");
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
                self.frames.push(new_frame);

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
