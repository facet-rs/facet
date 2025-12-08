//! Bolero fuzzer for integrated SHM transport behavior.
//!
//! Properties tested:
//! - Ring and slab work correctly together
//! - Messages are delivered in order
//! - No slot leaks (all allocated slots are eventually freed)
//! - No double-free (generation prevents it)
//! - Backpressure works (ring full / slab exhausted)

use bolero::check;
use rapace_fuzz::shm_integration::{execute_and_verify, ShmOp};

fn main() {
    check!()
        .with_type::<(u8, u8, Vec<ShmOpInput>)>()
        .for_each(|(ring_cap_byte, slot_count_byte, ops)| {
            // Map to valid capacities
            let ring_capacity = ((*ring_cap_byte as u32 % 13) + 4).next_power_of_two(); // 4-16
            let slot_count = ((*slot_count_byte as u32 % 13) + 4).min(16); // 4-16

            let ops: Vec<ShmOp> = ops.iter().map(|op| op.to_shm_op()).collect();

            if let Err(e) = execute_and_verify(ring_capacity, slot_count, &ops) {
                panic!("Invariant violated: {}", e);
            }
        });
}

/// Fuzz-friendly input type for SHM operations.
#[derive(Debug, Clone, bolero::TypeGenerator)]
enum ShmOpInput {
    Send(u16),
    Recv,
    Free(u8),
    Retry,
}

impl ShmOpInput {
    fn to_shm_op(&self) -> ShmOp {
        match self {
            ShmOpInput::Send(len) => ShmOp::Send(*len),
            ShmOpInput::Recv => ShmOp::Recv,
            ShmOpInput::Free(idx) => ShmOp::Free(*idx),
            ShmOpInput::Retry => ShmOp::Retry,
        }
    }
}

#[cfg(test)]
mod tests {
    #![allow(unused_imports)]
    use rapace_fuzz::shm_integration::{execute_and_verify, ShmOp};

    #[test]
    fn test_basic_flow() {
        let ops = vec![
            ShmOp::Send(100),
            ShmOp::Send(200),
            ShmOp::Recv,
            ShmOp::Recv,
            ShmOp::Free(0),
            ShmOp::Free(0),
        ];
        execute_and_verify(4, 8, &ops).unwrap();
    }

    #[test]
    fn test_pressure_and_recovery() {
        let mut ops = Vec::new();

        // Fill ring and slab
        for _ in 0..8 {
            ops.push(ShmOp::Send(100));
        }

        // Drain and free
        for _ in 0..8 {
            ops.push(ShmOp::Recv);
            ops.push(ShmOp::Free(0));
        }

        // Retry any pending
        ops.push(ShmOp::Retry);

        // Should work after recovery
        for _ in 0..4 {
            ops.push(ShmOp::Send(100));
        }

        execute_and_verify(4, 8, &ops).unwrap();
    }

    #[test]
    fn test_interleaved_operations() {
        let ops = vec![
            ShmOp::Send(100),
            ShmOp::Recv,
            ShmOp::Send(100),
            ShmOp::Free(0),
            ShmOp::Send(100),
            ShmOp::Recv,
            ShmOp::Recv,
            ShmOp::Free(0),
            ShmOp::Free(0),
        ];
        execute_and_verify(4, 4, &ops).unwrap();
    }
}
