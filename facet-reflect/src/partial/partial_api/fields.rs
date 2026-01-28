use super::*;

////////////////////////////////////////////////////////////////////////////////////////////////////
// Field selection
////////////////////////////////////////////////////////////////////////////////////////////////////
impl<const BORROW: bool> Partial<'_, BORROW> {
    /// Find the index of a field by name in the current struct
    ///
    /// If the current frame isn't a struct or an enum (with a selected variant)
    /// then this returns `None` for sure.
    pub fn field_index(&self, field_name: &str) -> Option<usize> {
        let frame = self.frames().last()?;
        let node_id = frame.type_plan;

        // For structs: use StructPlan's field_lookup
        if let Some(struct_plan) = self.root_plan.as_struct_plan(node_id) {
            return struct_plan.field_lookup.find(field_name);
        }

        // For enums with selected variant: use variant's field_lookup
        if let Some(enum_plan) = self.root_plan.as_enum_plan(node_id)
            && let Tracker::Enum { variant_idx, .. } = &frame.tracker
        {
            return enum_plan
                .variants
                .get(*variant_idx)?
                .field_lookup
                .find(field_name);
        }

        None
    }

    /// Check if a struct field at the given index has been set
    pub fn is_field_set(&self, index: usize) -> Result<bool, ReflectError> {
        let frame = self
            .frames()
            .last()
            .ok_or_else(|| self.err(ReflectErrorKind::NoActiveFrame))?;

        // First check via ISet/tracker
        let is_set_in_tracker = match &frame.tracker {
            Tracker::Scalar => frame.is_init,
            Tracker::Struct { iset, .. } => iset.get(index),
            Tracker::Enum { data, variant, .. } => {
                // Check if the field is already marked as set
                if data.get(index) {
                    true
                } else if let Some(field) = variant.data.fields.get(index)
                    && let Type::User(UserType::Struct(field_struct)) = field.shape().ty
                    && field_struct.fields.is_empty()
                {
                    // For enum variant fields that are empty structs, they are always initialized
                    true
                } else {
                    false
                }
            }
            Tracker::Option { building_inner } => {
                if index == 0 {
                    !building_inner
                } else {
                    return Err(self.err(ReflectErrorKind::InvalidOperation {
                        operation: "is_field_set",
                        reason: "Option only has one field (index 0)",
                    }));
                }
            }
            Tracker::Result { building_inner, .. } => {
                if index == 0 {
                    !building_inner
                } else {
                    return Err(self.err(ReflectErrorKind::InvalidOperation {
                        operation: "is_field_set",
                        reason: "Result only has one field (index 0)",
                    }));
                }
            }
            _ => {
                return Err(self.err(ReflectErrorKind::InvalidOperation {
                    operation: "is_field_set",
                    reason: "Current frame is not a struct, enum variant, option, or result",
                }));
            }
        };

        if is_set_in_tracker {
            return Ok(true);
        }

        // In deferred mode, also check if there's a stored frame for this field.
        // The ISet won't be updated when frames are stored, so we need to check
        // stored_frames directly to know if a value exists.
        if let FrameMode::Deferred {
            current_path,
            stored_frames,
            ..
        } = &self.mode
        {
            // Get field name from index
            if let Some(field_name) = self.get_field_name_for_path(index) {
                // Construct the full path for this field
                let mut check_path = current_path.clone();
                check_path.push(field_name);

                // Check if this path exists in stored frames
                if stored_frames.contains_key(&check_path) {
                    return Ok(true);
                }
            }
        }

        Ok(false)
    }

    /// Selects a field (by name) of a struct or enum data.
    ///
    /// For enums, the variant needs to be selected first, see [Self::select_nth_variant]
    /// and friends.
    pub fn begin_field(self, field_name: &str) -> Result<Self, ReflectError> {
        let frame = self.frames().last().unwrap();
        let fields = self.get_fields()?;
        let Some(idx) = fields.iter().position(|f| f.name == field_name) else {
            return Err(self.err(ReflectErrorKind::FieldError {
                shape: frame.allocated.shape(),
                field_error: facet_core::FieldError::NoSuchField,
            }));
        };
        self.begin_nth_field(idx)
    }

    /// Begins the nth field of a struct, enum variant, or array, by index.
    ///
    /// On success, this pushes a new frame which must be ended with a call to [Partial::end]
    pub fn begin_nth_field(mut self, idx: usize) -> Result<Self, ReflectError> {
        // Handle deferred mode path tracking (rare path - only for partial deserialization)
        if self.is_deferred() {
            // Get field name for path tracking
            let field_name = self.get_field_name_for_path(idx);

            if let FrameMode::Deferred {
                stack,
                start_depth,
                current_path,
                stored_frames,
                ..
            } = &mut self.mode
            {
                // Only track path if we're at the expected navigable depth
                // Path should have (frames.len() - start_depth) entries before we add this field
                let relative_depth = stack.len() - *start_depth;
                let should_track = current_path.len() == relative_depth;

                if let Some(name) = field_name
                    && should_track
                {
                    current_path.push(name);

                    // Check if we have a stored frame for this path
                    if let Some(stored_frame) = stored_frames.remove(current_path) {
                        trace!("begin_nth_field: Restoring stored frame for path {current_path:?}");

                        // Update parent's current_child tracking
                        let frame = stack.last_mut().unwrap();
                        frame.tracker.set_current_child(idx);

                        stack.push(stored_frame);
                        return Ok(self);
                    }
                }
            }
        }

        // Get shape and parent NodeId first (NodeId is Copy, no borrow conflict)
        let frame = self.frames().last().unwrap();
        let shape = frame.allocated.shape();
        let parent_node = frame.type_plan; // Copy the NodeId

        let next_frame = match shape.ty {
            Type::User(user_type) => match user_type {
                UserType::Struct(struct_type) => {
                    // Compute child NodeId before mutable borrow
                    let child_plan = self
                        .root_plan
                        .struct_field_node(parent_node, idx)
                        .expect("TypePlan must have struct field node");
                    let frame = self.frames_mut().last_mut().unwrap();
                    Self::begin_nth_struct_field(frame, struct_type, idx, child_plan)
                        .map_err(|e| self.err(e))?
                }
                UserType::Enum(_) => {
                    // Check if we have a variant selected and get variant info
                    let frame = self.frames().last().unwrap();
                    let (variant, variant_idx) = match &frame.tracker {
                        Tracker::Enum {
                            variant,
                            variant_idx,
                            ..
                        } => (*variant, *variant_idx),
                        _ => {
                            return Err(self.err(ReflectErrorKind::OperationFailed {
                                shape,
                                operation: "must call select_variant before selecting enum fields",
                            }));
                        }
                    };
                    // Compute child NodeId using stored variant_idx (O(1) lookup, not O(n) search)
                    let child_plan = self
                        .root_plan
                        .enum_variant_field_node(parent_node, variant_idx, idx)
                        .expect("TypePlan must have enum variant field node");
                    let frame = self.frames_mut().last_mut().unwrap();
                    Self::begin_nth_enum_field(frame, variant, idx, child_plan)
                        .map_err(|e| self.err(e))?
                }
                UserType::Union(_) => {
                    return Err(self.err(ReflectErrorKind::OperationFailed {
                        shape,
                        operation: "cannot select a field from a union",
                    }));
                }
                UserType::Opaque => {
                    return Err(self.err(ReflectErrorKind::OperationFailed {
                        shape,
                        operation: "cannot select a field from an opaque type",
                    }));
                }
            },
            Type::Sequence(sequence_type) => match sequence_type {
                SequenceType::Array(array_type) => {
                    // Compute child NodeId before mutable borrow
                    let child_plan = self
                        .root_plan
                        .list_item_node(parent_node)
                        .expect("TypePlan must have array item node");
                    let frame = self.frames_mut().last_mut().unwrap();
                    Self::begin_nth_array_element(frame, array_type, idx, child_plan)
                        .map_err(|e| self.err(e))?
                }
                SequenceType::Slice(_) => {
                    return Err(self.err(ReflectErrorKind::OperationFailed {
                        shape,
                        operation: "cannot select a field from slices yet",
                    }));
                }
            },
            _ => {
                return Err(self.err(ReflectErrorKind::OperationFailed {
                    shape,
                    operation: "cannot select a field from this type",
                }));
            }
        };

        self.frames_mut().push(next_frame);
        Ok(self)
    }

    /// Get the field name for path tracking (used in deferred mode)
    fn get_field_name_for_path(&self, idx: usize) -> Option<&'static str> {
        let frame = self.frames().last()?;
        match frame.allocated.shape().ty {
            Type::User(UserType::Struct(struct_type)) => {
                struct_type.fields.get(idx).map(|f| f.name)
            }
            Type::User(UserType::Enum(_)) => {
                if let Tracker::Enum { variant, .. } = &frame.tracker {
                    variant.data.fields.get(idx).map(|f| f.name)
                } else {
                    None
                }
            }
            // For arrays, we could use index as string, but for now return None
            _ => None,
        }
    }

    /// Sets the given field to its default value, preferring:
    ///
    ///   * A `default = some_fn()` function
    ///   * The field's `Default` implementation if any
    ///
    /// But without going all the way up to the parent struct's `Default` impl.
    ///
    /// Errors out if idx is out of bound, if the field has no default method or Default impl.
    pub fn set_nth_field_to_default(mut self, idx: usize) -> Result<Self, ReflectError> {
        let frame = self.frames().last().unwrap();
        let fields = self.get_fields()?;

        if idx >= fields.len() {
            return Err(self.err(ReflectErrorKind::OperationFailed {
                shape: frame.allocated.shape(),
                operation: "field index out of bounds",
            }));
        }

        let field = fields[idx];

        // Check for field-level default first, then type-level default
        if let Some(default_source) = field.default {
            self = self.begin_nth_field(idx)?;
            match default_source {
                facet_core::DefaultSource::Custom(field_default_fn) => {
                    // Custom default function provided via #[facet(default = expr)]
                    self = unsafe {
                        self.set_from_function(|ptr| {
                            field_default_fn(ptr);
                            Ok(())
                        })?
                    };
                }
                facet_core::DefaultSource::FromTrait => {
                    // Use the type's Default trait via #[facet(default)]
                    self = self.set_default()?;
                }
            }
            self.end()
        } else if field.shape().is(Characteristic::Default) {
            self = self.begin_nth_field(idx)?;
            self = self.set_default()?;
            self.end()
        } else {
            Err(self.err(ReflectErrorKind::DefaultAttrButNoDefaultImpl {
                shape: field.shape(),
            }))
        }
    }
}
