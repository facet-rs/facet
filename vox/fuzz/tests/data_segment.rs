//! Bolero fuzzer for DataSegment alloc/free operations.
//!
//! Properties tested:
//! - No double-free (generation prevents it)
//! - No overlapping allocations
//! - State machine: Free -> Allocated -> InFlight -> Free
//! - Generation monotonically increases per slot
//! - Stale handles are rejected

use bolero::check;
use rapace_fuzz::slab_model::{execute_and_verify, SlabOp, MAX_SLOT_COUNT, MIN_SLOT_COUNT};

fn main() {
    check!()
        .with_type::<(u8, Vec<SlabOpInput>)>()
        .for_each(|(slot_count_byte, ops_data)| {
            // Map to valid slot count
            let slot_count = (*slot_count_byte as u32 % (MAX_SLOT_COUNT - MIN_SLOT_COUNT + 1)) + MIN_SLOT_COUNT;

            // Convert ops_data to SlabOps
            let ops: Vec<SlabOp> = ops_data.iter().map(|op| op.to_slab_op()).collect();

            // Run and verify
            if let Err(e) = execute_and_verify(slot_count, &ops) {
                panic!("Invariant violated: {}", e);
            }
        });
}

/// Fuzz-friendly input type for slab operations.
#[derive(Debug, Clone, bolero::TypeGenerator)]
enum SlabOpInput {
    Alloc,
    MarkInFlight(u8),  // Index into allocated list (will be modulo'd)
    Free(u8),          // Index into in_flight list (will be modulo'd)
    DoubleFree { index: u8, generation: u8 },
    UseStale { index: u8, generation: u8 },
}

impl SlabOpInput {
    fn to_slab_op(&self) -> SlabOp {
        match self {
            SlabOpInput::Alloc => SlabOp::Alloc,
            SlabOpInput::MarkInFlight(idx) => SlabOp::MarkInFlight(*idx as usize),
            SlabOpInput::Free(idx) => SlabOp::Free(*idx as usize),
            SlabOpInput::DoubleFree { index, generation } => SlabOp::DoubleFree {
                index: *index as u32,
                generation: *generation as u32,
            },
            SlabOpInput::UseStale { index, generation } => SlabOp::UseStale {
                index: *index as u32,
                generation: *generation as u32,
            },
        }
    }
}

#[cfg(test)]
mod tests {
    #![allow(unused_imports)]
    use rapace_fuzz::slab_model::{execute_and_verify, SlabOp};

    #[test]
    fn fuzz_slab_basic() {
        let ops = vec![
            SlabOp::Alloc,
            SlabOp::Alloc,
            SlabOp::MarkInFlight(0),
            SlabOp::Free(0),
            SlabOp::Alloc,
        ];
        execute_and_verify(4, &ops).unwrap();
    }

    #[test]
    fn fuzz_slab_full_cycle() {
        // Allocate all, mark all in-flight, free all, repeat
        let mut ops = Vec::new();
        for _ in 0..3 {
            // Allocate all 8 slots
            for _ in 0..8 {
                ops.push(SlabOp::Alloc);
            }
            // Mark all in-flight
            for _ in 0..8 {
                ops.push(SlabOp::MarkInFlight(0));
            }
            // Free all
            for _ in 0..8 {
                ops.push(SlabOp::Free(0));
            }
        }
        execute_and_verify(8, &ops).unwrap();
    }

    #[test]
    fn fuzz_slab_interleaved() {
        // Interleaved alloc/mark/free
        let ops = vec![
            SlabOp::Alloc,       // h0
            SlabOp::Alloc,       // h1
            SlabOp::MarkInFlight(0), // h0 -> in_flight
            SlabOp::Alloc,       // h2
            SlabOp::Free(0),     // h0 freed
            SlabOp::MarkInFlight(0), // h1 -> in_flight
            SlabOp::Alloc,       // h3 (reuses h0's slot)
            SlabOp::Free(0),     // h1 freed
            SlabOp::MarkInFlight(0), // h2 -> in_flight
            SlabOp::MarkInFlight(0), // h3 -> in_flight
            SlabOp::Free(0),
            SlabOp::Free(0),
        ];
        execute_and_verify(4, &ops).unwrap();
    }

    #[test]
    fn fuzz_slab_double_free_rejected() {
        let ops = vec![
            SlabOp::Alloc,
            SlabOp::MarkInFlight(0),
            SlabOp::Free(0),
            // Try to double-free with old handle
            SlabOp::DoubleFree { index: 0, generation: 1 },
        ];
        // Should not panic - the double-free attempt should be rejected
        execute_and_verify(4, &ops).unwrap();
    }

    #[test]
    fn fuzz_slab_stale_handle_rejected() {
        let ops = vec![
            SlabOp::Alloc,
            SlabOp::MarkInFlight(0),
            SlabOp::Free(0),
            SlabOp::Alloc, // Gets generation 2
            // Try to use stale handle with generation 1
            SlabOp::UseStale { index: 0, generation: 1 },
        ];
        execute_and_verify(4, &ops).unwrap();
    }

    #[test]
    fn fuzz_slab_exhaustion() {
        let mut ops = Vec::new();
        // Allocate more than available
        for _ in 0..10 {
            ops.push(SlabOp::Alloc);
        }
        // Only 4 should succeed, rest should be NoFreeSlots
        execute_and_verify(4, &ops).unwrap();
    }
}
