use super::*;

////////////////////////////////////////////////////////////////////////////////////////////////////
// Field selection
////////////////////////////////////////////////////////////////////////////////////////////////////
impl Partial<'_> {
    /// Find the index of a field by name in the current struct
    ///
    /// If the current frame isn't a struct or an enum (with a selected variant)
    /// then this returns `None` for sure.
    pub fn field_index(&self, field_name: &str) -> Option<usize> {
        let frame = self.frames().last()?;

        match frame.shape.ty {
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
        let frame = self.frames().last().ok_or(ReflectError::NoActiveFrame)?;

        match &frame.tracker {
            Tracker::Uninit => Ok(false),
            Tracker::Init => Ok(true),
            Tracker::Struct { iset, .. } => Ok(iset.get(index)),
            Tracker::Enum { data, variant, .. } => {
                // Check if the field is already marked as set
                if data.get(index) {
                    return Ok(true);
                }

                // For enum variant fields that are empty structs, they are always initialized
                if let Some(field) = variant.data.fields.get(index) {
                    if let Type::User(UserType::Struct(field_struct)) = field.shape().ty {
                        if field_struct.fields.is_empty() {
                            return Ok(true);
                        }
                    }
                }

                Ok(false)
            }
            Tracker::Option { building_inner } => {
                // For Options, index 0 represents the inner value
                if index == 0 {
                    Ok(!building_inner)
                } else {
                    Err(ReflectError::InvalidOperation {
                        operation: "is_field_set",
                        reason: "Option only has one field (index 0)",
                    })
                }
            }
            _ => Err(ReflectError::InvalidOperation {
                operation: "is_field_set",
                reason: "Current frame is not a struct, enum variant, or option",
            }),
        }
    }

    /// Selects a field (by name) of a struct or enum data.
    ///
    /// For enums, the variant needs to be selected first, see [Self::select_nth_variant]
    /// and friends.
    pub fn begin_field(&mut self, field_name: &str) -> Result<&mut Self, ReflectError> {
        self.require_active()?;

        let frame = self.frames().last().unwrap();
        let fields = self.get_fields()?;
        let Some(idx) = fields.iter().position(|f| f.name == field_name) else {
            return Err(ReflectError::FieldError {
                shape: frame.shape,
                field_error: facet_core::FieldError::NoSuchField,
            });
        };
        self.begin_nth_field(idx)
    }

    /// Begins the nth field of a struct, enum variant, or array, by index.
    ///
    /// On success, this pushes a new frame which must be ended with a call to [Partial::end]
    pub fn begin_nth_field(&mut self, idx: usize) -> Result<&mut Self, ReflectError> {
        self.require_active()?;

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

            if let Some(name) = field_name {
                if should_track {
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

        let frame = self.frames_mut().last_mut().unwrap();

        let next_frame = match frame.shape.ty {
            Type::User(user_type) => match user_type {
                UserType::Struct(struct_type) => {
                    Self::begin_nth_struct_field(frame, struct_type, idx)?
                }
                UserType::Enum(_) => {
                    // Check if we have a variant selected
                    match &frame.tracker {
                        Tracker::Enum { variant, .. } => {
                            Self::begin_nth_enum_field(frame, variant, idx)?
                        }
                        _ => {
                            return Err(ReflectError::OperationFailed {
                                shape: frame.shape,
                                operation: "must call select_variant before selecting enum fields",
                            });
                        }
                    }
                }
                UserType::Union(_) => {
                    return Err(ReflectError::OperationFailed {
                        shape: frame.shape,
                        operation: "cannot select a field from a union",
                    });
                }
                UserType::Opaque => {
                    return Err(ReflectError::OperationFailed {
                        shape: frame.shape,
                        operation: "cannot select a field from an opaque type",
                    });
                }
            },
            Type::Sequence(sequence_type) => match sequence_type {
                SequenceType::Array(array_type) => {
                    Self::begin_nth_array_element(frame, array_type, idx)?
                }
                SequenceType::Slice(_) => {
                    return Err(ReflectError::OperationFailed {
                        shape: frame.shape,
                        operation: "cannot select a field from slices yet",
                    });
                }
            },
            _ => {
                return Err(ReflectError::OperationFailed {
                    shape: frame.shape,
                    operation: "cannot select a field from this type",
                });
            }
        };

        self.frames_mut().push(next_frame);
        Ok(self)
    }

    /// Get the field name for path tracking (used in deferred mode)
    fn get_field_name_for_path(&self, idx: usize) -> Option<&'static str> {
        let frame = self.frames().last()?;
        match frame.shape.ty {
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
    pub fn set_nth_field_to_default(&mut self, idx: usize) -> Result<&mut Self, ReflectError> {
        self.require_active()?;

        let frame = self.frames().last().unwrap();
        let fields = self.get_fields()?;

        if idx >= fields.len() {
            return Err(ReflectError::OperationFailed {
                shape: frame.shape,
                operation: "field index out of bounds",
            });
        }

        let field = fields[idx];

        // Check for field-level default function first, then type-level default
        if let Some(field_default_fn) = field.vtable.default_fn {
            self.begin_nth_field(idx)?;
            // the field default fn should be well-behaved
            unsafe {
                self.set_from_function(|ptr| {
                    field_default_fn(ptr);
                    Ok(())
                })?;
            }
            self.end()
        } else if field.shape().is(Characteristic::Default) {
            self.begin_nth_field(idx)?;
            self.set_default()?;
            self.end()
        } else {
            return Err(ReflectError::DefaultAttrButNoDefaultImpl {
                shape: field.shape(),
            });
        }
    }

    /// Given a `Partial` for the same shape, and assuming that partial has the nth
    /// field initialized, move the value from `src` to `self`, marking it as deinitialized
    /// in `src`.
    pub fn steal_nth_field(
        &mut self,
        src: &mut Partial,
        field_index: usize,
    ) -> Result<&mut Self, ReflectError> {
        let dst_shape = self.shape();
        let src_shape = src.shape();
        if dst_shape != src_shape {
            return Err(ReflectError::HeistCancelledDifferentShapes {
                src_shape,
                dst_shape,
            });
        }

        // FIXME: what about enums? we don't check that the right variant is
        // selected here.
        if !src.is_field_set(field_index)? {
            return Err(ReflectError::InvariantViolation {
                invariant: "stolen field must be initialized",
            });
        }

        let maybe_fields = match src_shape.ty {
            Type::Primitive(_primitive_type) => None,
            Type::Sequence(_sequence_type) => None,
            Type::User(user_type) => match user_type {
                UserType::Struct(struct_type) => Some(struct_type.fields),
                UserType::Enum(_enum_type) => match self.selected_variant() {
                    Some(variant) => Some(variant.data.fields),
                    None => {
                        return Err(ReflectError::InvariantViolation {
                            invariant: "enum field thief must have variant selected",
                        });
                    }
                },
                UserType::Union(_union_type) => None,
                UserType::Opaque => None,
            },
            Type::Pointer(_pointer_type) => None,
        };

        let Some(fields) = maybe_fields else {
            return Err(ReflectError::OperationFailed {
                shape: src_shape,
                operation: "fetching field list for steal_nth_field",
            });
        };

        if field_index >= fields.len() {
            return Err(ReflectError::OperationFailed {
                shape: src_shape,
                operation: "field index out of bounds",
            });
        }
        let field = fields[field_index];

        let src_frame = src.frames_mut().last_mut().unwrap();

        self.begin_nth_field(field_index)?;
        unsafe {
            self.set_from_function(|dst_field_ptr| {
                let src_field_ptr = src_frame.data.field_init_at(field.offset).as_const();
                dst_field_ptr
                    .copy_from(src_field_ptr, field.shape())
                    .unwrap();
                Ok(())
            })?;
        }
        self.end()?;

        // now mark field as uninitialized in `src`
        match &mut src_frame.tracker {
            Tracker::Uninit => {
                unreachable!("we just stole a field from src, it couldn't have been fully uninit")
            }
            Tracker::Init => {
                // all struct fields were init so we don't even have a struct tracker,
                // let's make one!
                let mut iset = ISet::new(fields.len());
                iset.set_all();
                iset.unset(field_index);
                src_frame.tracker = Tracker::Struct {
                    iset,
                    current_child: None,
                }
            }
            Tracker::Array { .. } => unreachable!("can't steal fields from arrays"),
            Tracker::Struct { iset, .. } => {
                iset.unset(field_index);
            }
            Tracker::SmartPointer { .. } => {
                unreachable!("can't steal fields from smart pointers")
            }
            Tracker::SmartPointerSlice { .. } => {
                unreachable!("can't steal fields from smart pointer slices")
            }
            Tracker::Enum { data, .. } => {
                data.unset(field_index);
            }
            Tracker::List { .. } => {
                unreachable!("can't steal fields from lists")
            }
            Tracker::Map { .. } => {
                unreachable!("can't steal fields from maps")
            }
            Tracker::Set { .. } => {
                unreachable!("can't steal fields from sets")
            }
            Tracker::Option { .. } => {
                unreachable!("can't steal fields from options")
            }
        }

        Ok(self)
    }
}
