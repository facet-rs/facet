use super::*;

////////////////////////////////////////////////////////////////////////////////////////////////////
// Internal methods
////////////////////////////////////////////////////////////////////////////////////////////////////
impl<'facet, const BORROW: bool> Partial<'facet, BORROW> {
    /// Preconditions:
    ///
    /// - `require_active()` check was made
    /// - frame.shape.ty is an Enum
    /// - `discriminant` is a valid discriminant
    ///
    /// Panics if current tracker is anything other than `Uninit`
    /// (switching variants is not supported for now).
    pub(crate) fn select_variant_internal(
        &mut self,
        enum_type: &EnumType,
        variant: &'static Variant,
    ) -> Result<(), ReflectError> {
        // Check all invariants early before making any changes
        let frame = self.frames().last().unwrap();

        // Check enum representation early
        match enum_type.enum_repr {
            EnumRepr::RustNPO => {
                return Err(ReflectError::OperationFailed {
                    shape: frame.shape,
                    operation: "RustNPO enums are not supported for incremental building",
                });
            }
            EnumRepr::U8
            | EnumRepr::U16
            | EnumRepr::U32
            | EnumRepr::U64
            | EnumRepr::I8
            | EnumRepr::I16
            | EnumRepr::I32
            | EnumRepr::I64
            | EnumRepr::USize
            | EnumRepr::ISize => {
                // These are supported, continue
            }
        }

        let Some(discriminant) = variant.discriminant else {
            return Err(ReflectError::OperationFailed {
                shape: frame.shape,
                operation: "trying to select an enum variant without a discriminant",
            });
        };

        // All checks passed, now we can safely make changes
        let fr = self.frames_mut().last_mut().unwrap();

        // Write the discriminant to memory
        unsafe {
            match enum_type.enum_repr {
                EnumRepr::U8 => {
                    let ptr = fr.data.as_mut_byte_ptr();
                    *ptr = discriminant as u8;
                }
                EnumRepr::U16 => {
                    let ptr = fr.data.as_mut_byte_ptr() as *mut u16;
                    *ptr = discriminant as u16;
                }
                EnumRepr::U32 => {
                    let ptr = fr.data.as_mut_byte_ptr() as *mut u32;
                    *ptr = discriminant as u32;
                }
                EnumRepr::U64 => {
                    let ptr = fr.data.as_mut_byte_ptr() as *mut u64;
                    *ptr = discriminant as u64;
                }
                EnumRepr::I8 => {
                    let ptr = fr.data.as_mut_byte_ptr() as *mut i8;
                    *ptr = discriminant as i8;
                }
                EnumRepr::I16 => {
                    let ptr = fr.data.as_mut_byte_ptr() as *mut i16;
                    *ptr = discriminant as i16;
                }
                EnumRepr::I32 => {
                    let ptr = fr.data.as_mut_byte_ptr() as *mut i32;
                    *ptr = discriminant as i32;
                }
                EnumRepr::I64 => {
                    let ptr = fr.data.as_mut_byte_ptr() as *mut i64;
                    *ptr = discriminant;
                }
                EnumRepr::USize => {
                    let ptr = fr.data.as_mut_byte_ptr() as *mut usize;
                    *ptr = discriminant as usize;
                }
                EnumRepr::ISize => {
                    let ptr = fr.data.as_mut_byte_ptr() as *mut isize;
                    *ptr = discriminant as isize;
                }
                _ => unreachable!("Already checked enum representation above"),
            }
        }

        // Update tracker to track the variant
        fr.tracker = Tracker::Enum {
            variant,
            data: ISet::new(variant.data.fields.len()),
            current_child: None,
        };

        Ok(())
    }

    /// Used by `begin_field` etc. to get a list of fields to look through, errors out
    /// if we're not pointing to a struct or an enum with an already-selected variant
    pub(crate) fn get_fields(&self) -> Result<&'static [Field], ReflectError> {
        let frame = self.frames().last().unwrap();
        match frame.shape.ty {
            Type::Undefined => Err(ReflectError::OperationFailed {
                shape: frame.shape,
                operation: "shape type is undefined - shape was not properly configured",
            }),
            Type::Primitive(_) => Err(ReflectError::OperationFailed {
                shape: frame.shape,
                operation: "cannot select a field from a primitive type",
            }),
            Type::Sequence(_) => Err(ReflectError::OperationFailed {
                shape: frame.shape,
                operation: "cannot select a field from a sequence type",
            }),
            Type::User(user_type) => match user_type {
                UserType::Struct(struct_type) => Ok(struct_type.fields),
                UserType::Enum(_) => {
                    let Tracker::Enum { variant, .. } = &frame.tracker else {
                        return Err(ReflectError::OperationFailed {
                            shape: frame.shape,
                            operation: "must select variant before selecting enum fields",
                        });
                    };
                    Ok(variant.data.fields)
                }
                UserType::Union(_) => Err(ReflectError::OperationFailed {
                    shape: frame.shape,
                    operation: "cannot select a field from a union type",
                }),
                UserType::Opaque => Err(ReflectError::OperationFailed {
                    shape: frame.shape,
                    operation: "opaque types cannot be reflected upon",
                }),
            },
            Type::Pointer(_) => Err(ReflectError::OperationFailed {
                shape: frame.shape,
                operation: "cannot select a field from a pointer type",
            }),
        }
    }

    /// Selects the nth field of a struct by index
    pub(crate) fn begin_nth_struct_field(
        frame: &mut Frame,
        struct_type: StructType,
        idx: usize,
    ) -> Result<Frame, ReflectError> {
        if idx >= struct_type.fields.len() {
            return Err(ReflectError::OperationFailed {
                shape: frame.shape,
                operation: "field index out of bounds",
            });
        }
        let field = &struct_type.fields[idx];

        if !matches!(frame.tracker, Tracker::Struct { .. }) {
            // When transitioning from Scalar (fully initialized) to Struct tracker,
            // we need to mark all fields as initialized in the iset. Otherwise,
            // we'll lose track of which fields were initialized and may double-free.
            let was_fully_init = frame.is_init && matches!(frame.tracker, Tracker::Scalar);
            let mut iset = ISet::new(struct_type.fields.len());
            if was_fully_init {
                iset.set_all();
            }
            frame.tracker = Tracker::Struct {
                iset,
                current_child: None,
            }
        }

        let was_field_init = match &mut frame.tracker {
            Tracker::Struct {
                iset,
                current_child,
            } => {
                *current_child = Some(idx);
                let was_init = iset.get(idx);
                iset.unset(idx); // Parent relinquishes responsibility
                was_init
            }
            _ => unreachable!(),
        };

        // Push a new frame for this field onto the frames stack.
        let field_ptr = unsafe { frame.data.field_uninit(field.offset) };
        let field_shape = field.shape();

        let mut next_frame = Frame::new(
            field_ptr,
            field_shape,
            FrameOwnership::Field { field_idx: idx },
        );
        if was_field_init {
            unsafe {
                // the struct field tracker said so!
                next_frame.mark_as_init();
            }
        }

        Ok(next_frame)
    }

    /// Selects the nth element of an array by index
    pub(crate) fn begin_nth_array_element(
        frame: &mut Frame,
        array_type: ArrayType,
        idx: usize,
    ) -> Result<Frame, ReflectError> {
        if idx >= array_type.n {
            return Err(ReflectError::OperationFailed {
                shape: frame.shape,
                operation: "array index out of bounds",
            });
        }

        if array_type.n > 63 {
            return Err(ReflectError::OperationFailed {
                shape: frame.shape,
                operation: "arrays larger than 63 elements are not yet supported",
            });
        }

        // Ensure frame is in Array state
        match &frame.tracker {
            Tracker::Scalar if !frame.is_init => {
                // this is fine, transition to Array tracker
                frame.tracker = Tracker::Array {
                    iset: ISet::default(),
                    current_child: None,
                };
            }
            Tracker::Array { .. } => {
                // fine too
            }
            _other => {
                return Err(ReflectError::OperationFailed {
                    shape: frame.shape,
                    operation: "unexpected tracker state: expected uninitialized Scalar or Array",
                });
            }
        }

        match &mut frame.tracker {
            Tracker::Array {
                iset,
                current_child,
            } => {
                *current_child = Some(idx);
                let was_field_init = iset.get(idx);
                iset.unset(idx); // Parent relinquishes responsibility

                // Calculate the offset for this array element
                let Ok(element_layout) = array_type.t.layout.sized_layout() else {
                    return Err(ReflectError::Unsized {
                        shape: array_type.t,
                        operation: "begin_nth_element, calculating array element offset",
                    });
                };
                let offset = element_layout.size() * idx;
                let element_data = unsafe { frame.data.field_uninit(offset) };

                let mut next_frame = Frame::new(
                    element_data,
                    array_type.t,
                    FrameOwnership::Field { field_idx: idx },
                );
                if was_field_init {
                    // safety: `iset` said it was initialized already
                    unsafe {
                        next_frame.mark_as_init();
                    }
                }
                Ok(next_frame)
            }
            _ => unreachable!(),
        }
    }

    /// Selects the nth field of an enum variant by index
    pub(crate) fn begin_nth_enum_field(
        frame: &mut Frame,
        variant: &'static Variant,
        idx: usize,
    ) -> Result<Frame, ReflectError> {
        if idx >= variant.data.fields.len() {
            return Err(ReflectError::OperationFailed {
                shape: frame.shape,
                operation: "enum field index out of bounds",
            });
        }

        let field = &variant.data.fields[idx];

        // Update tracker
        let was_field_init = match &mut frame.tracker {
            Tracker::Enum {
                data,
                current_child,
                ..
            } => {
                *current_child = Some(idx);
                let was_init = data.get(idx);
                data.unset(idx); // Parent relinquishes responsibility
                was_init
            }
            _ => {
                return Err(ReflectError::OperationFailed {
                    shape: frame.shape,
                    operation: "selecting a field on an enum requires selecting a variant first",
                });
            }
        };

        // SAFETY: the field offset comes from an unsafe impl of the Facet trait, we trust it.
        // also, we checked that the variant was selected.
        let field_ptr = unsafe { frame.data.field_uninit(field.offset) };
        let field_shape = field.shape();

        let mut next_frame = Frame::new(
            field_ptr,
            field_shape,
            FrameOwnership::Field { field_idx: idx },
        );
        if was_field_init {
            // SAFETY: `ISet` told us the field was initialized
            unsafe {
                next_frame.mark_as_init();
            }
        }

        Ok(next_frame)
    }

    /// Prepares the current frame for re-initialization by dropping any existing
    /// value and unmarking it in the parent's iset.
    ///
    /// This should be called at the start of `begin_*` methods that support
    /// re-initialization (e.g., `begin_some`, `begin_inner`, `begin_smart_ptr`).
    ///
    /// Returns `true` if cleanup was performed (frame was previously initialized),
    /// `false` if the frame was not initialized.
    pub(crate) fn prepare_for_reinitialization(&mut self) -> bool {
        let frame = self.frames().last().unwrap();

        // Check if there's anything to reinitialize:
        // - For Scalar tracker: check is_init flag
        // - For Struct/Array/Enum trackers: these may have initialized fields even if is_init is false
        //   (is_init tracks the whole value, iset/data tracks individual fields)
        let needs_cleanup = match &frame.tracker {
            Tracker::Scalar => frame.is_init,
            // Non-Scalar trackers indicate fields were accessed, so deinit() will handle them
            Tracker::Struct { .. }
            | Tracker::Array { .. }
            | Tracker::Enum { .. }
            | Tracker::SmartPointer
            | Tracker::SmartPointerSlice { .. }
            | Tracker::List { .. }
            | Tracker::Map { .. }
            | Tracker::Set { .. }
            | Tracker::Option { .. }
            | Tracker::Result { .. }
            | Tracker::DynamicValue { .. } => true,
        };
        if !needs_cleanup {
            return false;
        }

        // Use deinit() to properly handle:
        // - Scalar frames: drops the whole value if is_init
        // - Struct frames: only drops fields marked in iset (avoiding double-free)
        // - Array frames: only drops elements marked in iset
        // - Enum frames: only drops fields marked in data
        // - Map/Set frames: also cleans up partial insert state (key/value buffers)
        //
        // For ManagedElsewhere frames, skip deinit() entirely because:
        // 1. deinit() doesn't drop the value (parent owns it)
        // 2. deinit() resets is_init to false, which breaks our drop logic that relies
        //    on is_init to determine if the parent's key/value_initialized should be set
        // 3. begin_inner() pushes a new frame rather than replacing the current value,
        //    so we don't actually need to clean up - the parent will handle its entry
        let frame = self.frames_mut().last_mut().unwrap();
        if !matches!(frame.ownership, FrameOwnership::ManagedElsewhere) {
            frame.deinit();
        }

        true
    }
}
