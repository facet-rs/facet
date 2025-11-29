use super::*;

////////////////////////////////////////////////////////////////////////////////////////////////////
// Build
////////////////////////////////////////////////////////////////////////////////////////////////////
impl<'facet> Partial<'facet> {
    /// Builds the value, consuming the Partial.
    pub fn build(mut self) -> Result<HeapValue<'facet>, ReflectError> {
        if self.frames().len() != 1 {
            return Err(ReflectError::InvariantViolation {
                invariant: "Partial::build() expects a single frame — call end() until that's the case",
            });
        }

        let frame = self.frames_mut().pop().unwrap();

        // Check initialization before proceeding
        if let Err(e) = frame.require_full_initialization() {
            // Put the frame back so Drop can handle cleanup properly
            self.frames_mut().push(frame);
            return Err(e);
        }

        // Check invariants if present
        if let Some(invariants_fn) = frame.shape.vtable.invariants {
            // Safety: The value is fully initialized at this point (we just checked with require_full_initialization)
            let value_ptr = unsafe { frame.data.assume_init().as_const() };
            let invariants_ok = unsafe { invariants_fn(value_ptr) };

            if !invariants_ok {
                // Put the frame back so Drop can handle cleanup properly
                self.frames_mut().push(frame);
                return Err(ReflectError::InvariantViolation {
                    invariant: "Type invariants check failed",
                });
            }
        }

        // Mark as built to prevent Drop from cleaning up the value
        self.state = PartialState::Built;

        match frame
            .shape
            .layout
            .sized_layout()
            .map_err(|_layout_err| ReflectError::Unsized {
                shape: frame.shape,
                operation: "build (final check for sized layout)",
            }) {
            Ok(layout) => Ok(HeapValue {
                guard: Some(Guard {
                    ptr: unsafe { NonNull::new_unchecked(frame.data.as_mut_byte_ptr()) },
                    layout,
                }),
                shape: frame.shape,
                phantom: PhantomData,
            }),
            Err(e) => {
                // Put the frame back for proper cleanup
                self.frames_mut().push(frame);
                Err(e)
            }
        }
    }
}
