use super::*;

////////////////////////////////////////////////////////////////////////////////////////////////////
// Lists
////////////////////////////////////////////////////////////////////////////////////////////////////
impl<const BORROW: bool> Partial<'_, BORROW> {
    /// Initializes a list (Vec, etc.) if it hasn't been initialized before.
    /// This is a prerequisite to `begin_push_item`/`set`/`end` or the shorthand
    /// `push`.
    ///
    /// `begin_list` does not clear the list if it was previously initialized.
    /// `begin_list` does not push a new frame to the stack, and thus does not
    /// require `end` to be called afterwards.
    pub fn begin_list(mut self) -> Result<Self, ReflectError> {
        crate::trace!("begin_list()");
        let frame = self.frames_mut().last_mut().unwrap();

        match &frame.tracker {
            Tracker::Scalar if !frame.is_init => {
                // that's good, let's initialize it
            }
            Tracker::Scalar => {
                // is_init is true - initialized (perhaps from a previous round?) but should be a list tracker
                // Check what kind of shape we have
                match &frame.shape.def {
                    Def::List(_) => {
                        // Regular list type - just update the tracker
                        frame.tracker = Tracker::List {
                            current_child: false,
                        };
                        return Ok(self);
                    }
                    Def::DynamicValue(_) => {
                        // DynamicValue that was already initialized as an array
                        // Just update the tracker without deinit (preserve existing elements)
                        frame.tracker = Tracker::DynamicValue {
                            state: DynamicValueState::Array {
                                building_element: false,
                            },
                        };
                        return Ok(self);
                    }
                    _ => {
                        return Err(ReflectError::OperationFailed {
                            shape: frame.shape,
                            operation: "begin_list can only be called on List types or DynamicValue",
                        });
                    }
                }
            }
            Tracker::List { .. } => {
                if frame.is_init {
                    // already initialized, nothing to do
                    return Ok(self);
                }
            }
            Tracker::DynamicValue { state } => {
                // Already initialized as a dynamic array
                if matches!(state, DynamicValueState::Array { .. }) {
                    return Ok(self);
                }
                // Otherwise (Scalar or other state), we need to deinit before reinitializing.
                // For ManagedElsewhere frames, deinit() skips dropping, so drop explicitly.
                if matches!(frame.ownership, FrameOwnership::ManagedElsewhere) && frame.is_init {
                    unsafe { frame.shape.call_drop_in_place(frame.data.assume_init()) };
                }
                frame.deinit();
            }
            Tracker::SmartPointerSlice { .. } => {
                // begin_list is kinda superfluous when we're in a SmartPointerSlice state
                return Ok(self);
            }
            _ => {
                return Err(ReflectError::UnexpectedTracker {
                    message: "begin_list called but tracker isn't something list-like",
                    current_tracker: frame.tracker.kind(),
                });
            }
        };

        // Check that we have a List or DynamicValue
        match &frame.shape.def {
            Def::List(list_def) => {
                // Check that we have init_in_place_with_capacity function
                let init_fn = match list_def.init_in_place_with_capacity() {
                    Some(f) => f,
                    None => {
                        return Err(ReflectError::OperationFailed {
                            shape: frame.shape,
                            operation: "list type does not support initialization with capacity",
                        });
                    }
                };

                // Initialize the list with default capacity (0)
                unsafe {
                    init_fn(frame.data, 0);
                }

                // Update tracker to List state and mark as initialized
                frame.tracker = Tracker::List {
                    current_child: false,
                };
                frame.is_init = true;
            }
            Def::DynamicValue(dyn_def) => {
                // Initialize as a dynamic array
                unsafe {
                    (dyn_def.vtable.begin_array)(frame.data);
                }

                // Update tracker to DynamicValue array state and mark as initialized
                frame.tracker = Tracker::DynamicValue {
                    state: DynamicValueState::Array {
                        building_element: false,
                    },
                };
                frame.is_init = true;
            }
            _ => {
                return Err(ReflectError::OperationFailed {
                    shape: frame.shape,
                    operation: "begin_list can only be called on List or DynamicValue types",
                });
            }
        }

        Ok(self)
    }

    /// Pushes an element to the list
    /// The element should be set using `set()` or similar methods, then `pop()` to complete
    pub fn begin_list_item(mut self) -> Result<Self, ReflectError> {
        crate::trace!("begin_list_item()");
        let frame = self.frames_mut().last_mut().unwrap();

        // Check if we're building a smart pointer slice
        if let Tracker::SmartPointerSlice {
            building_item,
            vtable: _,
        } = &frame.tracker
        {
            if *building_item {
                return Err(ReflectError::OperationFailed {
                    shape: frame.shape,
                    operation: "already building an item, call end() first",
                });
            }

            // Get the element type from the smart pointer's pointee
            let element_shape = match &frame.shape.def {
                Def::Pointer(smart_ptr_def) => match smart_ptr_def.pointee() {
                    Some(pointee_shape) => match &pointee_shape.ty {
                        Type::Sequence(SequenceType::Slice(slice_type)) => slice_type.t,
                        _ => {
                            return Err(ReflectError::OperationFailed {
                                shape: frame.shape,
                                operation: "smart pointer pointee is not a slice",
                            });
                        }
                    },
                    None => {
                        return Err(ReflectError::OperationFailed {
                            shape: frame.shape,
                            operation: "smart pointer has no pointee",
                        });
                    }
                },
                _ => {
                    return Err(ReflectError::OperationFailed {
                        shape: frame.shape,
                        operation: "expected smart pointer definition",
                    });
                }
            };

            // Allocate space for the element
            crate::trace!("Pointee is a slice of {element_shape}");
            let element_layout = match element_shape.layout.sized_layout() {
                Ok(layout) => layout,
                Err(_) => {
                    return Err(ReflectError::OperationFailed {
                        shape: element_shape,
                        operation: "cannot allocate unsized element",
                    });
                }
            };

            let element_ptr: *mut u8 = unsafe { ::alloc::alloc::alloc(element_layout) };
            let Some(element_ptr) = NonNull::new(element_ptr) else {
                return Err(ReflectError::OperationFailed {
                    shape: frame.shape,
                    operation: "failed to allocate memory for list element",
                });
            };

            // Create and push the element frame
            crate::trace!("Pushing element frame, which we just allocated");
            let element_frame = Frame::new(
                PtrUninit::new(element_ptr.as_ptr()),
                element_shape,
                FrameOwnership::Owned,
            );
            self.frames_mut().push(element_frame);

            // Mark that we're building an item
            // We need to update the tracker after pushing the frame
            let parent_idx = self.frames().len() - 2;
            if let Tracker::SmartPointerSlice { building_item, .. } =
                &mut self.frames_mut()[parent_idx].tracker
            {
                crate::trace!("Marking element frame as building item");
                *building_item = true;
            }

            return Ok(self);
        }

        // Check if we're building a DynamicValue array
        if let Tracker::DynamicValue {
            state: DynamicValueState::Array { building_element },
        } = &frame.tracker
        {
            if *building_element {
                return Err(ReflectError::OperationFailed {
                    shape: frame.shape,
                    operation: "already building an element, call end() first",
                });
            }

            // For DynamicValue arrays, the element shape is the same DynamicValue shape
            // (Value arrays contain Value elements)
            let element_shape = frame.shape;
            let element_layout = match element_shape.layout.sized_layout() {
                Ok(layout) => layout,
                Err(_) => {
                    return Err(ReflectError::Unsized {
                        shape: element_shape,
                        operation: "begin_list_item: calculating element layout",
                    });
                }
            };

            let element_ptr: *mut u8 = unsafe { ::alloc::alloc::alloc(element_layout) };
            let Some(element_ptr) = NonNull::new(element_ptr) else {
                return Err(ReflectError::OperationFailed {
                    shape: frame.shape,
                    operation: "failed to allocate memory for list element",
                });
            };

            // Push a new frame for the element
            self.frames_mut().push(Frame::new(
                PtrUninit::new(element_ptr.as_ptr()),
                element_shape,
                FrameOwnership::Owned,
            ));

            // Mark that we're building an element
            let parent_idx = self.frames().len() - 2;
            if let Tracker::DynamicValue {
                state: DynamicValueState::Array { building_element },
            } = &mut self.frames_mut()[parent_idx].tracker
            {
                *building_element = true;
            }

            return Ok(self);
        }

        // Check that we have a List that's been initialized
        let list_def = match &frame.shape.def {
            Def::List(list_def) => list_def,
            _ => {
                return Err(ReflectError::OperationFailed {
                    shape: frame.shape,
                    operation: "push can only be called on List or DynamicValue types",
                });
            }
        };

        // Verify the tracker is in List state and initialized
        match &mut frame.tracker {
            Tracker::List { current_child } if frame.is_init => {
                if *current_child {
                    return Err(ReflectError::OperationFailed {
                        shape: frame.shape,
                        operation: "already pushing an element, call pop() first",
                    });
                }
                *current_child = true;
            }
            _ => {
                return Err(ReflectError::OperationFailed {
                    shape: frame.shape,
                    operation: "must call begin_list() before push()",
                });
            }
        }

        // Get the element shape
        let element_shape = list_def.t();

        // Allocate space for the new element
        let element_layout = match element_shape.layout.sized_layout() {
            Ok(layout) => layout,
            Err(_) => {
                return Err(ReflectError::Unsized {
                    shape: element_shape,
                    operation: "begin_list_item: calculating element layout",
                });
            }
        };
        let element_ptr: *mut u8 = unsafe { ::alloc::alloc::alloc(element_layout) };

        let Some(element_ptr) = NonNull::new(element_ptr) else {
            return Err(ReflectError::OperationFailed {
                shape: frame.shape,
                operation: "failed to allocate memory for list element",
            });
        };

        // Push a new frame for the element
        self.frames_mut().push(Frame::new(
            PtrUninit::new(element_ptr.as_ptr()),
            element_shape,
            FrameOwnership::Owned,
        ));

        Ok(self)
    }
}
