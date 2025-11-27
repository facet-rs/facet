use super::*;

////////////////////////////////////////////////////////////////////////////////////////////////////
// Misc.
////////////////////////////////////////////////////////////////////////////////////////////////////
impl<'facet> Partial<'facet> {
    /// Returns the current frame count (depth of nesting)
    ///
    /// The initial frame count is 1 — `begin_field` would push a new frame,
    /// bringing it to 2, then `end` would bring it back to `1`.
    ///
    /// This is an implementation detail of `Partial`, kinda, but deserializers
    /// might use this for debug assertions, to make sure the state is what
    /// they think it is.
    #[inline]
    pub fn frame_count(&self) -> usize {
        self.frames.len()
    }

    /// Returns the shape of the current frame.
    #[inline]
    pub fn shape(&self) -> &'static Shape {
        self.frames
            .last()
            .expect("Partial always has at least one frame")
            .shape
    }

    /// Returns whether deferred materialization mode is enabled.
    #[inline]
    pub fn is_deferred(&self) -> bool {
        self.deferred.is_some()
    }

    /// Returns the current deferred resolution, if in deferred mode.
    #[inline]
    pub fn deferred_resolution(&self) -> Option<&Resolution> {
        self.deferred.as_ref().map(|d| &d.resolution)
    }

    /// Returns the current path in deferred mode (for debugging/tracing).
    #[inline]
    pub fn current_path(&self) -> Option<&[&'static str]> {
        self.deferred.as_ref().map(|d| d.current_path.as_slice())
    }

    /// Enables deferred materialization mode with the given Resolution.
    ///
    /// When deferred mode is enabled:
    /// - `end()` stores frames instead of validating them
    /// - Re-entering a path restores the stored frame with its state intact
    /// - `finish_deferred()` performs final validation and materialization
    ///
    /// This allows deserializers to handle interleaved fields (e.g., TOML dotted
    /// keys, flattened structs) where nested fields aren't contiguous in the input.
    ///
    /// # Use Cases
    ///
    /// - TOML dotted keys: `inner.x = 1` followed by `count = 2` then `inner.y = 3`
    /// - Flattened structs where nested fields appear at the parent level
    /// - Any format where field order doesn't match struct nesting
    #[inline]
    pub fn begin_deferred(&mut self, resolution: Resolution) -> &mut Self {
        self.deferred = Some(DeferredState {
            resolution,
            current_path: Vec::new(),
            stored_frames: BTreeMap::new(),
        });
        self
    }

    /// Finishes deferred mode: validates all stored frames and finalizes.
    ///
    /// This method:
    /// 1. Validates that all stored frames are fully initialized
    /// 2. Processes frames from deepest to shallowest, updating parent ISets
    /// 3. Validates the root frame
    ///
    /// # Errors
    ///
    /// Returns an error if any required fields are missing or if the partial is
    /// not in deferred mode.
    pub fn finish_deferred(&mut self) -> Result<&mut Self, ReflectError> {
        let mut deferred_state =
            self.deferred
                .take()
                .ok_or_else(|| ReflectError::InvariantViolation {
                    invariant: "finish_deferred() called but deferred mode is not enabled",
                })?;

        // Sort paths by depth (deepest first) so we process children before parents
        let mut paths: Vec<_> = deferred_state.stored_frames.keys().cloned().collect();
        paths.sort_by(|a, b| b.len().cmp(&a.len()));

        trace!(
            "finish_deferred: Processing {} stored frames in order: {:?}",
            paths.len(),
            paths
        );

        // Process each stored frame from deepest to shallowest
        for path in paths {
            let frame = deferred_state.stored_frames.remove(&path).unwrap();

            trace!(
                "finish_deferred: Processing frame at {:?}, shape {}, tracker {:?}",
                path,
                frame.shape,
                frame.tracker.kind()
            );

            // Validate the frame is fully initialized
            frame.require_full_initialization()?;

            // Update parent's ISet to mark this field as initialized
            // The parent is either in stored_frames (if path.len() > 1) or on the frame stack (if path.len() == 1)
            if let Some(field_name) = path.last() {
                let parent_path: Vec<_> = path[..path.len() - 1].to_vec();

                if parent_path.is_empty() {
                    // Parent is the root frame on the stack
                    if let Some(root_frame) = self.frames.last_mut() {
                        Self::mark_field_initialized(root_frame, field_name);
                    }
                } else {
                    // Parent is also a stored frame
                    if let Some(parent_frame) = deferred_state.stored_frames.get_mut(&parent_path) {
                        Self::mark_field_initialized(parent_frame, field_name);
                    }
                }
            }

            // Frame is validated and parent is updated - frame is no longer needed
            // (The actual data is already in place in memory, pointed to by parent)
            drop(frame);
        }

        // Validate the root frame is fully initialized
        if let Some(frame) = self.frames.last() {
            frame.require_full_initialization()?;
        }

        Ok(self)
    }

    /// Mark a field as initialized in a frame's tracker
    fn mark_field_initialized(frame: &mut Frame, field_name: &str) {
        if let Some(idx) = Self::find_field_index(frame, field_name) {
            match &mut frame.tracker {
                Tracker::Struct { iset, .. } => {
                    iset.set(idx);
                }
                Tracker::Enum { data, .. } => {
                    data.set(idx);
                }
                Tracker::Array { iset, .. } => {
                    iset.set(idx);
                }
                _ => {}
            }
        }
    }

    /// Find the field index for a given field name in a frame
    fn find_field_index(frame: &Frame, field_name: &str) -> Option<usize> {
        match frame.shape.ty {
            Type::User(UserType::Struct(struct_type)) => {
                struct_type.fields.iter().position(|f| f.name == field_name)
            }
            Type::User(UserType::Enum(_)) => {
                if let Tracker::Enum { variant, .. } = &frame.tracker {
                    variant
                        .data
                        .fields
                        .iter()
                        .position(|f| f.name == field_name)
                } else {
                    None
                }
            }
            _ => None,
        }
    }

    /// Pops the current frame off the stack, indicating we're done initializing the current field
    pub fn end(&mut self) -> Result<&mut Self, ReflectError> {
        crate::trace!("end() called");
        self.require_active()?;

        // Special handling for SmartPointerSlice - convert builder to Arc
        if self.frames.len() == 1 {
            if let Tracker::SmartPointerSlice {
                vtable,
                building_item,
            } = &self.frames[0].tracker
            {
                if *building_item {
                    return Err(ReflectError::OperationFailed {
                        shape: self.frames[0].shape,
                        operation: "still building an item, finish it first",
                    });
                }

                // Convert the builder to Arc<[T]>
                let builder_ptr = unsafe { self.frames[0].data.assume_init() };
                let arc_ptr = unsafe { (vtable.convert_fn)(builder_ptr) };

                // Update the frame to store the Arc
                self.frames[0].data = PtrUninit::new(unsafe {
                    NonNull::new_unchecked(arc_ptr.as_byte_ptr() as *mut u8)
                });
                self.frames[0].tracker = Tracker::Init;
                // The builder memory has been consumed by convert_fn, so we no longer own it
                self.frames[0].ownership = FrameOwnership::ManagedElsewhere;

                return Ok(self);
            }
        }

        if self.frames.len() <= 1 {
            // Never pop the last/root frame.
            return Err(ReflectError::InvariantViolation {
                invariant: "Partial::end() called with only one frame on the stack",
            });
        }

        // Require that the top frame is fully initialized before popping.
        // Skip this check in deferred mode - validation happens in finish_deferred().
        if self.deferred.is_none() {
            let frame = self.frames.last().unwrap();
            trace!(
                "end(): Checking full initialization for frame with shape {} and tracker {:?}",
                frame.shape,
                frame.tracker.kind()
            );
            frame.require_full_initialization()?
        }

        // Pop the frame and save its data pointer for SmartPointer handling
        let mut popped_frame = self.frames.pop().unwrap();

        // In deferred mode, store the frame for potential re-entry and skip
        // the normal parent-updating logic. The frame will be finalized later
        // in finish_deferred().
        //
        // We only store if the path depth matches the frame depth, meaning we're
        // ending a tracked struct/enum field, not something like begin_some().
        if let Some(deferred) = &mut self.deferred {
            // Path depth should be (frames.len() - 1) for a tracked field
            // (subtract 1 because root frame isn't in the path)
            let is_tracked_field = !deferred.current_path.is_empty()
                && deferred.current_path.len() == self.frames.len();

            if is_tracked_field {
                trace!(
                    "end(): Storing frame for deferred path {:?}, shape {}",
                    deferred.current_path, popped_frame.shape
                );

                // Store the frame at the current path
                let path = deferred.current_path.clone();
                deferred.stored_frames.insert(path, popped_frame);

                // Pop from current_path
                deferred.current_path.pop();

                // Clear parent's current_child tracking
                if let Some(parent_frame) = self.frames.last_mut() {
                    parent_frame.tracker.clear_current_child();
                }

                return Ok(self);
            }
        }

        // check if this needs deserialization from a different shape
        if popped_frame.using_custom_deserialization {
            if let Some(deserialize_with) = self
                .parent_field()
                .and_then(|field| field.vtable.deserialize_with)
            {
                let parent_frame = self.frames.last_mut().unwrap();

                trace!(
                    "Detected custom conversion needed from {} to {}",
                    popped_frame.shape, parent_frame.shape
                );

                unsafe {
                    let res = {
                        let inner_value_ptr = popped_frame.data.assume_init().as_const();
                        (deserialize_with)(inner_value_ptr, parent_frame.data)
                    };
                    let popped_frame_shape = popped_frame.shape;

                    // we need to do this before any error handling to avoid leaks
                    popped_frame.deinit();
                    popped_frame.dealloc();
                    let rptr = res.map_err(|message| ReflectError::CustomDeserializationError {
                        message,
                        src_shape: popped_frame_shape,
                        dst_shape: parent_frame.shape,
                    })?;
                    if rptr.as_uninit() != parent_frame.data {
                        return Err(ReflectError::CustomDeserializationError {
                            message: "deserialize_with did not return the expected pointer".into(),
                            src_shape: popped_frame_shape,
                            dst_shape: parent_frame.shape,
                        });
                    }
                    parent_frame.mark_as_init();
                }
                return Ok(self);
            }
        }

        // Update parent frame's tracking when popping from a child
        let parent_frame = self.frames.last_mut().unwrap();

        trace!(
            "end(): Popped {} (tracker {:?}), Parent {} (tracker {:?})",
            popped_frame.shape,
            popped_frame.tracker.kind(),
            parent_frame.shape,
            parent_frame.tracker.kind()
        );

        // Check if we need to do a conversion - this happens when:
        // 1. The parent frame has an inner type that matches the popped frame's shape
        // 2. The parent frame has try_from
        // 3. The parent frame is not yet initialized
        let needs_conversion = matches!(parent_frame.tracker, Tracker::Uninit)
            && parent_frame.shape.inner.is_some()
            && parent_frame.shape.inner.unwrap() == popped_frame.shape
            && parent_frame.shape.vtable.try_from.is_some();

        if needs_conversion {
            trace!(
                "Detected implicit conversion needed from {} to {}",
                popped_frame.shape, parent_frame.shape
            );
            // Perform the conversion
            if let Some(try_from_fn) = parent_frame.shape.vtable.try_from {
                let inner_ptr = unsafe { popped_frame.data.assume_init().as_const() };
                let inner_shape = popped_frame.shape;

                trace!("Converting from {} to {}", inner_shape, parent_frame.shape);
                let result = unsafe { try_from_fn(inner_ptr, inner_shape, parent_frame.data) };

                if let Err(e) = result {
                    trace!("Conversion failed: {e:?}");

                    // Deallocate the inner value's memory since conversion failed
                    if let FrameOwnership::Owned = popped_frame.ownership {
                        if let Ok(layout) = popped_frame.shape.layout.sized_layout() {
                            if layout.size() > 0 {
                                trace!(
                                    "Deallocating conversion frame memory after failure: size={}, align={}",
                                    layout.size(),
                                    layout.align()
                                );
                                unsafe {
                                    ::alloc::alloc::dealloc(
                                        popped_frame.data.as_mut_byte_ptr(),
                                        layout,
                                    );
                                }
                            }
                        }
                    }

                    return Err(ReflectError::TryFromError {
                        src_shape: inner_shape,
                        dst_shape: parent_frame.shape,
                        inner: e,
                    });
                }

                trace!("Conversion succeeded, marking parent as initialized");
                parent_frame.tracker = Tracker::Init;

                // Deallocate the inner value's memory since try_from consumed it
                if let FrameOwnership::Owned = popped_frame.ownership {
                    if let Ok(layout) = popped_frame.shape.layout.sized_layout() {
                        if layout.size() > 0 {
                            trace!(
                                "Deallocating conversion frame memory: size={}, align={}",
                                layout.size(),
                                layout.align()
                            );
                            unsafe {
                                ::alloc::alloc::dealloc(
                                    popped_frame.data.as_mut_byte_ptr(),
                                    layout,
                                );
                            }
                        }
                    }
                }

                return Ok(self);
            }
        }

        match &mut parent_frame.tracker {
            Tracker::Struct {
                iset,
                current_child,
            } => {
                if let Some(idx) = *current_child {
                    iset.set(idx);
                    *current_child = None;
                }
            }
            Tracker::Array {
                iset,
                current_child,
            } => {
                if let Some(idx) = *current_child {
                    iset.set(idx);
                    *current_child = None;
                }
            }
            Tracker::SmartPointer { is_initialized } => {
                // We just popped the inner value frame, so now we need to create the smart pointer
                if let Def::Pointer(smart_ptr_def) = parent_frame.shape.def {
                    let Some(new_into_fn) = smart_ptr_def.vtable.new_into_fn else {
                        return Err(ReflectError::OperationFailed {
                            shape: parent_frame.shape,
                            operation: "SmartPointer missing new_into_fn",
                        });
                    };

                    // The child frame contained the inner value
                    let inner_ptr = PtrMut::new(unsafe {
                        NonNull::new_unchecked(popped_frame.data.as_mut_byte_ptr())
                    });

                    // Use new_into_fn to create the Box
                    unsafe {
                        new_into_fn(parent_frame.data, inner_ptr);
                    }

                    // We just moved out of it
                    popped_frame.tracker = Tracker::Uninit;

                    // Deallocate the inner value's memory since new_into_fn moved it
                    popped_frame.dealloc();

                    *is_initialized = true;
                }
            }
            Tracker::Enum {
                data,
                current_child,
                ..
            } => {
                if let Some(idx) = *current_child {
                    data.set(idx);
                    *current_child = None;
                }
            }
            Tracker::List {
                is_initialized: true,
                current_child,
            } => {
                if *current_child {
                    // We just popped an element frame, now push it to the list
                    if let Def::List(list_def) = parent_frame.shape.def {
                        let Some(push_fn) = list_def.vtable.push else {
                            return Err(ReflectError::OperationFailed {
                                shape: parent_frame.shape,
                                operation: "List missing push function",
                            });
                        };

                        // The child frame contained the element value
                        let element_ptr = PtrMut::new(unsafe {
                            NonNull::new_unchecked(popped_frame.data.as_mut_byte_ptr())
                        });

                        // Use push to add element to the list
                        unsafe {
                            push_fn(
                                PtrMut::new(NonNull::new_unchecked(
                                    parent_frame.data.as_mut_byte_ptr(),
                                )),
                                element_ptr,
                            );
                        }

                        // Push moved out of popped_frame
                        popped_frame.tracker = Tracker::Uninit;
                        popped_frame.dealloc();

                        *current_child = false;
                    }
                }
            }
            Tracker::Map {
                is_initialized: true,
                insert_state,
            } => {
                match insert_state {
                    MapInsertState::PushingKey { key_ptr } => {
                        // We just popped the key frame
                        if let Some(key_ptr) = key_ptr {
                            // Transition to PushingValue state
                            *insert_state = MapInsertState::PushingValue {
                                key_ptr: *key_ptr,
                                value_ptr: None,
                            };
                        }
                    }
                    MapInsertState::PushingValue { key_ptr, value_ptr } => {
                        // We just popped the value frame, now insert the pair
                        if let (Some(value_ptr), Def::Map(map_def)) =
                            (value_ptr, parent_frame.shape.def)
                        {
                            let insert_fn = map_def.vtable.insert_fn;

                            // Use insert to add key-value pair to the map
                            unsafe {
                                insert_fn(
                                    PtrMut::new(NonNull::new_unchecked(
                                        parent_frame.data.as_mut_byte_ptr(),
                                    )),
                                    PtrMut::new(NonNull::new_unchecked(key_ptr.as_mut_byte_ptr())),
                                    PtrMut::new(NonNull::new_unchecked(
                                        value_ptr.as_mut_byte_ptr(),
                                    )),
                                );
                            }

                            // Note: We don't deallocate the key and value memory here.
                            // The insert function has semantically moved the values into the map,
                            // but we still need to deallocate the temporary buffers.
                            // However, since we don't have frames for them anymore (they were popped),
                            // we need to handle deallocation here.
                            if let Ok(key_shape) = map_def.k().layout.sized_layout() {
                                if key_shape.size() > 0 {
                                    unsafe {
                                        ::alloc::alloc::dealloc(
                                            key_ptr.as_mut_byte_ptr(),
                                            key_shape,
                                        );
                                    }
                                }
                            }
                            if let Ok(value_shape) = map_def.v().layout.sized_layout() {
                                if value_shape.size() > 0 {
                                    unsafe {
                                        ::alloc::alloc::dealloc(
                                            value_ptr.as_mut_byte_ptr(),
                                            value_shape,
                                        );
                                    }
                                }
                            }

                            // Reset to idle state
                            *insert_state = MapInsertState::Idle;
                        }
                    }
                    MapInsertState::Idle => {
                        // Nothing to do
                    }
                }
            }
            Tracker::Set {
                is_initialized: true,
                current_child,
            } => {
                if *current_child {
                    // We just popped an element frame, now insert it into the set
                    if let Def::Set(set_def) = parent_frame.shape.def {
                        let insert_fn = set_def.vtable.insert_fn;

                        // The child frame contained the element value
                        let element_ptr = PtrMut::new(unsafe {
                            NonNull::new_unchecked(popped_frame.data.as_mut_byte_ptr())
                        });

                        // Use insert to add element to the set
                        unsafe {
                            insert_fn(
                                PtrMut::new(NonNull::new_unchecked(
                                    parent_frame.data.as_mut_byte_ptr(),
                                )),
                                element_ptr,
                            );
                        }

                        // Insert moved out of popped_frame
                        popped_frame.tracker = Tracker::Uninit;
                        popped_frame.dealloc();

                        *current_child = false;
                    }
                }
            }
            Tracker::Option { building_inner } => {
                // We just popped the inner value frame for an Option's Some variant
                if *building_inner {
                    if let Def::Option(option_def) = parent_frame.shape.def {
                        // Use the Option vtable to initialize Some(inner_value)
                        let init_some_fn = option_def.vtable.init_some_fn;

                        // The popped frame contains the inner value
                        let inner_value_ptr = unsafe { popped_frame.data.assume_init().as_const() };

                        // Initialize the Option as Some(inner_value)
                        unsafe {
                            init_some_fn(parent_frame.data, inner_value_ptr);
                        }

                        // Deallocate the inner value's memory since init_some_fn moved it
                        if let FrameOwnership::Owned = popped_frame.ownership {
                            if let Ok(layout) = popped_frame.shape.layout.sized_layout() {
                                if layout.size() > 0 {
                                    unsafe {
                                        ::alloc::alloc::dealloc(
                                            popped_frame.data.as_mut_byte_ptr(),
                                            layout,
                                        );
                                    }
                                }
                            }
                        }

                        // Mark that we're no longer building the inner value
                        *building_inner = false;
                    } else {
                        return Err(ReflectError::OperationFailed {
                            shape: parent_frame.shape,
                            operation: "Option frame without Option definition",
                        });
                    }
                }
            }
            Tracker::Uninit | Tracker::Init => {
                // the main case here is: the popped frame was a `String` and the
                // parent frame is an `Arc<str>`, `Box<str>` etc.
                match &parent_frame.shape.def {
                    Def::Pointer(smart_ptr_def) => {
                        let pointee =
                            smart_ptr_def
                                .pointee()
                                .ok_or(ReflectError::InvariantViolation {
                                    invariant: "pointer type doesn't have a pointee",
                                })?;

                        if !pointee.is_shape(str::SHAPE) {
                            return Err(ReflectError::InvariantViolation {
                                invariant: "only T=str is supported when building SmartPointer<T> and T is unsized",
                            });
                        }

                        if !popped_frame.shape.is_shape(String::SHAPE) {
                            return Err(ReflectError::InvariantViolation {
                                invariant: "the popped frame should be String when building a SmartPointer<T>",
                            });
                        }

                        popped_frame.require_full_initialization()?;

                        // if the just-popped frame was a SmartPointerStr, we have some conversion to do:
                        // Special-case: SmartPointer<str> (Box<str>, Arc<str>, Rc<str>) via SmartPointerStr tracker
                        // Here, popped_frame actually contains a value for String that should be moved into the smart pointer.
                        // We convert the String into Box<str>, Arc<str>, or Rc<str> as appropriate and write it to the parent frame.
                        use ::alloc::{rc::Rc, string::String, sync::Arc};
                        let parent_shape = parent_frame.shape;

                        let Some(known) = smart_ptr_def.known else {
                            return Err(ReflectError::OperationFailed {
                                shape: parent_shape,
                                operation: "SmartPointerStr for unknown smart pointer kind",
                            });
                        };

                        parent_frame.deinit();

                        // Interpret the memory as a String, then convert and write.
                        let string_ptr = popped_frame.data.as_mut_byte_ptr() as *mut String;
                        let string_value = unsafe { core::ptr::read(string_ptr) };

                        match known {
                            KnownPointer::Box => {
                                let boxed: Box<str> = string_value.into_boxed_str();
                                unsafe {
                                    core::ptr::write(
                                        parent_frame.data.as_mut_byte_ptr() as *mut Box<str>,
                                        boxed,
                                    );
                                }
                            }
                            KnownPointer::Arc => {
                                let arc: Arc<str> = Arc::from(string_value.into_boxed_str());
                                unsafe {
                                    core::ptr::write(
                                        parent_frame.data.as_mut_byte_ptr() as *mut Arc<str>,
                                        arc,
                                    );
                                }
                            }
                            KnownPointer::Rc => {
                                let rc: Rc<str> = Rc::from(string_value.into_boxed_str());
                                unsafe {
                                    core::ptr::write(
                                        parent_frame.data.as_mut_byte_ptr() as *mut Rc<str>,
                                        rc,
                                    );
                                }
                            }
                            _ => {
                                return Err(ReflectError::OperationFailed {
                                    shape: parent_shape,
                                    operation: "Don't know how to build this pointer type",
                                });
                            }
                        }

                        parent_frame.tracker = Tracker::Init;

                        popped_frame.tracker = Tracker::Uninit;
                        popped_frame.dealloc();
                    }
                    _ => {
                        unreachable!(
                            "we popped a frame and parent was Init or Uninit, but it wasn't a smart pointer and... there's no way this should happen normally"
                        )
                    }
                }
            }
            Tracker::SmartPointerSlice {
                vtable,
                building_item,
            } => {
                if *building_item {
                    // We just popped an element frame, now push it to the slice builder
                    let element_ptr = PtrMut::new(unsafe {
                        NonNull::new_unchecked(popped_frame.data.as_mut_byte_ptr())
                    });

                    // Use the slice builder's push_fn to add the element
                    crate::trace!("Pushing element to slice builder");
                    unsafe {
                        let parent_ptr = parent_frame.data.assume_init();
                        (vtable.push_fn)(parent_ptr, element_ptr);
                    }

                    popped_frame.tracker = Tracker::Uninit;
                    popped_frame.dealloc();

                    if let Tracker::SmartPointerSlice {
                        building_item: bi, ..
                    } = &mut parent_frame.tracker
                    {
                        *bi = false;
                    }
                }
            }
            _ => {}
        }

        Ok(self)
    }

    /// Returns a human-readable path representing the current traversal in the builder,
    /// e.g., `RootStruct.fieldName[index].subfield`.
    pub fn path(&self) -> String {
        let mut out = String::new();

        let mut path_components = Vec::new();
        // The stack of enum/struct/sequence names currently in context.
        // Start from root and build upwards.
        for (i, frame) in self.frames.iter().enumerate() {
            match frame.shape.ty {
                Type::User(user_type) => match user_type {
                    UserType::Struct(struct_type) => {
                        // Try to get currently active field index
                        let mut field_str = None;
                        if let Tracker::Struct {
                            current_child: Some(idx),
                            ..
                        } = &frame.tracker
                        {
                            if let Some(field) = struct_type.fields.get(*idx) {
                                field_str = Some(field.name);
                            }
                        }
                        if i == 0 {
                            // Use Display for the root struct shape
                            path_components.push(format!("{}", frame.shape));
                        }
                        if let Some(field_name) = field_str {
                            path_components.push(format!(".{field_name}"));
                        }
                    }
                    UserType::Enum(_enum_type) => {
                        // Try to get currently active variant and field
                        if let Tracker::Enum {
                            variant,
                            current_child,
                            ..
                        } = &frame.tracker
                        {
                            if i == 0 {
                                // Use Display for the root enum shape
                                path_components.push(format!("{}", frame.shape));
                            }
                            path_components.push(format!("::{}", variant.name));
                            if let Some(idx) = *current_child {
                                if let Some(field) = variant.data.fields.get(idx) {
                                    path_components.push(format!(".{}", field.name));
                                }
                            }
                        } else if i == 0 {
                            // just the enum display
                            path_components.push(format!("{}", frame.shape));
                        }
                    }
                    UserType::Union(_union_type) => {
                        path_components.push(format!("{}", frame.shape));
                    }
                    UserType::Opaque => {
                        path_components.push("<opaque>".to_string());
                    }
                },
                Type::Sequence(seq_type) => match seq_type {
                    facet_core::SequenceType::Array(_array_def) => {
                        // Try to show current element index
                        if let Tracker::Array {
                            current_child: Some(idx),
                            ..
                        } = &frame.tracker
                        {
                            path_components.push(format!("[{idx}]"));
                        }
                    }
                    // You can add more for Slice, Vec, etc., if applicable
                    _ => {
                        // just indicate "[]" for sequence
                        path_components.push("[]".to_string());
                    }
                },
                Type::Pointer(_) => {
                    // Indicate deref
                    path_components.push("*".to_string());
                }
                _ => {
                    // No structural path
                }
            }
        }
        // Merge the path_components into a single string
        for component in path_components {
            out.push_str(&component);
        }
        out
    }

    /// Get the field for the parent frame
    pub fn parent_field(&self) -> Option<&Field> {
        self.frames.iter().rev().nth(1).and_then(|f| f.get_field())
    }
}
