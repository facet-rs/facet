//! Bolero fuzzer for DescRing enqueue/dequeue operations.
//!
//! Properties tested:
//! - Ring never writes outside capacity
//! - FIFO ordering is preserved
//! - visible_head >= tail always
//! - len <= capacity always
//! - Wrap-around works correctly

use bolero::check;
use rapace_fuzz::ring_model::{execute_and_verify, RingOp, MAX_CAPACITY, MIN_CAPACITY};

fn main() {
    check!()
        .with_type::<(u8, Vec<(bool, u64)>)>()
        .for_each(|(capacity_byte, ops_data)| {
            // Map capacity to a valid power of 2
            let capacity = {
                let c = (*capacity_byte as u32 % (MAX_CAPACITY - MIN_CAPACITY + 1)) + MIN_CAPACITY;
                c.next_power_of_two().min(MAX_CAPACITY)
            };

            // Convert ops_data to RingOps
            let ops: Vec<RingOp> = ops_data
                .iter()
                .map(|(is_enqueue, id)| {
                    if *is_enqueue {
                        RingOp::Enqueue(*id)
                    } else {
                        RingOp::Dequeue
                    }
                })
                .collect();

            // Run and verify - panics are caught by bolero
            if let Err(e) = execute_and_verify(capacity, &ops) {
                panic!("Invariant violated: {}", e);
            }
        });
}

#[cfg(test)]
mod tests {
    #![allow(unused_imports)]
    use rapace_fuzz::ring_model::{execute_and_verify, RingOp};

    #[test]
    fn fuzz_ring_basic() {
        // Quick sanity test with hardcoded sequences
        let ops = vec![
            RingOp::Enqueue(1),
            RingOp::Enqueue(2),
            RingOp::Dequeue,
            RingOp::Enqueue(3),
            RingOp::Dequeue,
            RingOp::Dequeue,
        ];
        execute_and_verify(4, &ops).unwrap();
    }

    #[test]
    fn fuzz_ring_full_cycle() {
        // Fill and drain multiple times
        let mut ops = Vec::new();
        for round in 0..5 {
            for i in 0..8 {
                ops.push(RingOp::Enqueue(round * 8 + i));
            }
            for _ in 0..8 {
                ops.push(RingOp::Dequeue);
            }
        }
        execute_and_verify(8, &ops).unwrap();
    }

    #[test]
    fn fuzz_ring_interleaved() {
        // Interleaved enqueue/dequeue
        let mut ops = Vec::new();
        for i in 0..100 {
            ops.push(RingOp::Enqueue(i));
            if i % 3 == 0 {
                ops.push(RingOp::Dequeue);
            }
        }
        // Drain remaining
        for _ in 0..100 {
            ops.push(RingOp::Dequeue);
        }
        execute_and_verify(16, &ops).unwrap();
    }

    #[test]
    fn fuzz_ring_edge_capacity_1() {
        // Capacity 4 (minimum power of 2 >= MIN_CAPACITY)
        let ops = vec![
            RingOp::Enqueue(1),
            RingOp::Enqueue(2),
            RingOp::Enqueue(3),
            RingOp::Enqueue(4),
            RingOp::Enqueue(5), // Should fail - full
            RingOp::Dequeue,
            RingOp::Enqueue(5), // Now should succeed
            RingOp::Dequeue,
            RingOp::Dequeue,
            RingOp::Dequeue,
            RingOp::Dequeue,
            RingOp::Dequeue, // Should be empty
        ];
        execute_and_verify(4, &ops).unwrap();
    }
}
