use super::*;
use crate::AllocatedShape;

////////////////////////////////////////////////////////////////////////////////////////////////////
// Result
////////////////////////////////////////////////////////////////////////////////////////////////////
impl<const BORROW: bool> Partial<'_, BORROW> {
    /// Begin building the Ok variant of a Result
    pub fn begin_ok(mut self) -> Result<Self, ReflectError> {
        // Verify we're working with a Result and get the def
        let result_def = {
            let frame = self.frames().last().unwrap();
            match frame.allocated.shape().def {
                Def::Result(def) => def,
                _ => {
                    return Err(self.err(ReflectErrorKind::WasNotA {
                        expected: "Result",
                        actual: frame.allocated.shape(),
                    }));
                }
            }
        };

        // Check if we need to handle re-initialization.
        let needs_reinit = {
            let frame = self.frames().last().unwrap();
            frame.is_init
                || matches!(
                    frame.tracker,
                    Tracker::Result {
                        building_inner: false,
                        ..
                    }
                )
        };

        if needs_reinit {
            self.prepare_for_reinitialization();
        }

        // Set tracker to indicate we're building the Ok value
        // Get the type_plan before modifying tracker
        let parent_type_plan = self.frames().last().unwrap().type_plan;
        self.mode.stack_mut().last_mut().unwrap().tracker = Tracker::Result {
            is_ok: true,
            building_inner: true,
        };

        // Get the Ok type shape
        let inner_shape = result_def.t;

        // Allocate memory for the inner value
        let inner_layout = inner_shape.layout.sized_layout().map_err(|_| {
            self.err(ReflectErrorKind::Unsized {
                shape: inner_shape,
                operation: "begin_ok, allocating Result Ok value",
            })
        })?;

        let inner_data = if inner_layout.size() == 0 {
            // For ZST, use a non-null but unallocated pointer
            PtrUninit::new(NonNull::<u8>::dangling().as_ptr())
        } else {
            // Allocate memory for the inner value
            let ptr = unsafe { ::alloc::alloc::alloc(inner_layout) };
            let Some(ptr) = NonNull::new(ptr) else {
                ::alloc::alloc::handle_alloc_error(inner_layout);
            };
            PtrUninit::new(ptr.as_ptr())
        };

        // Create a new frame for the inner value
        // Get child type plan NodeId for Result Ok type
        let (ok_node_id, _err_node_id) = self
            .root_plan
            .result_nodes_id(parent_type_plan)
            .expect("TypePlan should have Result nodes");
        let inner_frame = Frame::new(
            inner_data,
            AllocatedShape::new(inner_shape, inner_layout.size()),
            FrameOwnership::Owned,
            ok_node_id,
        );
        self.mode.stack_mut().push(inner_frame);

        Ok(self)
    }

    /// Begin building the Err variant of a Result
    pub fn begin_err(mut self) -> Result<Self, ReflectError> {
        // Verify we're working with a Result and get the def
        let result_def = {
            let frame = self.frames().last().unwrap();
            match frame.allocated.shape().def {
                Def::Result(def) => def,
                _ => {
                    return Err(self.err(ReflectErrorKind::WasNotA {
                        expected: "Result",
                        actual: frame.allocated.shape(),
                    }));
                }
            }
        };

        // Check if we need to handle re-initialization.
        let needs_reinit = {
            let frame = self.frames().last().unwrap();
            frame.is_init
                || matches!(
                    frame.tracker,
                    Tracker::Result {
                        building_inner: false,
                        ..
                    }
                )
        };

        if needs_reinit {
            self.prepare_for_reinitialization();
        }

        // Set tracker to indicate we're building the Err value
        // Get the type_plan before modifying tracker
        let parent_type_plan = self.frames().last().unwrap().type_plan;
        self.mode.stack_mut().last_mut().unwrap().tracker = Tracker::Result {
            is_ok: false,
            building_inner: true,
        };

        // Get the Err type shape
        let inner_shape = result_def.e;

        // Allocate memory for the inner value
        let inner_layout = inner_shape.layout.sized_layout().map_err(|_| {
            self.err(ReflectErrorKind::Unsized {
                shape: inner_shape,
                operation: "begin_err, allocating Result Err value",
            })
        })?;

        let inner_data = if inner_layout.size() == 0 {
            // For ZST, use a non-null but unallocated pointer
            PtrUninit::new(NonNull::<u8>::dangling().as_ptr())
        } else {
            // Allocate memory for the inner value
            let ptr = unsafe { ::alloc::alloc::alloc(inner_layout) };
            let Some(ptr) = NonNull::new(ptr) else {
                ::alloc::alloc::handle_alloc_error(inner_layout);
            };
            PtrUninit::new(ptr.as_ptr())
        };

        // Create a new frame for the inner value
        // Get child type plan NodeId for Result Err type
        let (_ok_node_id, err_node_id) = self
            .root_plan
            .result_nodes_id(parent_type_plan)
            .expect("TypePlan should have Result nodes");
        let inner_frame = Frame::new(
            inner_data,
            AllocatedShape::new(inner_shape, inner_layout.size()),
            FrameOwnership::Owned,
            err_node_id,
        );
        self.mode.stack_mut().push(inner_frame);

        Ok(self)
    }
}
