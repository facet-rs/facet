use super::*;
use crate::AllocatedShape;

////////////////////////////////////////////////////////////////////////////////////////////////////
// Option / inner
////////////////////////////////////////////////////////////////////////////////////////////////////
impl<const BORROW: bool> Partial<'_, BORROW> {
    /// Begin building the Some variant of an Option
    pub fn begin_some(mut self) -> Result<Self, ReflectError> {
        // Verify we're working with an Option and get the def
        let option_def = {
            let frame = self.frames().last().unwrap();
            match frame.allocated.shape().def {
                Def::Option(def) => def,
                _ => {
                    return Err(self.err(ReflectErrorKind::WasNotA {
                        expected: "Option",
                        actual: frame.allocated.shape(),
                    }));
                }
            }
        };

        // Check if we need to handle re-initialization.
        // For Options, also check if tracker is Option{building_inner:false} which means
        // a previous begin_some/end cycle completed.
        //
        // IMPORTANT: For Option<Vec<T>> and similar accumulator types, we do NOT want to
        // reinitialize when re-entering, as this would destroy the existing Vec.
        // This can happen with TOML array-of-tables which emit multiple FieldKey events
        // for the same field.
        let needs_reinit = {
            let frame = self.frames().last().unwrap();

            // Check if this is a re-entry into an already-initialized Option.
            // After end() completes, the tracker is reset to Scalar, not Option{building_inner: false}.
            // So we check for Scalar tracker + is_init flag.
            if matches!(frame.tracker, Tracker::Scalar) && frame.is_init {
                // The Option was previously built and completed.
                // Check if the inner type can accumulate more values (like List, Map, DynamicValue)
                let inner_shape = option_def.t;
                let is_accumulator = matches!(
                    inner_shape.def,
                    Def::List(_) | Def::Map(_) | Def::Set(_) | Def::DynamicValue(_)
                );

                if is_accumulator {
                    // Don't reinitialize - we'll re-enter the existing inner value below
                    false
                } else {
                    // For scalars and other types, reinitialize as before
                    true
                }
            } else {
                frame.is_init
            }
        };

        // Check if we're re-entering an existing accumulator (like Option<Vec<T>>)
        let is_reentry = {
            let frame = self.frames().last().unwrap();
            matches!(frame.tracker, Tracker::Scalar) && frame.is_init && !needs_reinit
        };

        if needs_reinit {
            self.prepare_for_reinitialization();
        }

        // In deferred mode, push "Some" onto the path to distinguish
        // Option<T> (path ends before "Some") from the inner T (path includes "Some").
        // This treats Option like an enum with Some/None variants for path tracking.
        if let FrameMode::Deferred {
            stack,
            start_depth,
            current_path,
            stored_frames,
            ..
        } = &mut self.mode
        {
            let relative_depth = stack.len() - *start_depth;
            let should_track = current_path.len() == relative_depth;

            if should_track {
                current_path.push("Some");

                // Check if we have a stored frame for this path (re-entry case)
                if let Some(stored_frame) = stored_frames.remove(current_path) {
                    trace!("begin_some: Restoring stored frame for path {current_path:?}");

                    // Update tracker to indicate we're building the inner value
                    let frame = stack.last_mut().unwrap();
                    frame.tracker = Tracker::Option {
                        building_inner: true,
                    };

                    stack.push(stored_frame);
                    return Ok(self);
                }
            }
        }

        // Set tracker to indicate we're building the inner value
        // Copy the type_plan pointer before dropping the mutable borrow
        let parent_type_plan = {
            let frame = self.frames_mut().last_mut().unwrap();
            frame.tracker = Tracker::Option {
                building_inner: true,
            };
            frame.type_plan
        };

        // Get the inner type shape
        let inner_shape = option_def.t;

        // Get the inner layout (needed for AllocatedShape later)
        let inner_layout = inner_shape.layout.sized_layout().map_err(|_| {
            self.err(ReflectErrorKind::Unsized {
                shape: inner_shape,
                operation: "begin_some, getting inner layout",
            })
        })?;

        // If we're re-entering an existing accumulator, get a pointer to the existing inner value
        // instead of allocating new memory
        let inner_data = if is_reentry {
            // The Option is already initialized with Some(inner), so we need to get a pointer
            // to the existing inner value using the Option vtable's get_value function.
            let frame = self.frames().last().unwrap();

            // Get the Option's vtable which has a get_value function
            let option_vtable = match &frame.allocated.shape().def {
                Def::Option(opt_def) => opt_def.vtable,
                _ => unreachable!("Expected Option def"),
            };

            unsafe {
                // Use the vtable's get_value function to get a pointer to the inner T
                // get_value takes PtrConst and returns Option<PtrConst>
                let option_ptr = PtrConst::new(frame.data.as_byte_ptr());
                let inner_ptr_opt = (option_vtable.get_value)(option_ptr);
                let inner_ptr = inner_ptr_opt.expect("Option should be Some when re-entering");
                // Convert PtrConst to *mut for PtrUninit::new
                PtrUninit::new(inner_ptr.as_byte_ptr() as *mut u8)
            }
        } else {
            // Allocate memory for the inner value
            if inner_layout.size() == 0 {
                // For ZST, use a non-null but unallocated pointer
                PtrUninit::new(NonNull::<u8>::dangling().as_ptr())
            } else {
                // Allocate memory for the inner value
                let ptr = unsafe { ::alloc::alloc::alloc(inner_layout) };
                let Some(ptr) = NonNull::new(ptr) else {
                    ::alloc::alloc::handle_alloc_error(inner_layout);
                };
                PtrUninit::new(ptr.as_ptr())
            }
        };

        // Create a new frame for the inner value
        // For re-entry, we use ManagedElsewhere ownership since the Option frame owns the memory
        // Get child type plan NodeId for Option inner
        let child_plan = parent_type_plan.and_then(|pn| self.root_plan.option_inner_node(pn));
        let mut inner_frame = Frame::new(
            inner_data,
            AllocatedShape::new(inner_shape, inner_layout.size()),
            if is_reentry {
                FrameOwnership::BorrowedInPlace
            } else {
                FrameOwnership::Owned
            },
            child_plan,
        );

        // CRITICAL: For re-entry, mark the frame as already initialized so that init_list()
        // doesn't reinitialize the Vec (which would clear it)
        if is_reentry {
            inner_frame.is_init = true;
        }

        self.frames_mut().push(inner_frame);

        Ok(self)
    }

    /// Begin building the inner value of a wrapper type
    pub fn begin_inner(mut self) -> Result<Self, ReflectError> {
        // Get the inner shape and check for try_from
        // Priority: builder_shape (for immutable collections) > inner (for variance/transparent wrappers)
        let (inner_shape, has_try_from, parent_shape, is_option) = {
            let frame = self.frames().last().unwrap();
            // Check builder_shape first (immutable collections like Bytes, Arc<[T]>)
            if let Some(builder_shape) = frame.allocated.shape().builder_shape {
                let has_try_from = frame.allocated.shape().vtable.has_try_from();
                let is_option = matches!(frame.allocated.shape().def, Def::Option(_));
                (
                    Some(builder_shape),
                    has_try_from,
                    frame.allocated.shape(),
                    is_option,
                )
            } else if let Some(inner_shape) = frame.allocated.shape().inner {
                let has_try_from = frame.allocated.shape().vtable.has_try_from();
                let is_option = matches!(frame.allocated.shape().def, Def::Option(_));
                (
                    Some(inner_shape),
                    has_try_from,
                    frame.allocated.shape(),
                    is_option,
                )
            } else {
                (None, false, frame.allocated.shape(), false)
            }
        };

        // Handle re-initialization if needed
        self.prepare_for_reinitialization();

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
                let inner_layout = inner_shape.layout.sized_layout().map_err(|_| {
                    self.err(ReflectErrorKind::Unsized {
                        shape: inner_shape,
                        operation: "begin_inner, getting inner layout",
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

                // For conversion frames, we create a frame directly with the inner shape
                // This allows setting values of the inner type which will be converted
                // The automatic conversion detection in end() will handle the conversion
                trace!(
                    "begin_inner: Creating frame for inner type {inner_shape} (parent is {parent_shape})"
                );
                // Conversion frames use a different shape than the parent, so no precomputed plan applies
                self.frames_mut().push(Frame::new(
                    inner_data,
                    AllocatedShape::new(inner_shape, inner_layout.size()),
                    FrameOwnership::Owned,
                    None,
                ));

                Ok(self)
            } else {
                // For wrapper types without try_from, navigate to the first field
                // This is a common pattern for newtype wrappers
                trace!("begin_inner: No try_from for {parent_shape}, using field navigation");
                self.begin_nth_field(0)
            }
        } else {
            Err(self.err(ReflectErrorKind::OperationFailed {
                shape: parent_shape,
                operation: "type does not have an inner value",
            }))
        }
    }

    /// Begin bulding the source shape for custom deserialization, calling end() for this frame will
    /// call the deserialize_with function provided by the field and set the field using the result.
    pub fn begin_custom_deserialization(mut self) -> Result<Self, ReflectError> {
        let current_frame = self.frames().last().unwrap();
        let target_shape = current_frame.allocated.shape();
        trace!("begin_custom_deserialization: target_shape={target_shape}");
        if let Some(field) = self.parent_field() {
            trace!("begin_custom_deserialization: field name={}", field.name);
            if let Some(proxy_def) = field.proxy() {
                // Get the source shape from the proxy definition
                let source_shape = proxy_def.shape;
                let source_data = source_shape.allocate().map_err(|_| {
                    self.err(ReflectErrorKind::Unsized {
                        shape: target_shape,
                        operation: "Not a Sized type",
                    })
                })?;
                let source_size = source_shape
                    .layout
                    .sized_layout()
                    .expect("must be sized")
                    .size();

                trace!(
                    "begin_custom_deserialization: Creating frame for deserialization type {source_shape}"
                );
                // Custom deserialization uses proxy shape, no precomputed plan applies
                let mut new_frame = Frame::new(
                    source_data,
                    AllocatedShape::new(source_shape, source_size),
                    FrameOwnership::Owned,
                    None,
                );
                new_frame.using_custom_deserialization = true;
                self.frames_mut().push(new_frame);

                Ok(self)
            } else {
                Err(self.err(ReflectErrorKind::OperationFailed {
                    shape: target_shape,
                    operation: "field does not have a proxy definition",
                }))
            }
        } else {
            Err(self.err(ReflectErrorKind::OperationFailed {
                shape: target_shape,
                operation: "not currently processing a field",
            }))
        }
    }

    /// Begin building the source shape for custom deserialization using container-level proxy.
    ///
    /// Unlike `begin_custom_deserialization` which uses field-level proxy info, this method
    /// uses the shape's own proxy definition (from `#[facet(proxy = ...)]` at container level).
    ///
    /// Returns `Ok((self, true))` if the shape has a container-level proxy and we've begun
    /// custom deserialization, `Ok((self, false))` if not (self is returned unchanged).
    pub fn begin_custom_deserialization_from_shape(self) -> Result<(Self, bool), ReflectError> {
        // Delegate to the format-aware version with no format namespace
        self.begin_custom_deserialization_from_shape_with_format(None)
    }

    /// Begin building the source shape for custom deserialization using container-level proxy,
    /// with support for format-specific proxy resolution.
    ///
    /// If `format_namespace` is provided (e.g., `Some("xml")`), looks for a format-specific
    /// proxy first (e.g., `#[facet(xml::proxy = XmlProxy)]`), falling back to the format-agnostic
    /// proxy if no format-specific one is found.
    ///
    /// Returns `Ok((self, true))` if a proxy was found and we've begun custom deserialization,
    /// `Ok((self, false))` if not (self is returned unchanged).
    pub fn begin_custom_deserialization_from_shape_with_format(
        mut self,
        format_namespace: Option<&str>,
    ) -> Result<(Self, bool), ReflectError> {
        let current_frame = self.frames().last().unwrap();
        let target_shape = current_frame.allocated.shape();
        trace!(
            "begin_custom_deserialization_from_shape_with_format: target_shape={target_shape}, format={format_namespace:?}"
        );

        // Use effective_proxy for format-aware resolution
        let Some(proxy_def) = target_shape.effective_proxy(format_namespace) else {
            return Ok((self, false));
        };

        let source_shape = proxy_def.shape;
        let source_data = source_shape.allocate().map_err(|_| {
            self.err(ReflectErrorKind::Unsized {
                shape: target_shape,
                operation: "Not a Sized type",
            })
        })?;
        let source_size = source_shape
            .layout
            .sized_layout()
            .expect("must be sized")
            .size();

        trace!(
            "begin_custom_deserialization_from_shape_with_format: Creating frame for deserialization type {source_shape}"
        );
        // Custom deserialization uses proxy shape, no precomputed plan applies
        let mut new_frame = Frame::new(
            source_data,
            AllocatedShape::new(source_shape, source_size),
            FrameOwnership::Owned,
            None,
        );
        new_frame.using_custom_deserialization = true;
        // Store the target shape's proxy in the frame so end() can use it for conversion
        new_frame.shape_level_proxy = Some(proxy_def);
        self.frames_mut().push(new_frame);

        Ok((self, true))
    }

    /// Begin building the source shape for custom deserialization using field-level proxy,
    /// with support for format-specific proxy resolution.
    ///
    /// If `format_namespace` is provided (e.g., `Some("xml")`), looks for a format-specific
    /// proxy first (e.g., `#[facet(xml::proxy = XmlProxy)]`), falling back to the format-agnostic
    /// proxy if no format-specific one is found.
    ///
    /// This is the format-aware version of `begin_custom_deserialization`.
    pub fn begin_custom_deserialization_with_format(
        mut self,
        format_namespace: Option<&str>,
    ) -> Result<Self, ReflectError> {
        let current_frame = self.frames().last().unwrap();
        let target_shape = current_frame.allocated.shape();
        trace!(
            "begin_custom_deserialization_with_format: target_shape={target_shape}, format={format_namespace:?}"
        );
        if let Some(field) = self.parent_field() {
            trace!(
                "begin_custom_deserialization_with_format: field name={}",
                field.name
            );
            // Use effective_proxy for format-aware resolution
            if let Some(proxy_def) = field.effective_proxy(format_namespace) {
                // Get the source shape from the proxy definition
                let source_shape = proxy_def.shape;
                let source_data = source_shape.allocate().map_err(|_| {
                    self.err(ReflectErrorKind::Unsized {
                        shape: target_shape,
                        operation: "Not a Sized type",
                    })
                })?;
                let source_size = source_shape
                    .layout
                    .sized_layout()
                    .expect("must be sized")
                    .size();

                trace!(
                    "begin_custom_deserialization_with_format: Creating frame for deserialization type {source_shape}"
                );
                // Custom deserialization uses proxy shape, no precomputed plan applies
                let mut new_frame = Frame::new(
                    source_data,
                    AllocatedShape::new(source_shape, source_size),
                    FrameOwnership::Owned,
                    None,
                );
                new_frame.using_custom_deserialization = true;
                // Store the proxy def so end() can use the correct convert_in function
                // This is important for format-specific proxies where field.proxy() would
                // return the wrong proxy.
                new_frame.shape_level_proxy = Some(proxy_def);
                self.frames_mut().push(new_frame);

                Ok(self)
            } else {
                Err(self.err(ReflectErrorKind::OperationFailed {
                    shape: target_shape,
                    operation: "field does not have a proxy",
                }))
            }
        } else {
            Err(self.err(ReflectErrorKind::OperationFailed {
                shape: target_shape,
                operation: "not currently processing a field",
            }))
        }
    }
}
