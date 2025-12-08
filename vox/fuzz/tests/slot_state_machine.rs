//! Bolero fuzzer for slot state machine transitions.
//!
//! This fuzzer specifically targets the state machine:
//!   Free -> Allocated -> InFlight -> Free
//!
//! Properties tested:
//! - Only valid transitions are allowed
//! - Invalid transitions are rejected
//! - Generation prevents ABA problems
//! - Concurrent-style access patterns are safe

use bolero::check;
use rapace_fuzz::slab_model::{SlabModel, SlotHandle, SlotState, SlabError};

fn main() {
    check!()
        .with_type::<Vec<StateOp>>()
        .for_each(|ops| {
            let mut slab = SlabModel::new(8);
            let mut handles: Vec<Option<SlotHandle>> = vec![None; 8];

            for op in ops {
                match op {
                    StateOp::AllocSlot(slot_idx) => {
                        let slot_idx = (*slot_idx as usize) % 8;
                        if handles[slot_idx].is_some() {
                            continue; // Already have a handle for this "logical slot"
                        }
                        if let Ok(h) = slab.alloc() {
                            handles[slot_idx] = Some(h);
                        }
                    }
                    StateOp::MarkInFlight(slot_idx) => {
                        let slot_idx = (*slot_idx as usize) % 8;
                        if let Some(h) = handles[slot_idx] {
                            if slab.get_state(h.index) == Some(SlotState::Allocated) {
                                let result = slab.mark_in_flight(h);
                                assert!(result.is_ok(), "mark_in_flight should succeed for Allocated slot");
                            }
                        }
                    }
                    StateOp::FreeSlot(slot_idx) => {
                        let slot_idx = (*slot_idx as usize) % 8;
                        if let Some(h) = handles[slot_idx].take() {
                            if slab.get_state(h.index) == Some(SlotState::InFlight) {
                                let result = slab.free(h);
                                assert!(result.is_ok(), "free should succeed for InFlight slot");
                            } else {
                                // Put it back if we can't free
                                handles[slot_idx] = Some(h);
                            }
                        }
                    }
                    StateOp::InvalidTransition { slot_idx, from, to } => {
                        let slot_idx = (*slot_idx as usize) % 8;
                        if let Some(h) = handles[slot_idx] {
                            let current_state = slab.get_state(h.index);

                            // Try an invalid transition based on current state
                            match (current_state, from, to) {
                                // Free -> Free (no-op, but free() on Free should fail)
                                (Some(SlotState::Free), _, _) => {
                                    let result = slab.free(h);
                                    assert!(result.is_err(), "free() on Free slot should fail");
                                }
                                // Allocated -> Free (should fail, must go through InFlight)
                                (Some(SlotState::Allocated), _, _) => {
                                    let result = slab.free(h);
                                    assert!(result.is_err(), "free() on Allocated slot should fail");
                                }
                                // InFlight -> Allocated (should fail)
                                (Some(SlotState::InFlight), _, _) => {
                                    let result = slab.mark_in_flight(h);
                                    assert!(result.is_err(), "mark_in_flight() on InFlight slot should fail");
                                }
                                _ => {}
                            }
                        }
                    }
                    StateOp::UseStaleGen { slot_idx, gen_offset } => {
                        let slot_idx = (*slot_idx as usize) % 8;
                        if let Some(h) = handles[slot_idx] {
                            // Create a stale handle with wrong generation
                            let stale = SlotHandle {
                                index: h.index,
                                generation: h.generation.wrapping_sub(*gen_offset as u32 + 1),
                            };

                            // Any operation with stale gen should fail
                            let mark_result = slab.mark_in_flight(stale);
                            let free_result = slab.free(stale);

                            // At least one should be StaleGeneration (depending on state)
                            let is_stale = matches!(mark_result, Err(SlabError::StaleGeneration))
                                || matches!(free_result, Err(SlabError::StaleGeneration));

                            // It's also valid to get InvalidState if gen matches but state is wrong
                            let is_invalid_state = matches!(mark_result, Err(SlabError::InvalidState { .. }))
                                || matches!(free_result, Err(SlabError::InvalidState { .. }));

                            assert!(
                                is_stale || is_invalid_state || (mark_result.is_err() && free_result.is_err()),
                                "stale handle operations should fail"
                            );
                        }
                    }
                }
            }
        });
}

/// Operations for state machine fuzzing.
#[derive(Debug, Clone, bolero::TypeGenerator)]
enum StateOp {
    AllocSlot(u8),
    MarkInFlight(u8),
    FreeSlot(u8),
    InvalidTransition { slot_idx: u8, from: u8, to: u8 },
    UseStaleGen { slot_idx: u8, gen_offset: u8 },
}

#[cfg(test)]
mod tests {
    #![allow(unused_imports)]
    use rapace_fuzz::slab_model::{SlabError, SlabModel, SlotHandle, SlotState};

    #[test]
    fn test_valid_state_machine() {
        let mut slab = SlabModel::new(4);

        // Free -> Allocated
        let h = slab.alloc().unwrap();
        assert_eq!(slab.get_state(h.index), Some(SlotState::Allocated));

        // Allocated -> InFlight
        slab.mark_in_flight(h).unwrap();
        assert_eq!(slab.get_state(h.index), Some(SlotState::InFlight));

        // InFlight -> Free
        slab.free(h).unwrap();
        assert_eq!(slab.get_state(h.index), Some(SlotState::Free));
    }

    #[test]
    fn test_invalid_transitions() {
        let mut slab = SlabModel::new(4);

        let h = slab.alloc().unwrap();

        // Allocated -> Free (invalid, must go through InFlight)
        assert!(matches!(slab.free(h), Err(SlabError::InvalidState { .. })));

        // Allocated -> InFlight (valid)
        slab.mark_in_flight(h).unwrap();

        // InFlight -> Allocated (invalid)
        assert!(matches!(slab.mark_in_flight(h), Err(SlabError::InvalidState { .. })));
    }

    #[test]
    fn test_generation_aba_safety() {
        let mut slab = SlabModel::new(4);

        // Get a handle
        let h1 = slab.alloc().unwrap();
        slab.mark_in_flight(h1).unwrap();
        slab.free(h1).unwrap();

        // Allocate again - same slot, new generation
        let h2 = slab.alloc().unwrap();
        assert_eq!(h1.index, h2.index); // Same slot
        assert_ne!(h1.generation, h2.generation); // Different generation

        // Old handle should be stale
        assert!(matches!(slab.mark_in_flight(h1), Err(SlabError::StaleGeneration)));
    }
}
