use super::*;
use crate::AllocatedShape;

////////////////////////////////////////////////////////////////////////////////////////////////////
// Sets
////////////////////////////////////////////////////////////////////////////////////////////////////
impl<const BORROW: bool> Partial<'_, BORROW> {
    /// Initializes a set (HashSet, BTreeSet, etc.) if it hasn't been initialized before.
    /// This is a prerequisite to `begin_set_item`/`set`/`end` or the shorthand `insert`.
    ///
    /// `init_set` does not clear the set if it was previously initialized.
    /// `init_set` does not push a new frame to the stack, and thus does not
    /// require `end` to be called afterwards.
    pub fn init_set(mut self) -> Result<Self, ReflectError> {
        crate::trace!("init_set()");
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
                return Err(ReflectError::UnexpectedTracker {
                    message: "init_set called but tracker isn't something set-like",
                    current_tracker: frame.tracker.kind(),
                });
            }
        };

        // Check that we have a Set
        let set_def = match &frame.allocated.shape().def {
            Def::Set(set_def) => set_def,
            _ => {
                return Err(ReflectError::OperationFailed {
                    shape: frame.allocated.shape(),
                    operation: "init_set can only be called on Set types",
                });
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
        let frame = self.frames_mut().last_mut().unwrap();

        // Check that we have a Set that's been initialized
        let set_def = match &frame.allocated.shape().def {
            Def::Set(set_def) => set_def,
            _ => {
                return Err(ReflectError::OperationFailed {
                    shape: frame.allocated.shape(),
                    operation: "init_set_item can only be called on Set types",
                });
            }
        };

        // Verify the tracker is in Set state and initialized
        match &mut frame.tracker {
            Tracker::Set { current_child } if frame.is_init => {
                if *current_child {
                    return Err(ReflectError::OperationFailed {
                        shape: frame.allocated.shape(),
                        operation: "already pushing an element, call end() first",
                    });
                }
                *current_child = true;
            }
            _ => {
                return Err(ReflectError::OperationFailed {
                    shape: frame.allocated.shape(),
                    operation: "must call init_set() before begin_set_item()",
                });
            }
        }

        // Get the element shape
        let element_shape = set_def.t();

        // Allocate space for the new element
        let element_layout = match element_shape.layout.sized_layout() {
            Ok(layout) => layout,
            Err(_) => {
                return Err(ReflectError::Unsized {
                    shape: element_shape,
                    operation: "begin_set_item: calculating element layout",
                });
            }
        };
        let element_ptr: *mut u8 = unsafe { ::alloc::alloc::alloc(element_layout) };

        let Some(element_ptr) = NonNull::new(element_ptr) else {
            return Err(ReflectError::OperationFailed {
                shape: frame.allocated.shape(),
                operation: "failed to allocate memory for set element",
            });
        };

        // Push a new frame for the element
        self.frames_mut().push(Frame::new(
            PtrUninit::new(element_ptr.as_ptr()),
            AllocatedShape::new(element_shape, element_layout.size()),
            FrameOwnership::Owned,
        ));

        Ok(self)
    }
}
