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

        // Get shape upfront to avoid borrow conflicts
        let shape = self.frames().last().unwrap().allocated.shape();
        let frame = self.frames_mut().last_mut().unwrap();

        if pointee_shape.layout.sized_layout().is_ok() {
            // pointee is sized, we can allocate it — for `Arc<T>` we'll be allocating a `T` and
            // holding onto it. We'll build a new Arc with it when ending the smart pointer frame.

            frame.tracker = Tracker::SmartPointer;

            let inner_layout = match pointee_shape.layout.sized_layout() {
                Ok(layout) => layout,
                Err(_) => {
                    return Err(self.err(ReflectErrorKind::Unsized {
                        shape: pointee_shape,
                        operation: "begin_smart_ptr, calculating inner value layout",
                    }));
                }
            };
            let inner_ptr: *mut u8 = unsafe { ::alloc::alloc::alloc(inner_layout) };
            let Some(inner_ptr) = NonNull::new(inner_ptr) else {
                return Err(self.err(ReflectErrorKind::OperationFailed {
                    shape,
                    operation: "failed to allocate memory for smart pointer inner value",
                }));
            };

            // Push a new frame for the inner value
            // Get child type plan NodeId for smart pointer pointee
            let child_plan = frame
                .type_plan
                .and_then(|pn| self.root_plan.pointer_pointee_node(pn));
            self.frames_mut().push(Frame::new(
                PtrUninit::new(inner_ptr.as_ptr()),
                AllocatedShape::new(pointee_shape, inner_layout.size()),
                FrameOwnership::Owned,
                child_plan,
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
                let string_ptr: *mut u8 = unsafe { ::alloc::alloc::alloc(string_layout) };
                let Some(string_ptr) = NonNull::new(string_ptr) else {
                    return Err(self.err(ReflectErrorKind::OperationFailed {
                        shape,
                        operation: "failed to allocate memory for string",
                    }));
                };
                let string_size = string_layout.size();
                // For Arc<str> -> String conversion, use None since the shapes differ
                let new_frame = Frame::new(
                    PtrUninit::new(string_ptr.as_ptr()),
                    AllocatedShape::new(String::SHAPE, string_size),
                    FrameOwnership::Owned,
                    None,
                );
                // Frame::new already sets tracker = Scalar and is_init = false
                self.frames_mut().push(new_frame);
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
