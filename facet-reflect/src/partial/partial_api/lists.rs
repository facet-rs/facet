use super::*;

////////////////////////////////////////////////////////////////////////////////////////////////////
// Lists
////////////////////////////////////////////////////////////////////////////////////////////////////
impl Partial<'_> {
    /// Initializes a list (Vec, etc.) if it hasn't been initialized before.
    /// This is a prerequisite to `begin_push_item`/`set`/`end` or the shorthand
    /// `push`.
    ///
    /// `begin_list` does not clear the list if it was previously initialized.
    /// `begin_list` does not push a new frame to the stack, and thus does not
    /// require `end` to be called afterwards.
    pub fn begin_list(&mut self) -> Result<&mut Self, ReflectError> {
        crate::trace!("begin_list()");
        self.require_active()?;
        let frame = self.frames.last_mut().unwrap();

        match &frame.tracker {
            Tracker::Uninit => {
                // that's good, let's initialize it
            }
            Tracker::Init => {
                // initialized (perhaps from a previous round?) but should be a list tracker
                // First verify this is actually a list type before changing tracker
                if !matches!(frame.shape.def, Def::List(_)) {
                    return Err(ReflectError::OperationFailed {
                        shape: frame.shape,
                        operation: "begin_list can only be called on List types",
                    });
                }
                frame.tracker = Tracker::List {
                    is_initialized: true,
                    current_child: false,
                };
                return Ok(self);
            }
            Tracker::List { is_initialized, .. } => {
                if *is_initialized {
                    // already initialized, nothing to do
                    return Ok(self);
                }
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

        // Check that we have a List
        let list_def = match &frame.shape.def {
            Def::List(list_def) => list_def,
            _ => {
                return Err(ReflectError::OperationFailed {
                    shape: frame.shape,
                    operation: "begin_list can only be called on List types",
                });
            }
        };

        // Check that we have init_in_place_with_capacity function
        let init_fn = match list_def.vtable.init_in_place_with_capacity {
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

        // Update tracker to List state
        frame.tracker = Tracker::List {
            is_initialized: true,
            current_child: false,
        };

        Ok(self)
    }

    /// Pushes an element to the list
    /// The element should be set using `set()` or similar methods, then `pop()` to complete
    pub fn begin_list_item(&mut self) -> Result<&mut Self, ReflectError> {
        crate::trace!("begin_list_item()");
        self.require_active()?;
        let frame = self.frames.last_mut().unwrap();

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
                PtrUninit::new(element_ptr),
                element_shape,
                FrameOwnership::Owned,
            );
            self.frames.push(element_frame);

            // Mark that we're building an item
            // We need to update the tracker after pushing the frame
            let parent_idx = self.frames.len() - 2;
            if let Tracker::SmartPointerSlice { building_item, .. } =
                &mut self.frames[parent_idx].tracker
            {
                crate::trace!("Marking element frame as building item");
                *building_item = true;
            }

            return Ok(self);
        }

        // Check that we have a List that's been initialized
        let list_def = match &frame.shape.def {
            Def::List(list_def) => list_def,
            _ => {
                return Err(ReflectError::OperationFailed {
                    shape: frame.shape,
                    operation: "push can only be called on List types",
                });
            }
        };

        // Verify the tracker is in List state and initialized
        match &mut frame.tracker {
            Tracker::List {
                is_initialized: true,
                current_child,
            } => {
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
        self.frames.push(Frame::new(
            PtrUninit::new(element_ptr),
            element_shape,
            FrameOwnership::Owned,
        ));

        Ok(self)
    }
}
