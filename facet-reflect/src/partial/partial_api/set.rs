use super::*;

////////////////////////////////////////////////////////////////////////////////////////////////////
// `Set` and set helpers
////////////////////////////////////////////////////////////////////////////////////////////////////
impl<'facet> Partial<'facet> {
    /// Sets a value wholesale into the current frame.
    ///
    /// If the current frame was already initialized, the previous value is
    /// dropped. If it was partially initialized, the fields that were initialized
    /// are dropped, etc.
    pub fn set<U>(&mut self, value: U) -> Result<&mut Self, ReflectError>
    where
        U: Facet<'facet>,
    {
        self.require_active()?;
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

        let ptr_const = PtrConst::new(unsafe { NonNull::new_unchecked(drop.ptr) });
        unsafe {
            // Safety: We are calling set_shape with a valid shape and a valid pointer
            self.set_shape(ptr_const, U::SHAPE)?
        };
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
        &mut self,
        src_value: PtrConst<'_>,
        src_shape: &'static Shape,
    ) -> Result<&mut Self, ReflectError> {
        self.require_active()?;

        let fr = self.frames_mut().last_mut().unwrap();
        crate::trace!("set_shape({src_shape:?})");

        if !fr.shape.is_shape(src_shape) {
            return Err(ReflectError::WrongShape {
                expected: fr.shape,
                actual: src_shape,
            });
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

    /// Sets the current frame using a function that initializes the value
    ///
    /// # Safety
    ///
    /// If `f` returns Ok(), it is assumed that it initialized the passed pointer fully and with a
    /// value of the right type.
    ///
    /// If `f` returns Err(), it is assumed that it did NOT initialize the passed pointer and that
    /// there is no need to drop it in place.
    pub unsafe fn set_from_function<F>(&mut self, f: F) -> Result<&mut Self, ReflectError>
    where
        F: FnOnce(PtrUninit<'_>) -> Result<(), ReflectError>,
    {
        self.require_active()?;
        let frame = self.frames_mut().last_mut().unwrap();

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
    pub fn set_default(&mut self) -> Result<&mut Self, ReflectError> {
        let frame = self.frames().last().unwrap();

        let Some(default_fn) = frame.shape.vtable.default_in_place else {
            return Err(ReflectError::OperationFailed {
                shape: frame.shape,
                operation: "type does not implement Default",
            });
        };

        // SAFETY: `default_fn` fully initializes the passed pointer. we took it
        // from the vtable of `frame.shape`.
        unsafe {
            self.set_from_function(move |ptr| {
                default_fn(ptr);
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
    pub unsafe fn set_from_peek(&mut self, peek: &Peek<'_, '_>) -> Result<&mut Self, ReflectError> {
        self.require_active()?;

        // Get the source value's pointer and shape
        let src_ptr = peek.data();
        let src_shape = peek.shape();

        // SAFETY: `Peek` guarantees that src_ptr is initialized and of type src_shape
        unsafe { self.set_shape(src_ptr, src_shape) }
    }

    /// Parses a string value into the current frame using the type's ParseFn from the vtable.
    ///
    /// If the current frame was previously initialized, its contents are dropped in place.
    pub fn parse_from_str(&mut self, s: &str) -> Result<&mut Self, ReflectError> {
        self.require_active()?;

        let frame = self.frames_mut().last_mut().unwrap();

        // Check if the type has a parse function
        let Some(parse_fn) = frame.shape.vtable.parse else {
            return Err(ReflectError::OperationFailed {
                shape: frame.shape,
                operation: "Type does not support parsing from string",
            });
        };

        // Note: deinit leaves us in `Tracker::Uninit` state which is valid even if we error out.
        frame.deinit();

        // Parse the string value using the type's parse function
        let result = unsafe { parse_fn(s, frame.data) };
        if let Err(_pe) = result {
            // TODO: can we propagate the ParseError somehow?
            return Err(ReflectError::OperationFailed {
                shape: frame.shape,
                operation: "Failed to parse string value",
            });
        }

        // SAFETY: `parse_fn` returned `Ok`, so `frame.data` is fully initialized now.
        unsafe {
            frame.mark_as_init();
        }
        Ok(self)
    }
}
