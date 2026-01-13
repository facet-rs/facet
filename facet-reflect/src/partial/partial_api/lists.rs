use super::*;
use crate::AllocatedShape;

////////////////////////////////////////////////////////////////////////////////////////////////////
// Lists
////////////////////////////////////////////////////////////////////////////////////////////////////
impl<const BORROW: bool> Partial<'_, BORROW> {
    /// Initializes a list (Vec, etc.) if it hasn't been initialized before.
    /// This is a prerequisite to `begin_push_item`/`set`/`end` or the shorthand
    /// `push`.
    ///
    /// `init_list` does not clear the list if it was previously initialized.
    /// `init_list` does not push a new frame to the stack, and thus does not
    /// require `end` to be called afterwards.
    pub fn init_list(mut self) -> Result<Self, ReflectError> {
        crate::trace!("init_list()");
        let frame = self.frames_mut().last_mut().unwrap();

        match &frame.tracker {
            Tracker::Scalar if !frame.is_init => {
                // that's good, let's initialize it
            }
            Tracker::Scalar => {
                // is_init is true - initialized (perhaps from a previous round?) but should be a list tracker
                // Check what kind of shape we have
                match &frame.allocated.shape().def {
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
                            shape: frame.allocated.shape(),
                            operation: "init_list can only be called on List types or DynamicValue",
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
                // Must use deinit_for_replace() since we're about to overwrite with a new Array.
                // This is important for BorrowedInPlace frames where deinit() would early-return
                // without dropping the existing value.
                frame.deinit_for_replace();
            }
            Tracker::SmartPointerSlice { .. } => {
                // init_list is kinda superfluous when we're in a SmartPointerSlice state
                return Ok(self);
            }
            _ => {
                return Err(ReflectError::UnexpectedTracker {
                    message: "init_list called but tracker isn't something list-like",
                    current_tracker: frame.tracker.kind(),
                });
            }
        };

        // Check that we have a List or DynamicValue
        match &frame.allocated.shape().def {
            Def::List(list_def) => {
                // Check that we have init_in_place_with_capacity function
                let init_fn = match list_def.init_in_place_with_capacity() {
                    Some(f) => f,
                    None => {
                        return Err(ReflectError::OperationFailed {
                            shape: frame.allocated.shape(),
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
                    shape: frame.allocated.shape(),
                    operation: "init_list can only be called on List or DynamicValue types",
                });
            }
        }

        Ok(self)
    }

    /// Transitions the frame to Array tracker state.
    ///
    /// This is used to prepare a fixed-size array for element initialization.
    /// Unlike `init_list`, this does not initialize any runtime data - arrays
    /// are stored inline and don't need a vtable call.
    ///
    /// This method is particularly important for zero-length arrays like `[u8; 0]`,
    /// which have no elements to initialize but still need their tracker state
    /// to be set correctly for `require_full_initialization` to pass.
    ///
    /// `init_array` does not push a new frame to the stack.
    pub fn init_array(mut self) -> Result<Self, ReflectError> {
        crate::trace!("init_array()");
        let frame = self.frames_mut().last_mut().unwrap();

        // Verify this is an array type
        let array_def = match &frame.allocated.shape().def {
            Def::Array(array_def) => array_def,
            _ => {
                return Err(ReflectError::OperationFailed {
                    shape: frame.allocated.shape(),
                    operation: "init_array can only be called on Array types",
                });
            }
        };

        // Check array size limit
        if array_def.n > 63 {
            return Err(ReflectError::OperationFailed {
                shape: frame.allocated.shape(),
                operation: "arrays larger than 63 elements are not yet supported",
            });
        }

        match &frame.tracker {
            Tracker::Scalar if !frame.is_init => {
                // Transition to Array tracker
                frame.tracker = Tracker::Array {
                    iset: ISet::default(),
                    current_child: None,
                };
            }
            Tracker::Array { .. } => {
                // Already in Array state, nothing to do
            }
            _ => {
                return Err(ReflectError::OperationFailed {
                    shape: frame.allocated.shape(),
                    operation: "init_array: unexpected tracker state",
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
                    shape: frame.allocated.shape(),
                    operation: "already building an item, call end() first",
                });
            }

            // Get the element type from the smart pointer's pointee
            let element_shape = match &frame.allocated.shape().def {
                Def::Pointer(smart_ptr_def) => match smart_ptr_def.pointee() {
                    Some(pointee_shape) => match &pointee_shape.ty {
                        Type::Sequence(SequenceType::Slice(slice_type)) => slice_type.t,
                        _ => {
                            return Err(ReflectError::OperationFailed {
                                shape: frame.allocated.shape(),
                                operation: "smart pointer pointee is not a slice",
                            });
                        }
                    },
                    None => {
                        return Err(ReflectError::OperationFailed {
                            shape: frame.allocated.shape(),
                            operation: "smart pointer has no pointee",
                        });
                    }
                },
                _ => {
                    return Err(ReflectError::OperationFailed {
                        shape: frame.allocated.shape(),
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

            let element_data = if element_layout.size() == 0 {
                // For ZST, use a non-null but unallocated pointer
                PtrUninit::new(NonNull::<u8>::dangling().as_ptr())
            } else {
                let element_ptr: *mut u8 = unsafe { ::alloc::alloc::alloc(element_layout) };
                let Some(element_ptr) = NonNull::new(element_ptr) else {
                    return Err(ReflectError::OperationFailed {
                        shape: frame.allocated.shape(),
                        operation: "failed to allocate memory for list element",
                    });
                };
                PtrUninit::new(element_ptr.as_ptr())
            };

            // Create and push the element frame
            crate::trace!("Pushing element frame, which we just allocated");
            let element_frame = Frame::new(
                element_data,
                AllocatedShape::new(element_shape, element_layout.size()),
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
                    shape: frame.allocated.shape(),
                    operation: "already building an element, call end() first",
                });
            }

            // For DynamicValue arrays, the element shape is the same DynamicValue shape
            // (Value arrays contain Value elements)
            let element_shape = frame.allocated.shape();
            let element_layout = match element_shape.layout.sized_layout() {
                Ok(layout) => layout,
                Err(_) => {
                    return Err(ReflectError::Unsized {
                        shape: element_shape,
                        operation: "begin_list_item: calculating element layout",
                    });
                }
            };

            let element_data = if element_layout.size() == 0 {
                // For ZST, use a non-null but unallocated pointer
                PtrUninit::new(NonNull::<u8>::dangling().as_ptr())
            } else {
                let element_ptr: *mut u8 = unsafe { ::alloc::alloc::alloc(element_layout) };
                let Some(element_ptr) = NonNull::new(element_ptr) else {
                    return Err(ReflectError::OperationFailed {
                        shape: frame.allocated.shape(),
                        operation: "failed to allocate memory for list element",
                    });
                };
                PtrUninit::new(element_ptr.as_ptr())
            };

            // Push a new frame for the element
            self.frames_mut().push(Frame::new(
                element_data,
                AllocatedShape::new(element_shape, element_layout.size()),
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
        let list_def = match &frame.allocated.shape().def {
            Def::List(list_def) => list_def,
            _ => {
                return Err(ReflectError::OperationFailed {
                    shape: frame.allocated.shape(),
                    operation: "push can only be called on List or DynamicValue types",
                });
            }
        };

        // Verify the tracker is in List state and initialized
        match &mut frame.tracker {
            Tracker::List { current_child } if frame.is_init => {
                if *current_child {
                    return Err(ReflectError::OperationFailed {
                        shape: frame.allocated.shape(),
                        operation: "already pushing an element, call pop() first",
                    });
                }
                *current_child = true;
            }
            _ => {
                return Err(ReflectError::OperationFailed {
                    shape: frame.allocated.shape(),
                    operation: "must call init_list() before push()",
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
        let element_data = if element_layout.size() == 0 {
            // For ZST, use a non-null but unallocated pointer
            PtrUninit::new(NonNull::<u8>::dangling().as_ptr())
        } else {
            let element_ptr: *mut u8 = unsafe { ::alloc::alloc::alloc(element_layout) };
            let Some(element_ptr) = NonNull::new(element_ptr) else {
                return Err(ReflectError::OperationFailed {
                    shape: frame.allocated.shape(),
                    operation: "failed to allocate memory for list element",
                });
            };
            PtrUninit::new(element_ptr.as_ptr())
        };

        // Push a new frame for the element
        self.frames_mut().push(Frame::new(
            element_data,
            AllocatedShape::new(element_shape, element_layout.size()),
            FrameOwnership::Owned,
        ));

        Ok(self)
    }
}
