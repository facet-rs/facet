//! Set frame handling for HashSet, BTreeSet, and similar types.
//!
//! # State Machine
//!
//! ```text
//!                          alloc_shape / Set dst=[] src=Stage
//!                                       │
//!                                       ▼
//!                               ┌──────────────┐
//!                               │   PENDING    │
//!                               │              │
//!                               │ • slab=None  │
//!                               │ • set uninit │
//!                               └──────┬───────┘
//!                                      │
//!             ┌────────────────────────┼────────────────────────┐
//!             │                        │                        │
//!             │ Set dst=[]             │ Append                 │ Set dst=[]
//!             │ src=Imm/Default        │ (creates slab)         │ src=Imm/Default
//!             │                        ▼                        │
//!             │                ┌──────────────┐                 │
//!             │                │    STAGED    │                 │
//!             │                │              │                 │
//!             │                │ • slab=Some  │◄────────┐       │
//!             │                │ • collecting │         │       │
//!             │                │   elements   │    Append       │
//!             │                └──────┬───────┘         │       │
//!             │                       │                 │       │
//!             │                       │ End             │       │
//!             │                       ▼                 │       │
//!             │                ┌──────────────┐         │       │
//!             └───────────────►│   COMPLETE   │◄────────┴───────┘
//!                              │              │
//!                              │ • slab=None  │
//!                              │ • set built  │
//!                              │   (from_     │
//!                              │    slice)    │
//!                              └──────────────┘
//! ```
//!
//! # Slab Pattern
//!
//! Sets use a two-phase construction:
//! 1. **Staging**: Collect elements in a temporary slab
//! 2. **Finalization**: Call `from_slice` to build the actual set

use crate::arena::Idx;
use crate::errors::{ReflectError, ReflectErrorKind};
use crate::frame::{Frame, FrameFlags, FrameKind, ParentLink, SetFrame};
use crate::ops::Source;
use crate::shape_desc::ShapeDesc;
use crate::slab::Slab;

use super::Partial;

// ============================================================================
// State
// ============================================================================

/// The build state of a set frame.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SetState {
    /// No slab created yet. Set memory is uninitialized.
    Pending,
    /// Slab exists, actively collecting elements.
    Staged,
    /// Set is built (via End calling from_slice) or set via Imm/Default.
    Complete,
}

impl SetFrame {
    /// Get the current build state of this set frame.
    pub fn state(&self, frame_init: bool) -> SetState {
        if self.slab.is_some() {
            SetState::Staged
        } else if frame_init {
            SetState::Complete
        } else {
            SetState::Pending
        }
    }
}

// ============================================================================
// Operations
// ============================================================================

impl<'facet> Partial<'facet> {
    /// Ensure the set has a slab for collecting elements.
    ///
    /// Transitions: PENDING → STAGED
    pub(crate) fn set_ensure_staged(
        &mut self,
        capacity: Option<usize>,
    ) -> Result<(), ReflectError> {
        let frame = self.arena.get(self.current);
        let FrameKind::Set(ref set_frame) = frame.kind else {
            return Ok(());
        };

        if set_frame.slab.is_some() {
            return Ok(()); // Already staged
        }

        // Create slab to collect elements
        let element_shape = set_frame.def.t;
        let slab = Slab::new(ShapeDesc::Static(element_shape), capacity);

        let frame = self.arena.get_mut(self.current);
        if let FrameKind::Set(ref mut s) = frame.kind {
            s.slab = Some(slab);
        }
        // Note: Do NOT set INIT flag - set memory is still uninitialized

        Ok(())
    }

    /// Finalize the set by calling from_slice.
    ///
    /// Transitions: STAGED → COMPLETE
    pub(crate) fn set_finalize(&mut self) -> Result<(), ReflectError> {
        let frame = self.arena.get(self.current);
        let FrameKind::Set(ref set_frame) = frame.kind else {
            return Ok(()); // Not a set, nothing to do
        };

        // Check if there's a slab to finalize
        if set_frame.slab.is_none() {
            return Ok(()); // No slab = no elements were staged
        }

        // Get from_slice function
        let from_slice = set_frame.def.vtable.from_slice.ok_or_else(|| {
            self.error(ReflectErrorKind::SetDoesNotSupportFromSlice { shape: frame.shape })
        })?;

        // Get the data we need before taking the slab
        let set_ptr = frame.data;
        let len = set_frame.len;

        // Take the slab out of the frame
        let frame = self.arena.get_mut(self.current);
        let FrameKind::Set(ref mut set_frame) = frame.kind else {
            unreachable!()
        };
        let slab = set_frame.slab.take().expect("slab exists");
        let elements_ptr = slab.as_mut_ptr();

        // Build the set using from_slice
        // SAFETY: set_ptr is uninitialized memory, elements_ptr points to len initialized elements
        unsafe {
            from_slice(set_ptr, elements_ptr, len);
        }

        // Slab is dropped here - deallocates buffer but doesn't drop elements
        // (elements were moved out by from_slice)
        drop(slab);

        // Mark set as initialized
        let frame = self.arena.get_mut(self.current);
        frame.flags |= FrameFlags::INIT;

        Ok(())
    }

    /// Handle Append operation on a set.
    ///
    /// Transitions: PENDING → STAGED (via ensure_staged), STAGED → STAGED
    /// Error from: COMPLETE
    pub(crate) fn set_append(&mut self, source: &Source<'_>) -> Result<(), ReflectError> {
        let frame = self.arena.get(self.current);
        let FrameKind::Set(ref set_frame) = frame.kind else {
            unreachable!()
        };

        // Check state - cannot append to complete set
        let state = set_frame.state(frame.flags.contains(FrameFlags::INIT));
        if state == SetState::Complete {
            return Err(self.error(ReflectErrorKind::CannotAppendToCompleteCollection));
        }

        // Ensure slab exists
        if state == SetState::Pending {
            self.set_ensure_staged(None)?;
        }

        // Get set def and current element count
        let frame = self.arena.get(self.current);
        let FrameKind::Set(ref set_frame) = frame.kind else {
            unreachable!()
        };
        let set_def = set_frame.def;
        let current_len = set_frame.len;

        match source {
            Source::Imm(imm) => {
                self.set_append_imm(&set_def, current_len, imm)?;
            }
            Source::Stage(_capacity) => {
                self.set_append_stage(&set_def, current_len)?;
            }
            Source::Default => {
                self.set_append_default(&set_def, current_len)?;
            }
        }

        Ok(())
    }

    /// Handle completion of a SetElement child frame.
    pub(crate) fn set_element_end(&mut self, parent_idx: Idx<Frame>) -> Result<(), ReflectError> {
        // Element is in slab - free frame and increment parent's len
        let _ = self.arena.free(self.current);

        let parent = self.arena.get_mut(parent_idx);
        if let FrameKind::Set(ref mut s) = parent.kind {
            s.len += 1;
        }

        self.current = parent_idx;
        Ok(())
    }

    /// Update SetFrame state after Set at empty path with Imm/Default.
    pub(crate) fn set_sync_after_set(&mut self) {
        let frame = self.arena.get_mut(self.current);
        if let FrameKind::Set(ref mut s) = frame.kind {
            let set_ptr = unsafe { frame.data.assume_init() };
            s.len = unsafe { (s.def.vtable.len)(set_ptr.as_const()) };
            // Set is now fully initialized, no slab needed
            s.slab = None;
        }
    }

    // ========================================================================
    // Append helpers
    // ========================================================================

    fn set_append_imm(
        &mut self,
        set_def: &facet_core::SetDef,
        current_len: usize,
        imm: &crate::ops::Imm<'_>,
    ) -> Result<(), ReflectError> {
        // Verify the source shape matches our element shape
        let element_shape = set_def.t;
        if !element_shape.is_shape(imm.shape()) {
            return Err(self.error(ReflectErrorKind::ShapeMismatch {
                expected: ShapeDesc::Static(element_shape),
                actual: ShapeDesc::Static(imm.shape()),
            }));
        }

        // Get element size for copying
        let element_size = element_shape
            .layout
            .sized_layout()
            .map(|l| l.size())
            .unwrap_or(0);

        // Get a slot from the slab for this element
        let frame = self.arena.get_mut(self.current);
        let FrameKind::Set(ref mut set_frame) = frame.kind else {
            unreachable!()
        };
        let slab = set_frame
            .slab
            .as_mut()
            .expect("slab must exist after set_ensure_staged");
        let slot = slab.nth_slot(current_len);

        // Copy the element into the slab slot
        if element_size > 0 {
            unsafe {
                std::ptr::copy_nonoverlapping(
                    imm.ptr().as_byte_ptr(),
                    slot.as_mut_byte_ptr(),
                    element_size,
                );
            }
        }

        // Increment element count
        set_frame.len += 1;

        Ok(())
    }

    fn set_append_stage(
        &mut self,
        set_def: &facet_core::SetDef,
        current_len: usize,
    ) -> Result<(), ReflectError> {
        // Get a slot from the slab for this element
        let frame = self.arena.get_mut(self.current);
        let FrameKind::Set(ref mut set_frame) = frame.kind else {
            unreachable!()
        };
        let slab = set_frame
            .slab
            .as_mut()
            .expect("slab must exist after set_ensure_staged");
        let slot = slab.nth_slot(current_len);

        // Create frame pointing into the slab
        // Note: Do NOT set OWNS_ALLOC - the slab owns this memory
        let element_shape = set_def.t;
        let mut element_frame = Self::create_frame_for_shape(slot, element_shape);
        element_frame.parent_link = ParentLink::SetElement {
            parent: self.current,
        };

        // Push frame and make it current
        let element_idx = self.arena.alloc(element_frame);
        self.current = element_idx;

        Ok(())
    }

    fn set_append_default(
        &mut self,
        set_def: &facet_core::SetDef,
        current_len: usize,
    ) -> Result<(), ReflectError> {
        // Get a slot from the slab for this element
        let frame = self.arena.get_mut(self.current);
        let FrameKind::Set(ref mut set_frame) = frame.kind else {
            unreachable!()
        };
        let slab = set_frame
            .slab
            .as_mut()
            .expect("slab must exist after set_ensure_staged");
        let slot = slab.nth_slot(current_len);

        // Initialize with default directly into slab
        let element_shape = set_def.t;
        let ok = unsafe { element_shape.call_default_in_place(slot) };
        if ok.is_none() {
            return Err(self.error(ReflectErrorKind::NoDefault {
                shape: ShapeDesc::Static(element_shape),
            }));
        }

        // Increment element count
        set_frame.len += 1;

        Ok(())
    }
}
