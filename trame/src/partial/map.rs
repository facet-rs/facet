//! Map frame handling for HashMap, BTreeMap, and similar types.
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
//!                               │ • map uninit │
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
//!             │                │   entries    │    Append       │
//!             │                └──────┬───────┘         │       │
//!             │                       │                 │       │
//!             │                       │ End             │       │
//!             │                       ▼                 │       │
//!             │                ┌──────────────┐         │       │
//!             └───────────────►│   COMPLETE   │◄────────┴───────┘
//!                              │              │
//!                              │ • slab=None  │
//!                              │ • map built  │
//!                              │   (from_pair │
//!                              │    _slice)   │
//!                              └──────────────┘
//! ```
//!
//! # Slab Pattern
//!
//! Maps use a two-phase construction:
//! 1. **Staging**: Collect `(K, V)` tuples in a temporary slab
//! 2. **Finalization**: Call `from_pair_slice` to build the actual map
//!
//! This avoids per-entry hash lookups and enables optimal initial capacity.

use crate::arena::Idx;
use crate::errors::{ReflectError, ReflectErrorKind};
use crate::frame::{Frame, FrameFlags, FrameKind, MapFrame, ParentLink};
use crate::ops::Source;
use crate::shape_desc::ShapeDesc;
use crate::slab::Slab;
use facet_core::MapDef;

use super::Partial;

// ============================================================================
// State
// ============================================================================

/// The build state of a map frame.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MapState {
    /// No slab created yet. Map memory is uninitialized.
    Pending,
    /// Slab exists, actively collecting entries.
    Staged,
    /// Map is built (via End calling from_pair_slice) or set via Imm/Default.
    Complete,
}

impl MapFrame {
    /// Get the current build state of this map frame.
    pub fn state(&self, frame_init: bool) -> MapState {
        if self.slab.is_some() {
            MapState::Staged
        } else if frame_init {
            MapState::Complete
        } else {
            MapState::Pending
        }
    }
}

// ============================================================================
// Operations
// ============================================================================

impl<'facet> Partial<'facet> {
    /// Ensure the map has a slab for collecting entries.
    ///
    /// Transitions: PENDING → STAGED
    pub(crate) fn map_ensure_staged(
        &mut self,
        capacity: Option<usize>,
    ) -> Result<(), ReflectError> {
        let frame = self.arena.get(self.current);
        let FrameKind::Map(ref map_frame) = frame.kind else {
            return Ok(());
        };

        if map_frame.slab.is_some() {
            return Ok(()); // Already staged
        }

        // Create slab to collect (K, V) tuples
        let entry_shape = crate::tuple2(&map_frame.def);
        let slab = Slab::new(ShapeDesc::Tuple2(entry_shape), capacity);

        let frame = self.arena.get_mut(self.current);
        if let FrameKind::Map(ref mut m) = frame.kind {
            m.slab = Some(slab);
        }
        // Note: Do NOT set INIT flag - map memory is still uninitialized

        Ok(())
    }

    /// Finalize the map by calling from_pair_slice.
    ///
    /// Transitions: STAGED → COMPLETE
    pub(crate) fn map_finalize(&mut self) -> Result<(), ReflectError> {
        let frame = self.arena.get(self.current);
        let FrameKind::Map(ref map_frame) = frame.kind else {
            return Ok(()); // Not a map, nothing to do
        };

        // Check if there's a slab to finalize
        if map_frame.slab.is_none() {
            return Ok(()); // No slab = no entries were staged
        }

        // Get from_pair_slice function
        let from_pair_slice = map_frame.def.vtable.from_pair_slice.ok_or_else(|| {
            self.error(ReflectErrorKind::MapDoesNotSupportFromPairSlice { shape: frame.shape })
        })?;

        // Get the data we need before taking the slab
        let map_ptr = frame.data;
        let len = map_frame.len;

        // Take the slab out of the frame
        let frame = self.arena.get_mut(self.current);
        let FrameKind::Map(ref mut map_frame) = frame.kind else {
            unreachable!()
        };
        let slab = map_frame.slab.take().expect("slab exists");
        let pairs_ptr = slab.as_mut_ptr();

        // Build the map using from_pair_slice
        // SAFETY: map_ptr is uninitialized memory, pairs_ptr points to len initialized entries
        unsafe {
            from_pair_slice(map_ptr, pairs_ptr, len);
        }

        // Slab is dropped here - deallocates buffer but doesn't drop elements
        // (elements were moved out by from_pair_slice)
        drop(slab);

        // Mark map as initialized
        let frame = self.arena.get_mut(self.current);
        frame.flags |= FrameFlags::INIT;

        Ok(())
    }

    /// Handle Append operation on a map.
    ///
    /// Transitions: PENDING → STAGED (via ensure_staged), STAGED → STAGED
    /// Error from: COMPLETE
    pub(crate) fn map_append(&mut self, source: &Source<'_>) -> Result<(), ReflectError> {
        let frame = self.arena.get(self.current);
        let FrameKind::Map(ref map_frame) = frame.kind else {
            unreachable!()
        };

        // Check state - cannot append to complete map
        let state = map_frame.state(frame.flags.contains(FrameFlags::INIT));
        if state == MapState::Complete {
            return Err(self.error(ReflectErrorKind::CannotAppendToCompleteCollection));
        }

        // Ensure slab exists
        if state == MapState::Pending {
            self.map_ensure_staged(None)?;
        }

        // Get map def and current entry count
        let frame = self.arena.get(self.current);
        let FrameKind::Map(ref map_frame) = frame.kind else {
            unreachable!()
        };
        let map_def = map_frame.def;
        let current_len = map_frame.len;

        match source {
            Source::Imm(imm) => {
                self.map_append_imm(&map_def, current_len, imm)?;
            }
            Source::Stage(_capacity) => {
                self.map_append_stage(&map_def, current_len)?;
            }
            Source::Default => {
                // Default doesn't make sense for map entries
                return Err(self.error(ReflectErrorKind::MapAppendRequiresStage));
            }
        }

        Ok(())
    }

    /// Handle completion of a MapEntry child frame.
    pub(crate) fn map_entry_end(&mut self, parent_idx: Idx<Frame>) -> Result<(), ReflectError> {
        // Entry is in slab - free frame and increment parent's len
        let _ = self.arena.free(self.current);

        let parent = self.arena.get_mut(parent_idx);
        if let FrameKind::Map(ref mut m) = parent.kind {
            m.len += 1;
        }

        self.current = parent_idx;
        Ok(())
    }

    /// Handle completion of a MapEntryField child frame.
    pub(crate) fn map_entry_field_end(
        &mut self,
        parent_idx: Idx<Frame>,
        field_idx: u32,
    ) -> Result<(), ReflectError> {
        // Free the current frame and mark the field complete in parent entry
        let _ = self.arena.free(self.current);

        let parent = self.arena.get_mut(parent_idx);
        if let FrameKind::MapEntry(ref mut e) = parent.kind {
            e.mark_field_complete(field_idx as usize);
        }

        self.current = parent_idx;
        Ok(())
    }

    /// Update MapFrame state after Set at empty path with Imm/Default.
    pub(crate) fn map_sync_after_set(&mut self) {
        let frame = self.arena.get_mut(self.current);
        if let FrameKind::Map(ref mut m) = frame.kind {
            let map_ptr = unsafe { frame.data.assume_init() };
            m.len = unsafe { (m.def.vtable.len)(map_ptr.as_const()) };
            // Map is now fully initialized, no slab needed
            m.slab = None;
        }
    }

    // ========================================================================
    // Append helpers
    // ========================================================================

    fn map_append_imm(
        &mut self,
        map_def: &MapDef,
        current_len: usize,
        imm: &crate::ops::Imm<'_>,
    ) -> Result<(), ReflectError> {
        // Verify the source shape matches our expected tuple shape
        let entry_shape = crate::tuple2(map_def);
        if !ShapeDesc::Tuple2(entry_shape).is_shape(ShapeDesc::Static(imm.shape())) {
            return Err(self.error(ReflectErrorKind::ShapeMismatch {
                expected: ShapeDesc::Tuple2(entry_shape),
                actual: ShapeDesc::Static(imm.shape()),
            }));
        }

        // Get a slot from the slab for this entry
        let frame = self.arena.get_mut(self.current);
        let FrameKind::Map(ref mut map_frame) = frame.kind else {
            unreachable!()
        };
        let slab = map_frame
            .slab
            .as_mut()
            .expect("slab must exist after map_ensure_staged");
        let entry_ptr = slab.nth_slot(current_len);

        // Copy the tuple directly into the slab slot
        // SAFETY: entry_ptr points to uninitialized memory of correct size,
        // imm.ptr() points to valid (K, V) tuple
        unsafe {
            entry_ptr.copy_from(imm.ptr(), imm.shape()).unwrap();
        }

        // Increment entry count
        let frame = self.arena.get_mut(self.current);
        let FrameKind::Map(ref mut map_frame) = frame.kind else {
            unreachable!()
        };
        map_frame.len += 1;

        Ok(())
    }

    fn map_append_stage(
        &mut self,
        map_def: &MapDef,
        current_len: usize,
    ) -> Result<(), ReflectError> {
        // Get a slot from the slab for this entry
        let frame = self.arena.get_mut(self.current);
        let FrameKind::Map(ref mut map_frame) = frame.kind else {
            unreachable!()
        };
        let slab = map_frame
            .slab
            .as_mut()
            .expect("slab must exist after map_ensure_staged");
        let entry_ptr = slab.nth_slot(current_len);

        // Create MapEntryFrame pointing into the slab
        // Note: Do NOT set OWNS_ALLOC - the slab owns this memory
        let entry_shape = crate::tuple2(map_def);
        let mut entry_frame =
            Frame::new_map_entry(entry_ptr, ShapeDesc::Tuple2(entry_shape), *map_def);
        entry_frame.parent_link = ParentLink::MapEntry {
            parent: self.current,
        };

        // Push frame and make it current
        let entry_idx = self.arena.alloc(entry_frame);
        self.current = entry_idx;

        Ok(())
    }
}
