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
    pub fn init_list(self) -> Result<Self, ReflectError> {
        self.init_list_with_capacity(0)
    }

    /// Initializes a list with a capacity hint for pre-allocation.
    ///
    /// Like `init_list`, but reserves space for `capacity` elements upfront.
    /// This reduces allocations when the number of elements is known or estimated.
    pub fn init_list_with_capacity(mut self, capacity: usize) -> Result<Self, ReflectError> {
        crate::trace!("init_list_with_capacity({capacity})");

        // Get shape upfront to avoid borrow conflicts
        let shape = self.frames().last().unwrap().allocated.shape();
        let frame = self.frames_mut().last_mut().unwrap();

        match &frame.tracker {
            Tracker::Scalar if !frame.is_init => {
                // that's good, let's initialize it
            }
            Tracker::Scalar => {
                // is_init is true - initialized (perhaps from a previous round?) but should be a list tracker
                // Check what kind of shape we have
                match &shape.def {
                    Def::List(_) => {
                        // Regular list type - just update the tracker
                        frame.tracker = Tracker::List {
                            current_child: None,
                            rope: None,
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
                        return Err(self.err(ReflectErrorKind::OperationFailed {
                            shape,
                            operation: "init_list can only be called on List types or DynamicValue",
                        }));
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
                let tracker_kind = frame.tracker.kind();
                return Err(self.err(ReflectErrorKind::UnexpectedTracker {
                    message: "init_list called but tracker isn't something list-like",
                    current_tracker: tracker_kind,
                }));
            }
        };

        // Check that we have a List or DynamicValue
        match &shape.def {
            Def::List(list_def) => {
                // Check that we have init_in_place_with_capacity function
                let init_fn = match list_def.init_in_place_with_capacity() {
                    Some(f) => f,
                    None => {
                        return Err(self.err(ReflectErrorKind::OperationFailed {
                            shape,
                            operation: "list type does not support initialization with capacity",
                        }));
                    }
                };

                // Initialize the list with the given capacity
                // Need to re-borrow frame after the early returns above
                let frame = self.frames_mut().last_mut().unwrap();
                unsafe {
                    init_fn(frame.data, capacity);
                }

                // Update tracker to List state and mark as initialized
                frame.tracker = Tracker::List {
                    current_child: None,
                    rope: None,
                };
                frame.is_init = true;
            }
            Def::DynamicValue(dyn_def) => {
                // Initialize as a dynamic array
                // Need to re-borrow frame after the early returns above
                let frame = self.frames_mut().last_mut().unwrap();
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
                return Err(self.err(ReflectErrorKind::OperationFailed {
                    shape,
                    operation: "init_list can only be called on List or DynamicValue types",
                }));
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

        // Get shape upfront to avoid borrow conflicts
        let shape = self.frames().last().unwrap().allocated.shape();

        // Verify this is an array type
        let array_def = match &shape.def {
            Def::Array(array_def) => array_def,
            _ => {
                return Err(self.err(ReflectErrorKind::OperationFailed {
                    shape,
                    operation: "init_array can only be called on Array types",
                }));
            }
        };

        // Check array size limit
        if array_def.n > 63 {
            return Err(self.err(ReflectErrorKind::OperationFailed {
                shape,
                operation: "arrays larger than 63 elements are not yet supported",
            }));
        }

        let frame = self.frames_mut().last_mut().unwrap();
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
                return Err(self.err(ReflectErrorKind::OperationFailed {
                    shape,
                    operation: "init_array: unexpected tracker state",
                }));
            }
        }

        Ok(self)
    }

    /// Pushes an element to the list
    /// The element should be set using `set()` or similar methods, then `pop()` to complete
    pub fn begin_list_item(mut self) -> Result<Self, ReflectError> {
        crate::trace!("begin_list_item()");

        // Get immutable data upfront - shape and type_plan are Copy
        let frame = self.frames().last().unwrap();
        let shape = frame.allocated.shape();
        let parent_type_plan = frame.type_plan;

        // Check tracker state immutably first to determine which path to take
        let tracker_kind = frame.tracker.kind();

        // Check if we're building a smart pointer slice
        if tracker_kind == crate::error::TrackerKind::SmartPointerSlice {
            let frame = self.mode.stack_mut().last_mut().unwrap();
            if let Tracker::SmartPointerSlice {
                building_item: true,
                vtable: _,
            } = &frame.tracker
            {
                return Err(self.err(ReflectErrorKind::OperationFailed {
                    shape,
                    operation: "already building an item, call end() first",
                }));
            }

            // Get the element type from the smart pointer's pointee
            let element_shape = match &shape.def {
                Def::Pointer(smart_ptr_def) => match smart_ptr_def.pointee() {
                    Some(pointee_shape) => match &pointee_shape.ty {
                        Type::Sequence(SequenceType::Slice(slice_type)) => slice_type.t,
                        _ => {
                            return Err(self.err(ReflectErrorKind::OperationFailed {
                                shape,
                                operation: "smart pointer pointee is not a slice",
                            }));
                        }
                    },
                    None => {
                        return Err(self.err(ReflectErrorKind::OperationFailed {
                            shape,
                            operation: "smart pointer has no pointee",
                        }));
                    }
                },
                _ => {
                    return Err(self.err(ReflectErrorKind::OperationFailed {
                        shape,
                        operation: "expected smart pointer definition",
                    }));
                }
            };

            // Allocate space for the element
            crate::trace!("Pointee is a slice of {element_shape}");
            let element_layout = match element_shape.layout.sized_layout() {
                Ok(layout) => layout,
                Err(_) => {
                    return Err(self.err(ReflectErrorKind::OperationFailed {
                        shape: element_shape,
                        operation: "cannot allocate unsized element",
                    }));
                }
            };

            let element_data = if element_layout.size() == 0 {
                // For ZST, use a non-null but unallocated pointer
                PtrUninit::new(NonNull::<u8>::dangling().as_ptr())
            } else {
                let element_ptr: *mut u8 = unsafe { ::alloc::alloc::alloc(element_layout) };
                let Some(element_ptr) = NonNull::new(element_ptr) else {
                    return Err(self.err(ReflectErrorKind::OperationFailed {
                        shape,
                        operation: "failed to allocate memory for list element",
                    }));
                };
                PtrUninit::new(element_ptr.as_ptr())
            };

            // Get child type plan NodeId for slice items
            // Navigate: Pointer -> Slice -> element
            let slice_node_id = self
                .root_plan
                .pointer_inner_node_id(parent_type_plan)
                .expect("TypePlan must have slice node for smart pointer");
            let child_plan_id = self
                .root_plan
                .list_item_node_id(slice_node_id)
                .expect("TypePlan must have item node for slice");

            // Create and push the element frame
            crate::trace!("Pushing element frame, which we just allocated");
            let element_frame = Frame::new(
                element_data,
                AllocatedShape::new(element_shape, element_layout.size()),
                FrameOwnership::Owned,
                child_plan_id,
            );
            self.mode.stack_mut().push(element_frame);

            // Mark that we're building an item
            let parent_idx = self.mode.stack().len() - 2;
            if let Tracker::SmartPointerSlice { building_item, .. } =
                &mut self.mode.stack_mut()[parent_idx].tracker
            {
                crate::trace!("Marking element frame as building item");
                *building_item = true;
            }

            return Ok(self);
        }

        // Check if we're building a DynamicValue array
        if tracker_kind == crate::error::TrackerKind::DynamicValue {
            let frame = self.mode.stack().last().unwrap();
            if let Tracker::DynamicValue {
                state: DynamicValueState::Array { building_element },
            } = &frame.tracker
            {
                if *building_element {
                    return Err(self.err(ReflectErrorKind::OperationFailed {
                        shape,
                        operation: "already building an element, call end() first",
                    }));
                }

                // For DynamicValue arrays, the element shape is the same DynamicValue shape
                // (Value arrays contain Value elements)
                let element_shape = shape;
                let element_layout = match element_shape.layout.sized_layout() {
                    Ok(layout) => layout,
                    Err(_) => {
                        return Err(self.err(ReflectErrorKind::Unsized {
                            shape: element_shape,
                            operation: "begin_list_item: calculating element layout",
                        }));
                    }
                };

                let element_data = if element_layout.size() == 0 {
                    // For ZST, use a non-null but unallocated pointer
                    PtrUninit::new(NonNull::<u8>::dangling().as_ptr())
                } else {
                    let element_ptr: *mut u8 = unsafe { ::alloc::alloc::alloc(element_layout) };
                    let Some(element_ptr) = NonNull::new(element_ptr) else {
                        return Err(self.err(ReflectErrorKind::OperationFailed {
                            shape,
                            operation: "failed to allocate memory for list element",
                        }));
                    };
                    PtrUninit::new(element_ptr.as_ptr())
                };

                // For DynamicValue arrays, use the same type plan (self-recursive)
                let child_plan = parent_type_plan;
                self.mode.stack_mut().push(Frame::new(
                    element_data,
                    AllocatedShape::new(element_shape, element_layout.size()),
                    FrameOwnership::Owned,
                    child_plan,
                ));

                // Mark that we're building an element
                let parent_idx = self.mode.stack().len() - 2;
                if let Tracker::DynamicValue {
                    state: DynamicValueState::Array { building_element },
                } = &mut self.mode.stack_mut()[parent_idx].tracker
                {
                    *building_element = true;
                }

                return Ok(self);
            }
        }

        // Check that we have a List that's been initialized
        let list_def = match &shape.def {
            Def::List(list_def) => list_def,
            _ => {
                return Err(self.err(ReflectErrorKind::OperationFailed {
                    shape,
                    operation: "push can only be called on List or DynamicValue types",
                }));
            }
        };

        // Verify the tracker is in List state and initialized
        {
            let frame = self.mode.stack_mut().last_mut().unwrap();
            match &mut frame.tracker {
                Tracker::List {
                    current_child,
                    rope,
                    ..
                } if frame.is_init => {
                    if current_child.is_some() {
                        return Err(self.err(ReflectErrorKind::OperationFailed {
                            shape,
                            operation: "already pushing an element, call pop() first",
                        }));
                    }
                    // Get the current length to use as the index for path tracking.
                    // With rope-based storage, elements are stored in the rope until the list
                    // is finalized, so we use the rope's length if available.
                    let current_len = if let Some(rope) = rope {
                        rope.len()
                    } else {
                        unsafe { (list_def.vtable.len)(frame.data.assume_init().as_const()) }
                    };
                    *current_child = Some(current_len);
                }
                _ => {
                    return Err(self.err(ReflectErrorKind::OperationFailed {
                        shape,
                        operation: "must call init_list() before push()",
                    }));
                }
            }
        }

        // Get the element shape
        let element_shape = list_def.t();

        // Calculate element layout
        let element_layout = match element_shape.layout.sized_layout() {
            Ok(layout) => layout,
            Err(_) => {
                return Err(self.err(ReflectErrorKind::Unsized {
                    shape: element_shape,
                    operation: "begin_list_item: calculating element layout",
                }));
            }
        };

        // Allocate element in a stable rope chunk.
        // Rope chunks never move, so frames can be stored for deferred processing.
        let (element_data, ownership) = {
            let frame = self.mode.stack_mut().last_mut().unwrap();
            if element_layout.size() == 0 {
                // ZST: use dangling pointer, no allocation needed
                (
                    PtrUninit::new(NonNull::<u8>::dangling().as_ptr()),
                    FrameOwnership::Owned,
                )
            } else {
                // Initialize rope lazily on first element
                let Tracker::List { rope, .. } = &mut frame.tracker else {
                    unreachable!("already verified List tracker above");
                };
                if rope.is_none() {
                    *rope = Some(ListRope::new(element_layout));
                }
                let rope = rope.as_mut().unwrap();

                // Allocate from rope - this returns a stable pointer that won't move
                let element_ptr = rope.push_uninit();
                (
                    PtrUninit::new(element_ptr.as_ptr()),
                    FrameOwnership::RopeSlot,
                )
            }
        };

        // Get child type plan NodeId for list items - root_plan is free to access now
        let child_plan_id = self
            .root_plan
            .list_item_node_id(parent_type_plan)
            .expect("TypePlan must have list item node");

        // Push a new frame for the element
        self.mode.stack_mut().push(Frame::new(
            element_data,
            AllocatedShape::new(element_shape, element_layout.size()),
            ownership,
            child_plan_id,
        ));

        Ok(self)
    }
}
