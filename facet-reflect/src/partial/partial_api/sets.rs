use super::*;
use crate::AllocatedShape;

////////////////////////////////////////////////////////////////////////////////////////////////////
// Sets
////////////////////////////////////////////////////////////////////////////////////////////////////
impl<const BORROW: bool> Partial<'_, '_, BORROW> {
    /// Initializes a set (HashSet, BTreeSet, etc.) if it hasn't been initialized before.
    /// This is a prerequisite to `begin_set_item`/`set`/`end` or the shorthand `insert`.
    ///
    /// `init_set` does not clear the set if it was previously initialized.
    /// `init_set` does not push a new frame to the stack, and thus does not
    /// require `end` to be called afterwards.
    pub fn init_set(mut self) -> Result<Self, ReflectError> {
        crate::trace!("init_set()");
        // Get shape upfront to avoid borrow conflicts
        let shape = self.frames().last().unwrap().allocated.shape();
        let frame = self.frames_mut().last_mut().unwrap();

        match &frame.tracker {
            Tracker::Scalar if !frame.is_init => {
                // that's good, let's initialize it
            }
            Tracker::Scalar => {
                // is_init is true - initialized (perhaps from a previous round?) but should be a set tracker, let's fix that:
                frame.tracker = Tracker::Set {
                    current_child: false,
                };
                return Ok(self);
            }
            Tracker::Set { .. } => {
                if frame.is_init {
                    // already initialized, nothing to do
                    return Ok(self);
                }
            }
            _ => {
                let tracker_kind = frame.tracker.kind();
                return Err(self.err(ReflectErrorKind::UnexpectedTracker {
                    message: "init_set called but tracker isn't something set-like",
                    current_tracker: tracker_kind,
                }));
            }
        };

        // Check that we have a Set
        let set_def = match &shape.def {
            Def::Set(set_def) => set_def,
            _ => {
                return Err(self.err(ReflectErrorKind::OperationFailed {
                    shape,
                    operation: "init_set can only be called on Set types",
                }));
            }
        };

        let init_fn = set_def.vtable.init_in_place_with_capacity;

        // Initialize the set with default capacity (0)
        unsafe {
            init_fn(frame.data, 0);
        }

        // Update tracker to Set state and mark as initialized
        frame.tracker = Tracker::Set {
            current_child: false,
        };
        frame.is_init = true;

        Ok(self)
    }

    /// Begins pushing an element to the set.
    /// The element should be set using `set()` or similar methods, then `end()` to complete.
    pub fn begin_set_item(mut self) -> Result<Self, ReflectError> {
        crate::trace!("begin_set_item()");
        // Get frame info immutably first
        let (shape, parent_type_plan, set_def, is_init, has_current_child) = {
            let frame = self.frames().last().unwrap();
            let shape = frame.allocated.shape();
            let set_def = match &shape.def {
                Def::Set(set_def) => *set_def,
                _ => {
                    return Err(self.err(ReflectErrorKind::OperationFailed {
                        shape,
                        operation: "init_set_item can only be called on Set types",
                    }));
                }
            };
            let has_current_child = match &frame.tracker {
                Tracker::Set { current_child } => *current_child,
                _ => {
                    return Err(self.err(ReflectErrorKind::OperationFailed {
                        shape,
                        operation: "must call init_set() before begin_set_item()",
                    }));
                }
            };
            (
                shape,
                frame.type_plan,
                set_def,
                frame.is_init,
                has_current_child,
            )
        };

        // Verify the tracker is in Set state and initialized
        if !is_init {
            return Err(self.err(ReflectErrorKind::OperationFailed {
                shape,
                operation: "must call init_set() before begin_set_item()",
            }));
        }
        if has_current_child {
            return Err(self.err(ReflectErrorKind::OperationFailed {
                shape,
                operation: "already pushing an element, call end() first",
            }));
        }

        // Update tracker to indicate we're building a child
        match &mut self.mode.stack_mut().last_mut().unwrap().tracker {
            Tracker::Set { current_child } => *current_child = true,
            _ => unreachable!(),
        }

        // Get the element shape
        let element_shape = set_def.t();

        // Allocate space for the new element
        let element_layout = match element_shape.layout.sized_layout() {
            Ok(layout) => layout,
            Err(_) => {
                return Err(self.err(ReflectErrorKind::Unsized {
                    shape: element_shape,
                    operation: "begin_set_item: calculating element layout",
                }));
            }
        };
        let element_ptr: *mut u8 = unsafe { ::alloc::alloc::alloc(element_layout) };

        let Some(element_ptr) = NonNull::new(element_ptr) else {
            return Err(self.err(ReflectErrorKind::OperationFailed {
                shape,
                operation: "failed to allocate memory for set element",
            }));
        };

        // Push a new frame for the element
        // Get child type plan NodeId for set items
        let child_plan_id = self
            .root_plan
            .set_item_node_id(parent_type_plan)
            .expect("TypePlan should have item node for Set");
        self.mode.stack_mut().push(Frame::new(
            PtrUninit::new(element_ptr.as_ptr()),
            AllocatedShape::new(element_shape, element_layout.size()),
            FrameOwnership::Owned,
            child_plan_id,
        ));

        Ok(self)
    }
}
