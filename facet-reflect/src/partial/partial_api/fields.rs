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

        match frame.allocated.shape().ty {
            Type::User(UserType::Struct(struct_def)) => {
                struct_def.fields.iter().position(|f| f.name == field_name)
            }
            Type::User(UserType::Enum(_)) => {
                // If we're in an enum variant, check its fields
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

    /// Check if a struct field at the given index has been set
    pub fn is_field_set(&self, index: usize) -> Result<bool, ReflectError> {
        let frame = self
            .frames()
            .last()
            .ok_or_else(|| self.err(ReflectErrorKind::NoActiveFrame))?;

        match &frame.tracker {
            Tracker::Scalar => Ok(frame.is_init),
            Tracker::Struct { iset, .. } => Ok(iset.get(index)),
            Tracker::Enum { data, variant, .. } => {
                // Check if the field is already marked as set
                if data.get(index) {
                    return Ok(true);
                }

                // For enum variant fields that are empty structs, they are always initialized
                if let Some(field) = variant.data.fields.get(index)
                    && let Type::User(UserType::Struct(field_struct)) = field.shape().ty
                    && field_struct.fields.is_empty()
                {
                    return Ok(true);
                }

                Ok(false)
            }
            Tracker::Option { building_inner } => {
                // For Options, index 0 represents the inner value
                if index == 0 {
                    Ok(!building_inner)
                } else {
                    Err(self.err(ReflectErrorKind::InvalidOperation {
                        operation: "is_field_set",
                        reason: "Option only has one field (index 0)",
                    }))
                }
            }
            Tracker::Result { building_inner, .. } => {
                // For Results, index 0 represents the inner value (Ok or Err)
                if index == 0 {
                    Ok(!building_inner)
                } else {
                    Err(self.err(ReflectErrorKind::InvalidOperation {
                        operation: "is_field_set",
                        reason: "Result only has one field (index 0)",
                    }))
                }
            }
            _ => Err(self.err(ReflectErrorKind::InvalidOperation {
                operation: "is_field_set",
                reason: "Current frame is not a struct, enum variant, option, or result",
            })),
        }
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
        // In deferred mode, get the field name for path tracking and check for stored frames.
        // Only track the path if we're at a "navigable" level - i.e., the path length matches
        // the expected depth (frames.len() - 1). If we're inside a collection item, the path
        // will be shorter than expected, so we shouldn't add to it.
        let field_name = self.get_field_name_for_path(idx);

        // Update current_path in deferred mode
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

        // Get the shape first to avoid borrow conflicts
        let shape = self.frames().last().unwrap().allocated.shape();

        let next_frame = match shape.ty {
            Type::User(user_type) => match user_type {
                UserType::Struct(struct_type) => {
                    let frame = self.frames_mut().last_mut().unwrap();
                    self.begin_nth_struct_field(frame, struct_type, idx)?
                }
                UserType::Enum(_) => {
                    // Check if we have a variant selected
                    let frame = self.frames_mut().last_mut().unwrap();
                    match &frame.tracker {
                        Tracker::Enum { variant, .. } => {
                            self.begin_nth_enum_field(frame, variant, idx)?
                        }
                        _ => {
                            return Err(self.err(ReflectErrorKind::OperationFailed {
                                shape,
                                operation: "must call select_variant before selecting enum fields",
                            }));
                        }
                    }
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
                    let frame = self.frames_mut().last_mut().unwrap();
                    self.begin_nth_array_element(frame, array_type, idx)?
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
