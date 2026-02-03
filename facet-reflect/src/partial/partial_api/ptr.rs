use super::*;
use crate::AllocatedShape;

////////////////////////////////////////////////////////////////////////////////////////////////////
// Smart pointers
////////////////////////////////////////////////////////////////////////////////////////////////////
impl<const BORROW: bool> Partial<'_, BORROW> {
    /// Pushes a frame to initialize the inner value of a smart pointer (`Box<T>`, `Arc<T>`, etc.)
    pub fn begin_smart_ptr(mut self) -> Result<Self, ReflectError> {
        crate::trace!("begin_smart_ptr()");

        // Check that we have a SmartPointer and get necessary data
        let (smart_ptr_def, pointee_shape) = {
            let frame = self.frames().last().unwrap();

            match &frame.allocated.shape().def {
                Def::Pointer(smart_ptr_def) if smart_ptr_def.constructible_from_pointee() => {
                    let pointee_shape = match smart_ptr_def.pointee() {
                        Some(shape) => shape,
                        None => {
                            return Err(self.err(ReflectErrorKind::OperationFailed {
                                shape: frame.allocated.shape(),
                                operation: "Smart pointer must have a pointee shape",
                            }));
                        }
                    };
                    (*smart_ptr_def, pointee_shape)
                }
                _ => {
                    return Err(self.err(ReflectErrorKind::OperationFailed {
                        shape: frame.allocated.shape(),
                        operation: "push_smart_ptr can only be called on compatible types",
                    }));
                }
            }
        };

        // Handle re-initialization if the smart pointer is already initialized
        self.prepare_for_reinitialization();

        // Get shape and type_plan upfront to avoid borrow conflicts
        let shape = self.frames().last().unwrap().allocated.shape();
        let parent_type_plan = self.frames().last().unwrap().type_plan;

        if pointee_shape.layout.sized_layout().is_ok() {
            // pointee is sized, we can allocate it — for `Arc<T>` we'll be allocating a `T` and
            // holding onto it. We'll build a new Arc with it when ending the smart pointer frame.

            self.mode.stack_mut().last_mut().unwrap().tracker = Tracker::SmartPointer;

            let inner_layout = match pointee_shape.layout.sized_layout() {
                Ok(layout) => layout,
                Err(_) => {
                    return Err(self.err(ReflectErrorKind::Unsized {
                        shape: pointee_shape,
                        operation: "begin_smart_ptr, calculating inner value layout",
                    }));
                }
            };
            let inner_ptr = facet_core::alloc_for_layout(inner_layout);

            // Push a new frame for the inner value
            // Get child type plan NodeId for smart pointer pointee
            let child_plan_id = self
                .root_plan
                .pointer_inner_node_id(parent_type_plan)
                .expect("TypePlan should have pointee node for sized pointer");
            self.mode.stack_mut().push(Frame::new(
                inner_ptr,
                AllocatedShape::new(pointee_shape, inner_layout.size()),
                FrameOwnership::Owned,
                child_plan_id,
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
                let string_ptr = facet_core::alloc_for_layout(string_layout);
                let string_size = string_layout.size();
                // For Arc<str> -> String conversion, TypePlan builds for the conversion source (String)
                let child_plan_id = self
                    .root_plan
                    .pointer_inner_node_id(parent_type_plan)
                    .expect("TypePlan should have pointee node for str->String conversion");
                let new_frame = Frame::new(
                    string_ptr,
                    AllocatedShape::new(String::SHAPE, string_size),
                    FrameOwnership::Owned,
                    child_plan_id,
                );
                // Frame::new already sets tracker = Scalar and is_init = false
                self.mode.stack_mut().push(new_frame);
            } else if let Type::Sequence(SequenceType::Slice(_st)) = pointee_shape.ty {
                crate::trace!("Pointee is [{}]", _st.t);

                // Get the slice builder vtable
                let slice_builder_vtable = match smart_ptr_def.vtable.slice_builder_vtable {
                    Some(vtable) => vtable,
                    None => {
                        return Err(self.err(ReflectErrorKind::OperationFailed {
                            shape,
                            operation: "smart pointer does not support slice building",
                        }));
                    }
                };

                // Create a new builder
                let builder_ptr = (slice_builder_vtable.new_fn)();

                // Deallocate the original Arc allocation before replacing with slice builder
                let frame = self.mode.stack_mut().last_mut().unwrap();
                if let FrameOwnership::Owned = frame.ownership
                    && let Ok(layout) = shape.layout.sized_layout()
                    && layout.size() > 0
                {
                    unsafe { ::alloc::alloc::dealloc(frame.data.as_mut_byte_ptr(), layout) };
                }

                // Update the current frame to use the slice builder
                frame.data = builder_ptr.as_uninit();
                frame.tracker = Tracker::SmartPointerSlice {
                    vtable: slice_builder_vtable,
                    building_item: false,
                };
                // Keep the original ownership (e.g., Field) so parent tracking works correctly.
                // The slice builder memory itself is managed by the vtable's convert_fn/free_fn.
            } else {
                return Err(self.err(ReflectErrorKind::OperationFailed {
                    shape,
                    operation: "push_smart_ptr can only be called on pointers to supported pointee types",
                }));
            }
        }

        Ok(self)
    }
}
