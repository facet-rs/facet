//! In-memory model of DataSegment (slab allocator) for property-based testing.
//!
//! This module provides a pure Rust model of the slab allocator
//! that can be fuzzed without touching real shared memory.

use std::collections::HashSet;

/// Minimum slot count for fuzzing.
pub const MIN_SLOT_COUNT: u32 = 4;
/// Maximum slot count for fuzzing (keep small to find edge cases faster).
pub const MAX_SLOT_COUNT: u32 = 32;

/// Slot states matching layout.rs
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SlotState {
    Free,
    Allocated,
    InFlight,
}

/// Metadata for a single slot.
#[derive(Debug, Clone)]
pub struct SlotMetaModel {
    pub generation: u32,
    pub state: SlotState,
}

impl Default for SlotMetaModel {
    fn default() -> Self {
        Self {
            generation: 0,
            state: SlotState::Free,
        }
    }
}

impl SlotMetaModel {
    pub fn new() -> Self {
        Self::default()
    }
}

/// A handle to an allocated slot.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct SlotHandle {
    pub index: u32,
    pub generation: u32,
}

/// In-memory model of the slab allocator.
pub struct SlabModel {
    slots: Vec<SlotMetaModel>,
    slot_count: u32,
}

impl SlabModel {
    /// Create a new slab with given slot count.
    pub fn new(slot_count: u32) -> Self {
        let slot_count = slot_count.clamp(MIN_SLOT_COUNT, MAX_SLOT_COUNT);
        Self {
            slots: (0..slot_count).map(|_| SlotMetaModel::new()).collect(),
            slot_count,
        }
    }

    /// Allocate a slot. Returns (index, generation) on success.
    pub fn alloc(&mut self) -> Result<SlotHandle, SlabError> {
        // Linear scan for a free slot (matches the real implementation)
        for i in 0..self.slot_count {
            let slot = &mut self.slots[i as usize];
            if slot.state == SlotState::Free {
                slot.state = SlotState::Allocated;
                slot.generation = slot.generation.wrapping_add(1);
                return Ok(SlotHandle {
                    index: i,
                    generation: slot.generation,
                });
            }
        }
        Err(SlabError::NoFreeSlots)
    }

    /// Mark a slot as in-flight (after enqueuing descriptor).
    pub fn mark_in_flight(&mut self, handle: SlotHandle) -> Result<(), SlabError> {
        self.validate_handle(handle)?;

        let slot = &mut self.slots[handle.index as usize];

        if slot.generation != handle.generation {
            return Err(SlabError::StaleGeneration);
        }

        if slot.state != SlotState::Allocated {
            return Err(SlabError::InvalidState {
                expected: SlotState::Allocated,
                found: slot.state,
            });
        }

        slot.state = SlotState::InFlight;
        Ok(())
    }

    /// Free a slot (receiver side, after processing).
    pub fn free(&mut self, handle: SlotHandle) -> Result<(), SlabError> {
        self.validate_handle(handle)?;

        let slot = &mut self.slots[handle.index as usize];

        if slot.generation != handle.generation {
            return Err(SlabError::StaleGeneration);
        }

        if slot.state != SlotState::InFlight {
            return Err(SlabError::InvalidState {
                expected: SlotState::InFlight,
                found: slot.state,
            });
        }

        slot.state = SlotState::Free;
        Ok(())
    }

    /// Get slot state (for testing).
    pub fn get_state(&self, index: u32) -> Option<SlotState> {
        self.slots.get(index as usize).map(|s| s.state)
    }

    /// Get slot generation (for testing).
    pub fn get_generation(&self, index: u32) -> Option<u32> {
        self.slots.get(index as usize).map(|s| s.generation)
    }

    /// Get slot count.
    pub fn slot_count(&self) -> u32 {
        self.slot_count
    }

    /// Count free slots.
    pub fn free_count(&self) -> usize {
        self.slots.iter().filter(|s| s.state == SlotState::Free).count()
    }

    /// Count allocated slots.
    pub fn allocated_count(&self) -> usize {
        self.slots.iter().filter(|s| s.state == SlotState::Allocated).count()
    }

    /// Count in-flight slots.
    pub fn in_flight_count(&self) -> usize {
        self.slots.iter().filter(|s| s.state == SlotState::InFlight).count()
    }

    fn validate_handle(&self, handle: SlotHandle) -> Result<(), SlabError> {
        if handle.index >= self.slot_count {
            return Err(SlabError::InvalidIndex);
        }
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SlabError {
    NoFreeSlots,
    InvalidIndex,
    StaleGeneration,
    InvalidState {
        expected: SlotState,
        found: SlotState,
    },
}

/// Operations that can be performed on the slab.
#[derive(Clone, Debug)]
pub enum SlabOp {
    /// Allocate a new slot.
    Alloc,
    /// Mark a slot as in-flight (uses handle index into active handles list).
    MarkInFlight(usize),
    /// Free a slot (uses handle index into in-flight handles list).
    Free(usize),
    /// Try to double-free with a stale handle (fuzzer provides index + gen).
    DoubleFree { index: u32, generation: u32 },
    /// Try to use a stale handle after free.
    UseStale { index: u32, generation: u32 },
}

/// State tracker for verification.
#[derive(Debug, Default)]
pub struct SlabTracker {
    /// Handles that are Allocated (not yet in-flight).
    allocated: Vec<SlotHandle>,
    /// Handles that are InFlight (sent to receiver).
    in_flight: Vec<SlotHandle>,
    /// All handles ever seen (for stale detection).
    all_seen: HashSet<SlotHandle>,
}

impl SlabTracker {
    pub fn new() -> Self {
        Self::default()
    }
}

/// Execute a sequence of operations and verify invariants.
pub fn execute_and_verify(slot_count: u32, ops: &[SlabOp]) -> Result<(), String> {
    let mut slab = SlabModel::new(slot_count);
    let mut tracker = SlabTracker::new();

    for (i, op) in ops.iter().enumerate() {
        match op {
            SlabOp::Alloc => {
                match slab.alloc() {
                    Ok(handle) => {
                        // Verify generation increased
                        if tracker.all_seen.contains(&handle) {
                            return Err(format!(
                                "op {}: got duplicate handle {:?}",
                                i, handle
                            ));
                        }
                        tracker.all_seen.insert(handle);
                        tracker.allocated.push(handle);
                    }
                    Err(SlabError::NoFreeSlots) => {
                        // Expected when all slots are in use
                        let total_in_use = tracker.allocated.len() + tracker.in_flight.len();
                        if total_in_use < slab.slot_count() as usize {
                            return Err(format!(
                                "op {}: NoFreeSlots but only {} in use (capacity {})",
                                i, total_in_use, slab.slot_count()
                            ));
                        }
                    }
                    Err(e) => {
                        return Err(format!("op {}: unexpected alloc error: {:?}", i, e));
                    }
                }
            }
            SlabOp::MarkInFlight(handle_idx) => {
                if tracker.allocated.is_empty() {
                    continue; // Skip if no allocated slots
                }
                let idx = handle_idx % tracker.allocated.len();
                let handle = tracker.allocated.remove(idx);

                match slab.mark_in_flight(handle) {
                    Ok(()) => {
                        tracker.in_flight.push(handle);
                    }
                    Err(e) => {
                        return Err(format!(
                            "op {}: mark_in_flight({:?}) failed: {:?}",
                            i, handle, e
                        ));
                    }
                }
            }
            SlabOp::Free(handle_idx) => {
                if tracker.in_flight.is_empty() {
                    continue; // Skip if no in-flight slots
                }
                let idx = handle_idx % tracker.in_flight.len();
                let handle = tracker.in_flight.remove(idx);

                match slab.free(handle) {
                    Ok(()) => {
                        // Slot is now free, handle is stale
                    }
                    Err(e) => {
                        return Err(format!(
                            "op {}: free({:?}) failed: {:?}",
                            i, handle, e
                        ));
                    }
                }
            }
            SlabOp::DoubleFree { index, generation } => {
                let handle = SlotHandle {
                    index: *index % slab.slot_count(),
                    generation: *generation,
                };

                // This might succeed if by chance we hit a valid in-flight handle
                match slab.free(handle) {
                    Ok(()) => {
                        // The fuzzer happened to hit a valid in-flight handle
                        // Update tracker to reflect this
                        if let Some(pos) = tracker.in_flight.iter().position(|h| *h == handle) {
                            tracker.in_flight.remove(pos);
                        }
                    }
                    Err(SlabError::StaleGeneration) => {
                        // Expected - generation doesn't match
                    }
                    Err(SlabError::InvalidState { .. }) => {
                        // Expected - slot not in InFlight state
                    }
                    Err(SlabError::InvalidIndex) => {
                        // Shouldn't happen since we mod by slot_count
                        return Err(format!("op {}: unexpected InvalidIndex", i));
                    }
                    Err(e) => {
                        return Err(format!("op {}: unexpected error: {:?}", i, e));
                    }
                }
            }
            SlabOp::UseStale { index, generation } => {
                let handle = SlotHandle {
                    index: *index % slab.slot_count(),
                    generation: *generation,
                };

                // This might succeed if by chance we hit a valid allocated handle
                match slab.mark_in_flight(handle) {
                    Ok(()) => {
                        // The fuzzer happened to hit a valid allocated handle
                        // Move it from allocated to in_flight in tracker
                        if let Some(pos) = tracker.allocated.iter().position(|h| *h == handle) {
                            tracker.allocated.remove(pos);
                            tracker.in_flight.push(handle);
                        }
                    }
                    Err(SlabError::StaleGeneration) => {
                        // Expected
                    }
                    Err(SlabError::InvalidState { .. }) => {
                        // Also acceptable - slot exists but wrong state
                    }
                    Err(SlabError::InvalidIndex) => {
                        return Err(format!("op {}: unexpected InvalidIndex", i));
                    }
                    Err(e) => {
                        return Err(format!("op {}: unexpected error: {:?}", i, e));
                    }
                }
            }
        }

        // Verify invariants after each operation
        verify_slab_invariants(&slab, &tracker, i)?;
    }

    Ok(())
}

/// Verify slab invariants.
fn verify_slab_invariants(
    slab: &SlabModel,
    tracker: &SlabTracker,
    op_idx: usize,
) -> Result<(), String> {
    // Invariant 1: Total slots = free + allocated + in_flight
    let free = slab.free_count();
    let allocated = slab.allocated_count();
    let in_flight = slab.in_flight_count();
    let total = free + allocated + in_flight;

    if total != slab.slot_count() as usize {
        return Err(format!(
            "after op {}: free({}) + allocated({}) + in_flight({}) = {} != slot_count({})",
            op_idx, free, allocated, in_flight, total, slab.slot_count()
        ));
    }

    // Invariant 2: Tracker counts match slab counts
    if tracker.allocated.len() != allocated {
        return Err(format!(
            "after op {}: tracker.allocated.len()={} != slab.allocated_count()={}",
            op_idx, tracker.allocated.len(), allocated
        ));
    }

    if tracker.in_flight.len() != in_flight {
        return Err(format!(
            "after op {}: tracker.in_flight.len()={} != slab.in_flight_count()={}",
            op_idx, tracker.in_flight.len(), in_flight
        ));
    }

    // Invariant 3: No duplicate indices in allocated + in_flight
    let mut seen_indices = HashSet::new();
    for h in tracker.allocated.iter().chain(tracker.in_flight.iter()) {
        if !seen_indices.insert(h.index) {
            return Err(format!(
                "after op {}: duplicate index {} in active handles",
                op_idx, h.index
            ));
        }
    }

    // Invariant 4: Each active handle's state matches
    for h in &tracker.allocated {
        if slab.get_state(h.index) != Some(SlotState::Allocated) {
            return Err(format!(
                "after op {}: allocated handle {:?} has state {:?}",
                op_idx, h, slab.get_state(h.index)
            ));
        }
        if slab.get_generation(h.index) != Some(h.generation) {
            return Err(format!(
                "after op {}: allocated handle {:?} has generation {:?}",
                op_idx, h, slab.get_generation(h.index)
            ));
        }
    }

    for h in &tracker.in_flight {
        if slab.get_state(h.index) != Some(SlotState::InFlight) {
            return Err(format!(
                "after op {}: in_flight handle {:?} has state {:?}",
                op_idx, h, slab.get_state(h.index)
            ));
        }
        if slab.get_generation(h.index) != Some(h.generation) {
            return Err(format!(
                "after op {}: in_flight handle {:?} has generation {:?}",
                op_idx, h, slab.get_generation(h.index)
            ));
        }
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_basic_alloc_free() {
        let mut slab = SlabModel::new(4);

        let h1 = slab.alloc().unwrap();
        let h2 = slab.alloc().unwrap();

        assert_ne!(h1.index, h2.index);
        assert_eq!(slab.allocated_count(), 2);

        slab.mark_in_flight(h1).unwrap();
        assert_eq!(slab.allocated_count(), 1);
        assert_eq!(slab.in_flight_count(), 1);

        slab.free(h1).unwrap();
        assert_eq!(slab.free_count(), 3);
    }

    #[test]
    fn test_generation_prevents_reuse() {
        let mut slab = SlabModel::new(4);

        // Allocate, mark in-flight, free
        let h1 = slab.alloc().unwrap();
        slab.mark_in_flight(h1).unwrap();
        slab.free(h1).unwrap();

        // Allocate again - should get same index but different generation
        let h2 = slab.alloc().unwrap();
        assert_eq!(h1.index, h2.index); // Linear scan finds same slot
        assert_ne!(h1.generation, h2.generation);

        // Trying to use old handle should fail
        let result = slab.mark_in_flight(h1);
        assert_eq!(result, Err(SlabError::StaleGeneration));
    }

    #[test]
    fn test_state_machine_violations() {
        let mut slab = SlabModel::new(4);

        let h1 = slab.alloc().unwrap();

        // Can't free an Allocated slot (must be InFlight)
        let result = slab.free(h1);
        assert!(matches!(result, Err(SlabError::InvalidState { .. })));

        // Can't mark_in_flight twice
        slab.mark_in_flight(h1).unwrap();

        // Create a "fake" handle with same index/gen to try marking again
        let fake = SlotHandle {
            index: h1.index,
            generation: h1.generation,
        };
        let result = slab.mark_in_flight(fake);
        assert!(matches!(result, Err(SlabError::InvalidState { .. })));
    }
}
