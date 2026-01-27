use super::*;

////////////////////////////////////////////////////////////////////////////////////////////////////
// Build
////////////////////////////////////////////////////////////////////////////////////////////////////
impl<'facet, const BORROW: bool> Partial<'facet, BORROW> {
    /// Builds the value, consuming the Partial.
    pub fn build(mut self) -> Result<HeapValue<'facet, BORROW>, ReflectError> {
        if self.frames().len() != 1 {
            return Err(self.err(ReflectErrorKind::InvariantViolation {
                invariant: "Partial::build() expects a single frame — call end() until that's the case",
            }));
        }

        let frame = self.frames_mut().last_mut().unwrap();

        // Fill in defaults for any unset fields before checking initialization
        crate::trace!(
            "build(): calling fill_defaults for {}, tracker={:?}, is_init={}",
            frame.allocated.shape(),
            frame.tracker.kind(),
            frame.is_init
        );
        frame.fill_defaults().map_err(|e| self.err(e))?;
        crate::trace!(
            "build(): after fill_defaults, tracker={:?}, is_init={}",
            frame.tracker.kind(),
            frame.is_init
        );

        let frame = self.frames_mut().pop().unwrap();

        // Check initialization before proceeding
        crate::trace!(
            "build(): calling require_full_initialization, tracker={:?}",
            frame.tracker.kind()
        );
        let init_result = frame.require_full_initialization();
        crate::trace!(
            "build(): require_full_initialization returned {:?}",
            init_result.is_ok()
        );
        if let Err(e) = init_result {
            // Put the frame back so Drop can handle cleanup properly
            self.frames_mut().push(frame);
            return Err(self.err(e));
        }

        // Check invariants if present
        // Safety: The value is fully initialized at this point (we just checked with require_full_initialization)
        let value_ptr = unsafe { frame.data.assume_init().as_const() };
        if let Some(result) = unsafe { frame.allocated.shape().call_invariants(value_ptr) } {
            match result {
                Ok(()) => {
                    // Invariants passed
                }
                Err(message) => {
                    // Put the frame back so Drop can handle cleanup properly
                    let shape = frame.allocated.shape();
                    self.frames_mut().push(frame);
                    return Err(self.err(ReflectErrorKind::UserInvariantFailed { message, shape }));
                }
            }
        }

        // Mark as built to prevent Drop from cleaning up the value
        self.state = PartialState::Built;

        match frame
            .allocated
            .shape()
            .layout
            .sized_layout()
            .map_err(|_layout_err| {
                self.err(ReflectErrorKind::Unsized {
                    shape: frame.allocated.shape(),
                    operation: "build (final check for sized layout)",
                })
            }) {
            Ok(layout) => {
                // Determine if we should deallocate based on ownership
                let should_dealloc = frame.ownership.needs_dealloc();

                Ok(HeapValue {
                    guard: Some(Guard {
                        ptr: unsafe { NonNull::new_unchecked(frame.data.as_mut_byte_ptr()) },
                        layout,
                        should_dealloc,
                    }),
                    shape: frame.allocated.shape(),
                    phantom: PhantomData,
                })
            }
            Err(e) => {
                // Put the frame back for proper cleanup
                self.frames_mut().push(frame);
                Err(e)
            }
        }
    }

    /// Finishes deserialization in-place, validating the value without moving it.
    ///
    /// This is intended for use with [`from_raw`](Self::from_raw) where the value
    /// is deserialized into caller-provided memory (e.g., a `MaybeUninit<T>` on the stack).
    ///
    /// On success, the caller can safely assume the memory contains a fully initialized,
    /// valid value and call `MaybeUninit::assume_init()`.
    ///
    /// On failure, any partially initialized data is cleaned up (dropped), and the
    /// memory should be considered uninitialized.
    ///
    /// # Panics
    ///
    /// Panics if called with more than one frame on the stack (i.e., if you haven't
    /// called `end()` enough times to return to the root level).
    ///
    /// # Example
    ///
    /// ```ignore
    /// use std::mem::MaybeUninit;
    /// use facet_core::{Facet, PtrUninit};
    /// use facet_reflect::Partial;
    ///
    /// let mut slot = MaybeUninit::<MyStruct>::uninit();
    /// let ptr = PtrUninit::new(slot.as_mut_ptr().cast());
    ///
    /// let partial = unsafe { Partial::from_raw(ptr, MyStruct::SHAPE)? };
    /// // ... deserialize into partial ...
    /// partial.finish_in_place()?;
    ///
    /// // Now safe to assume initialized
    /// let value = unsafe { slot.assume_init() };
    /// ```
    pub fn finish_in_place(mut self) -> Result<(), ReflectError> {
        if self.frames().len() != 1 {
            return Err(self.err(ReflectErrorKind::InvariantViolation {
                invariant: "Partial::finish_in_place() expects a single frame — call end() until that's the case",
            }));
        }

        let frame = self.frames_mut().last_mut().unwrap();

        // Fill in defaults for any unset fields before checking initialization
        crate::trace!(
            "finish_in_place(): calling fill_defaults for {}, tracker={:?}, is_init={}",
            frame.allocated.shape(),
            frame.tracker.kind(),
            frame.is_init
        );
        frame.fill_defaults().map_err(|e| self.err(e))?;
        crate::trace!(
            "finish_in_place(): after fill_defaults, tracker={:?}, is_init={}",
            frame.tracker.kind(),
            frame.is_init
        );

        let frame = self.frames_mut().pop().unwrap();

        // Check initialization before proceeding
        crate::trace!(
            "finish_in_place(): calling require_full_initialization, tracker={:?}",
            frame.tracker.kind()
        );
        let init_result = frame.require_full_initialization();
        crate::trace!(
            "finish_in_place(): require_full_initialization returned {:?}",
            init_result.is_ok()
        );
        if let Err(e) = init_result {
            // Put the frame back so Drop can handle cleanup properly
            self.frames_mut().push(frame);
            return Err(self.err(e));
        }

        // Check invariants if present
        // Safety: The value is fully initialized at this point (we just checked with require_full_initialization)
        let value_ptr = unsafe { frame.data.assume_init().as_const() };
        if let Some(result) = unsafe { frame.allocated.shape().call_invariants(value_ptr) } {
            match result {
                Ok(()) => {
                    // Invariants passed
                }
                Err(message) => {
                    // Put the frame back so Drop can handle cleanup properly
                    let shape = frame.allocated.shape();
                    self.frames_mut().push(frame);
                    return Err(self.err(ReflectErrorKind::UserInvariantFailed { message, shape }));
                }
            }
        }

        // Mark as built to prevent Drop from cleaning up the now-valid value.
        // The caller owns the memory and will handle the value from here.
        self.state = PartialState::Built;

        // Frame is dropped here without deallocation (External ownership doesn't dealloc)
        Ok(())
    }
}
