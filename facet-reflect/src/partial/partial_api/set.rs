use super::*;
use facet_core::{Def, DynDateTimeKind, NumericType, PrimitiveType, Type};

////////////////////////////////////////////////////////////////////////////////////////////////////
// `Set` and set helpers
////////////////////////////////////////////////////////////////////////////////////////////////////
impl<'facet, const BORROW: bool> Partial<'facet, BORROW> {
    /// Sets a value wholesale into the current frame.
    ///
    /// If the current frame was already initialized, the previous value is
    /// dropped. If it was partially initialized, the fields that were initialized
    /// are dropped, etc.
    pub fn set<U>(mut self, value: U) -> Result<Self, ReflectError>
    where
        U: Facet<'facet>,
    {
        struct DropVal<U> {
            ptr: *mut U,
        }
        impl<U> Drop for DropVal<U> {
            #[inline]
            fn drop(&mut self) {
                unsafe { core::ptr::drop_in_place(self.ptr) };
            }
        }

        let mut value = ManuallyDrop::new(value);
        let drop = DropVal {
            ptr: (&mut value) as *mut ManuallyDrop<U> as *mut U,
        };

        let ptr_const = PtrConst::new(drop.ptr);
        // Safety: We are calling set_shape with a valid shape and a valid pointer
        self = unsafe { self.set_shape(ptr_const, U::SHAPE)? };
        core::mem::forget(drop);

        Ok(self)
    }

    /// Sets a value into the current frame by [PtrConst] / [Shape].
    ///
    /// # Safety
    ///
    /// The caller must ensure that `src_value` points to a valid instance of a value
    /// whose memory layout and type matches `src_shape`, and that this value can be
    /// safely copied (bitwise) into the destination specified by the Partial's current frame.
    ///
    /// After a successful call, the ownership of the value at `src_value` is effectively moved
    /// into the Partial (i.e., the destination), and the original value should not be used
    /// or dropped by the caller; you should use `core::mem::forget` on the passed value.
    ///
    /// If an error is returned, the destination remains unmodified and safe for future operations.
    #[inline]
    pub unsafe fn set_shape(
        mut self,
        src_value: PtrConst,
        src_shape: &'static Shape,
    ) -> Result<Self, ReflectError> {
        let fr = self.frames_mut().last_mut().unwrap();
        crate::trace!("set_shape({src_shape:?})");

        // Check if target is a DynamicValue - if so, convert the source value
        if let Def::DynamicValue(dyn_def) = &fr.shape.def {
            return unsafe { self.set_into_dynamic_value(src_value, src_shape, dyn_def) };
        }

        if !fr.shape.is_shape(src_shape) {
            return Err(ReflectError::WrongShape {
                expected: fr.shape,
                actual: src_shape,
            });
        }

        // Special case: if this is a ManagedElsewhere frame and it's initialized,
        // we need to drop the old value before replacing it (same reason as in set_into_dynamic_value)
        if matches!(fr.ownership, FrameOwnership::ManagedElsewhere) && fr.is_init {
            unsafe { fr.shape.call_drop_in_place(fr.data.assume_init()) };
        }

        fr.deinit();

        // SAFETY: `fr.shape` and `src_shape` are the same, so they have the same size,
        // and the preconditions for this function are that `src_value` is fully intialized.
        unsafe {
            // unwrap safety: the only failure condition for copy_from is that shape is unsized,
            // which is not possible for `Partial`
            fr.data.copy_from(src_value, fr.shape).unwrap();
        }

        // SAFETY: if we reached this point, `fr.data` is correctly initialized
        unsafe {
            fr.mark_as_init();
        }

        Ok(self)
    }

    /// Sets a value into a DynamicValue target by converting the source value.
    ///
    /// # Safety
    ///
    /// Same safety requirements as `set_shape`.
    unsafe fn set_into_dynamic_value(
        mut self,
        src_value: PtrConst,
        src_shape: &'static Shape,
        dyn_def: &facet_core::DynamicValueDef,
    ) -> Result<Self, ReflectError> {
        let fr = self.frames_mut().last_mut().unwrap();
        let vtable = dyn_def.vtable;

        // Special case: if this is a ManagedElsewhere frame (pointing into parent object)
        // and it's initialized, we need to drop the old value before replacing it.
        // deinit() normally skips dropping ManagedElsewhere to avoid double-free,
        // but when we're explicitly replacing via set(), we own that operation.
        if matches!(fr.ownership, FrameOwnership::ManagedElsewhere) && fr.is_init {
            unsafe { fr.shape.call_drop_in_place(fr.data.assume_init()) };
        }

        fr.deinit();

        // If source shape is also the same DynamicValue shape, just copy it
        if fr.shape.is_shape(src_shape) {
            unsafe {
                fr.data.copy_from(src_value, fr.shape).unwrap();
                fr.mark_as_init();
            }
            return Ok(self);
        }

        // Get the size in bits for numeric conversions
        let size_bits = src_shape
            .layout
            .sized_layout()
            .map(|l| l.size() * 8)
            .unwrap_or(0);

        // Convert based on source shape's type
        match &src_shape.ty {
            Type::Primitive(PrimitiveType::Boolean) => {
                let val = unsafe { *(src_value.as_byte_ptr() as *const bool) };
                unsafe { (vtable.set_bool)(fr.data, val) };
            }
            Type::Primitive(PrimitiveType::Numeric(NumericType::Float)) => {
                if size_bits == 64 {
                    let val = unsafe { *(src_value.as_byte_ptr() as *const f64) };
                    let success = unsafe { (vtable.set_f64)(fr.data, val) };
                    if !success {
                        return Err(ReflectError::OperationFailed {
                            shape: src_shape,
                            operation: "f64 value (NaN/Infinity) not representable in dynamic value",
                        });
                    }
                } else if size_bits == 32 {
                    let val = unsafe { *(src_value.as_byte_ptr() as *const f32) } as f64;
                    let success = unsafe { (vtable.set_f64)(fr.data, val) };
                    if !success {
                        return Err(ReflectError::OperationFailed {
                            shape: src_shape,
                            operation: "f32 value (NaN/Infinity) not representable in dynamic value",
                        });
                    }
                } else {
                    return Err(ReflectError::OperationFailed {
                        shape: src_shape,
                        operation: "unsupported float size for dynamic value",
                    });
                }
            }
            Type::Primitive(PrimitiveType::Numeric(NumericType::Integer { signed: true })) => {
                let val: i64 = match size_bits {
                    8 => (unsafe { *(src_value.as_byte_ptr() as *const i8) }) as i64,
                    16 => (unsafe { *(src_value.as_byte_ptr() as *const i16) }) as i64,
                    32 => (unsafe { *(src_value.as_byte_ptr() as *const i32) }) as i64,
                    64 => unsafe { *(src_value.as_byte_ptr() as *const i64) },
                    _ => {
                        return Err(ReflectError::OperationFailed {
                            shape: src_shape,
                            operation: "unsupported signed integer size for dynamic value",
                        });
                    }
                };
                unsafe { (vtable.set_i64)(fr.data, val) };
            }
            Type::Primitive(PrimitiveType::Numeric(NumericType::Integer { signed: false })) => {
                let val: u64 = match size_bits {
                    8 => (unsafe { *src_value.as_byte_ptr() }) as u64,
                    16 => (unsafe { *(src_value.as_byte_ptr() as *const u16) }) as u64,
                    32 => (unsafe { *(src_value.as_byte_ptr() as *const u32) }) as u64,
                    64 => unsafe { *(src_value.as_byte_ptr() as *const u64) },
                    _ => {
                        return Err(ReflectError::OperationFailed {
                            shape: src_shape,
                            operation: "unsupported unsigned integer size for dynamic value",
                        });
                    }
                };
                unsafe { (vtable.set_u64)(fr.data, val) };
            }
            Type::Primitive(PrimitiveType::Textual(_)) => {
                // char or str - for char, convert to string
                if src_shape.type_identifier == "char" {
                    let c = unsafe { *(src_value.as_byte_ptr() as *const char) };
                    let mut buf = [0u8; 4];
                    let s = c.encode_utf8(&mut buf);
                    unsafe { (vtable.set_str)(fr.data, s) };
                } else {
                    // &str
                    let s: &str = unsafe { *(src_value.as_byte_ptr() as *const &str) };
                    unsafe { (vtable.set_str)(fr.data, s) };
                }
            }
            _ => {
                // Handle String type (not a primitive but common)
                if src_shape.type_identifier == "String" {
                    let s: &::alloc::string::String =
                        unsafe { &*(src_value.as_byte_ptr() as *const ::alloc::string::String) };
                    unsafe { (vtable.set_str)(fr.data, s.as_str()) };
                    // Drop the source String since we cloned its content
                    unsafe {
                        src_shape
                            .call_drop_in_place(PtrMut::new(src_value.as_byte_ptr() as *mut u8));
                    }
                } else {
                    return Err(ReflectError::OperationFailed {
                        shape: src_shape,
                        operation: "cannot convert this type to dynamic value",
                    });
                }
            }
        }

        let fr = self.frames_mut().last_mut().unwrap();
        fr.tracker = Tracker::DynamicValue {
            state: DynamicValueState::Scalar,
        };
        unsafe { fr.mark_as_init() };
        Ok(self)
    }

    /// Sets a datetime value into a DynamicValue target.
    ///
    /// This is used for format-specific datetime types (like TOML datetime).
    /// Returns an error if the target doesn't support datetime values.
    #[allow(clippy::too_many_arguments)]
    pub fn set_datetime(
        mut self,
        year: i32,
        month: u8,
        day: u8,
        hour: u8,
        minute: u8,
        second: u8,
        nanos: u32,
        kind: DynDateTimeKind,
    ) -> Result<Self, ReflectError> {
        let fr = self.frames_mut().last_mut().unwrap();

        // Must be a DynamicValue type
        let dyn_def = match &fr.shape.def {
            Def::DynamicValue(dv) => dv,
            _ => {
                return Err(ReflectError::OperationFailed {
                    shape: fr.shape,
                    operation: "set_datetime requires a DynamicValue target",
                });
            }
        };

        let vtable = dyn_def.vtable;

        // Check if the vtable supports datetime
        let Some(set_datetime_fn) = vtable.set_datetime else {
            return Err(ReflectError::OperationFailed {
                shape: fr.shape,
                operation: "dynamic value type does not support datetime",
            });
        };

        fr.deinit();

        // Call the vtable's set_datetime function
        unsafe {
            set_datetime_fn(fr.data, year, month, day, hour, minute, second, nanos, kind);
        }

        let fr = self.frames_mut().last_mut().unwrap();
        fr.tracker = Tracker::DynamicValue {
            state: DynamicValueState::Scalar,
        };
        unsafe { fr.mark_as_init() };
        Ok(self)
    }

    /// Sets the current frame using a function that initializes the value
    ///
    /// # Safety
    ///
    /// If `f` returns Ok(), it is assumed that it initialized the passed pointer fully and with a
    /// value of the right type.
    ///
    /// If `f` returns Err(), it is assumed that it did NOT initialize the passed pointer and that
    /// there is no need to drop it in place.
    pub unsafe fn set_from_function<F>(mut self, f: F) -> Result<Self, ReflectError>
    where
        F: FnOnce(PtrUninit) -> Result<(), ReflectError>,
    {
        let frame = self.frames_mut().last_mut().unwrap();

        // Special case: if this is a ManagedElsewhere frame and it's initialized,
        // we need to drop the old value before replacing it.
        // deinit() normally skips dropping ManagedElsewhere to avoid double-free,
        // but when we're explicitly replacing via set_from_function(), we own that operation.
        if matches!(frame.ownership, FrameOwnership::ManagedElsewhere) && frame.is_init {
            unsafe { frame.shape.call_drop_in_place(frame.data.assume_init()) };
        }

        frame.deinit();
        f(frame.data)?;

        // safety: `f()` returned Ok, so `frame.data` must be initialized
        unsafe {
            frame.mark_as_init();
        }

        Ok(self)
    }

    /// Sets the current frame to its default value using `default_in_place` from the
    /// vtable.
    ///
    /// Note: if you have `struct S { field: F }`, and `F` does not implement `Default`
    /// but `S` does, this doesn't magically uses S's `Default` implementation to get a value
    /// for `field`.
    ///
    /// If the current frame's shape does not implement `Default`, then this returns an error.
    #[inline]
    pub fn set_default(self) -> Result<Self, ReflectError> {
        let frame = self.frames().last().unwrap();
        let shape = frame.shape;

        // SAFETY: `call_default_in_place` fully initializes the passed pointer.
        unsafe {
            self.set_from_function(move |ptr| {
                shape.call_default_in_place(ptr.assume_init()).ok_or(
                    ReflectError::OperationFailed {
                        shape,
                        operation: "type does not implement Default",
                    },
                )?;
                Ok(())
            })
        }
    }

    /// Copy a value from a Peek into the current frame.
    ///
    /// # Invariants
    ///
    /// `peek` must be a thin pointer, otherwise this panics.
    ///
    /// # Safety
    ///
    /// If this succeeds, the value `Peek` points to has been moved out of, and
    /// as such, should not be dropped (but should be deallocated).
    pub unsafe fn set_from_peek(self, peek: &Peek<'_, '_>) -> Result<Self, ReflectError> {
        // Get the source value's pointer and shape
        let src_ptr = peek.data();
        let src_shape = peek.shape();

        // SAFETY: `Peek` guarantees that src_ptr is initialized and of type src_shape
        unsafe { self.set_shape(src_ptr, src_shape) }
    }

    /// Parses a string value into the current frame using the type's ParseFn from the vtable.
    ///
    /// If the current frame was previously initialized, its contents are dropped in place.
    pub fn parse_from_str(mut self, s: &str) -> Result<Self, ReflectError> {
        let frame = self.frames_mut().last_mut().unwrap();
        let shape = frame.shape;

        // Note: deinit leaves us in `Tracker::Uninit` state which is valid even if we error out.
        frame.deinit();

        // Parse the string value using the type's parse function
        let result = unsafe { shape.call_parse(s, frame.data.assume_init()) };

        match result {
            Some(Ok(())) => {
                // SAFETY: `call_parse` returned `Ok`, so `frame.data` is fully initialized now.
                unsafe {
                    frame.mark_as_init();
                }
                Ok(self)
            }
            Some(Err(_pe)) => {
                // TODO: can we propagate the ParseError somehow?
                Err(ReflectError::OperationFailed {
                    shape,
                    operation: "Failed to parse string value",
                })
            }
            None => Err(ReflectError::OperationFailed {
                shape,
                operation: "Type does not support parsing from string",
            }),
        }
    }

    /// Parses a byte slice into the current frame using the type's ParseBytesFn from the vtable.
    ///
    /// This is used for binary formats where types have efficient binary representations
    /// (e.g., UUID as 16 raw bytes instead of a string).
    ///
    /// If the current frame was previously initialized, its contents are dropped in place.
    pub fn parse_from_bytes(mut self, bytes: &[u8]) -> Result<Self, ReflectError> {
        let frame = self.frames_mut().last_mut().unwrap();
        let shape = frame.shape;

        // Note: deinit leaves us in `Tracker::Uninit` state which is valid even if we error out.
        frame.deinit();

        // Parse the bytes using the type's parse_bytes function
        let result = unsafe { shape.call_parse_bytes(bytes, frame.data.assume_init()) };

        match result {
            Some(Ok(())) => {
                // SAFETY: `call_parse_bytes` returned `Ok`, so `frame.data` is fully initialized.
                unsafe {
                    frame.mark_as_init();
                }
                Ok(self)
            }
            Some(Err(_pe)) => {
                // TODO: can we propagate the ParseError somehow?
                Err(ReflectError::OperationFailed {
                    shape,
                    operation: "Failed to parse bytes value",
                })
            }
            None => Err(ReflectError::OperationFailed {
                shape,
                operation: "Type does not support parsing from bytes",
            }),
        }
    }
}
