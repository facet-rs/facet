//! Arena allocation with free list for frame storage.
//!
//! The arena stores frames using indices (`FrameId`) rather than pointers,
//! enabling efficient reuse of slots when frames complete. Sentinel values
//! (`NOT_STARTED` and `COMPLETE`) allow tracking frame state without
//! additional metadata.

use crate::frame::Frame;

/// Index into the frame arena.
///
/// Uses sentinel values for special states:
/// - `NOT_STARTED` (0): No frame exists, value not started
/// - `COMPLETE` (u32::MAX): Frame completed and freed, value is in place
/// - `1..MAX-1`: Valid arena index, frame in progress
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct FrameId(u32);

impl FrameId {
    /// Sentinel: no frame exists, value construction not started
    pub const NOT_STARTED: FrameId = FrameId(0);

    /// Sentinel: frame completed, value is in place
    pub const COMPLETE: FrameId = FrameId(u32::MAX);

    /// Returns true if this represents an unstarted value
    #[inline]
    pub fn is_not_started(self) -> bool {
        self.0 == 0
    }

    /// Returns true if this represents a completed value
    #[inline]
    pub fn is_complete(self) -> bool {
        self.0 == u32::MAX
    }

    /// Returns true if this is a valid arena index (frame in progress)
    #[inline]
    pub fn is_in_progress(self) -> bool {
        self.0 != 0 && self.0 != u32::MAX
    }

    /// Get the raw index. Only valid when `is_in_progress()` returns true.
    #[inline]
    fn index(self) -> usize {
        debug_assert!(
            self.is_in_progress(),
            "cannot get index of sentinel FrameId"
        );
        self.0 as usize
    }
}

/// Arena for frame allocation with slot reuse.
///
/// Freed slots are tracked in a free list and reused on subsequent allocations.
/// This prevents unbounded memory growth when constructing deeply nested values
/// where inner frames complete before outer frames.
///
/// # Invariants
///
/// - Slot 0 is never used (reserved for `NOT_STARTED` sentinel)
/// - Slots in the free list contain `None`
/// - Slots with live frames contain `Some(Frame)`
pub struct Arena {
    /// Frame storage. Slot 0 is always `None` (reserved).
    slots: Vec<Option<Frame>>,

    /// Indices of free slots available for reuse.
    free_list: Vec<u32>,
}

impl Arena {
    /// Create a new empty arena.
    pub fn new() -> Self {
        // Start with slot 0 reserved (for NOT_STARTED sentinel)
        Arena {
            slots: vec![None],
            free_list: Vec::new(),
        }
    }

    /// Allocate a new frame, returning its ID.
    ///
    /// Reuses a free slot if available, otherwise grows the arena.
    pub fn alloc(&mut self, frame: Frame) -> FrameId {
        if let Some(idx) = self.free_list.pop() {
            debug_assert!(
                self.slots[idx as usize].is_none(),
                "free slot was not empty"
            );
            self.slots[idx as usize] = Some(frame);
            FrameId(idx)
        } else {
            let idx = self.slots.len();
            // Ensure we don't collide with COMPLETE sentinel
            assert!(idx < u32::MAX as usize, "arena exceeded maximum capacity");
            self.slots.push(Some(frame));
            FrameId(idx as u32)
        }
    }

    /// Free a frame slot, returning it to the free list.
    ///
    /// The frame is dropped and the slot becomes available for reuse.
    ///
    /// # Panics
    ///
    /// Panics if `id` is a sentinel value or the slot is already empty.
    pub fn free(&mut self, id: FrameId) -> Frame {
        debug_assert!(id.is_in_progress(), "cannot free sentinel FrameId");
        let frame = self.slots[id.index()]
            .take()
            .expect("double-free of arena slot");
        self.free_list.push(id.0);
        frame
    }

    /// Get a reference to a frame.
    ///
    /// # Panics
    ///
    /// Panics if `id` is a sentinel or the slot is empty.
    #[inline]
    pub fn get(&self, id: FrameId) -> &Frame {
        debug_assert!(id.is_in_progress(), "cannot get sentinel FrameId");
        self.slots[id.index()]
            .as_ref()
            .expect("frame slot is empty")
    }

    /// Get a mutable reference to a frame.
    ///
    /// # Panics
    ///
    /// Panics if `id` is a sentinel or the slot is empty.
    #[inline]
    pub fn get_mut(&mut self, id: FrameId) -> &mut Frame {
        debug_assert!(id.is_in_progress(), "cannot get_mut sentinel FrameId");
        self.slots[id.index()]
            .as_mut()
            .expect("frame slot is empty")
    }

    /// Returns the number of currently allocated frames.
    pub fn live_count(&self) -> usize {
        self.slots.iter().filter(|s| s.is_some()).count()
    }

    /// Returns the total capacity (including free slots).
    pub fn capacity(&self) -> usize {
        self.slots.len()
    }
}

impl Default for Arena {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::frame::{Children, Frame, FrameFlags};
    use facet_core::{Facet, Shape};

    fn dummy_frame() -> Frame {
        Frame {
            parent: None,
            children: Children::None,
            data: facet_core::PtrUninit::dangling::<u32>(),
            shape: <u32 as Facet>::SHAPE,
            flags: FrameFlags::empty(),
        }
    }

    #[test]
    fn frame_id_sentinels() {
        assert!(FrameId::NOT_STARTED.is_not_started());
        assert!(!FrameId::NOT_STARTED.is_complete());
        assert!(!FrameId::NOT_STARTED.is_in_progress());

        assert!(!FrameId::COMPLETE.is_not_started());
        assert!(FrameId::COMPLETE.is_complete());
        assert!(!FrameId::COMPLETE.is_in_progress());

        let valid = FrameId(42);
        assert!(!valid.is_not_started());
        assert!(!valid.is_complete());
        assert!(valid.is_in_progress());
    }

    #[test]
    fn arena_alloc_and_get() {
        let mut arena = Arena::new();
        let id = arena.alloc(dummy_frame());

        assert!(id.is_in_progress());
        assert_eq!(arena.live_count(), 1);

        let frame = arena.get(id);
        assert_eq!(frame.shape, <u32 as Facet>::SHAPE);
    }

    #[test]
    fn arena_free_and_reuse() {
        let mut arena = Arena::new();

        let id1 = arena.alloc(dummy_frame());
        let id2 = arena.alloc(dummy_frame());
        assert_eq!(arena.live_count(), 2);

        arena.free(id1);
        assert_eq!(arena.live_count(), 1);

        // Next alloc should reuse the freed slot
        let id3 = arena.alloc(dummy_frame());
        assert_eq!(id3.0, id1.0, "should reuse freed slot");
        assert_eq!(arena.live_count(), 2);
        assert_eq!(arena.capacity(), 3); // slot 0 + 2 frames
    }

    #[test]
    #[should_panic(expected = "double-free")]
    fn arena_double_free_panics() {
        let mut arena = Arena::new();
        let id = arena.alloc(dummy_frame());
        arena.free(id);
        arena.free(id); // should panic
    }

    #[test]
    fn arena_multiple_alloc_free_cycles() {
        let mut arena = Arena::new();

        // Allocate several frames
        let ids: Vec<_> = (0..5).map(|_| arena.alloc(dummy_frame())).collect();
        assert_eq!(arena.live_count(), 5);

        // Free in reverse order
        for &id in ids.iter().rev() {
            arena.free(id);
        }
        assert_eq!(arena.live_count(), 0);

        // Allocate again - should reuse all freed slots
        let new_ids: Vec<_> = (0..5).map(|_| arena.alloc(dummy_frame())).collect();
        assert_eq!(arena.capacity(), 6); // No growth (slot 0 + 5 reused)

        // Original IDs should be reused (in LIFO order from free list)
        for new_id in new_ids {
            assert!(ids.iter().any(|&old_id| old_id.0 == new_id.0));
        }
    }
}
