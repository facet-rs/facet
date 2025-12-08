//! In-memory model of DescRing for property-based testing.
//!
//! This module provides a pure Rust model of the SPSC descriptor ring
//! that can be fuzzed without touching real shared memory.

use std::sync::atomic::{AtomicU64, Ordering};

/// Minimum ring capacity (must be power of 2).
pub const MIN_CAPACITY: u32 = 4;
/// Maximum ring capacity for fuzzing (keep small to find edge cases faster).
pub const MAX_CAPACITY: u32 = 64;

/// A model of DescRingHeader for in-memory testing.
#[repr(C)]
pub struct RingHeaderModel {
    pub visible_head: AtomicU64,
    pub tail: AtomicU64,
    pub capacity: u32,
}

impl RingHeaderModel {
    /// Create a new ring header with given capacity.
    ///
    /// # Panics
    /// Panics if capacity is not a power of 2.
    pub fn new(capacity: u32) -> Self {
        assert!(capacity.is_power_of_two(), "capacity must be power of 2");
        Self {
            visible_head: AtomicU64::new(0),
            tail: AtomicU64::new(0),
            capacity,
        }
    }

    #[inline]
    pub fn mask(&self) -> u64 {
        self.capacity as u64 - 1
    }

    #[inline]
    pub fn is_empty(&self) -> bool {
        let tail = self.tail.load(Ordering::Relaxed);
        let head = self.visible_head.load(Ordering::Acquire);
        tail >= head
    }

    #[inline]
    pub fn is_full(&self, local_head: u64) -> bool {
        let tail = self.tail.load(Ordering::Acquire);
        local_head.wrapping_sub(tail) >= self.capacity as u64
    }

    #[inline]
    pub fn len(&self) -> usize {
        let tail = self.tail.load(Ordering::Relaxed);
        let head = self.visible_head.load(Ordering::Acquire);
        head.saturating_sub(tail) as usize
    }
}

/// A simple descriptor for testing (just an ID).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(C)]
pub struct TestDescriptor {
    pub id: u64,
}

/// In-memory model of a descriptor ring.
pub struct RingModel {
    header: RingHeaderModel,
    descriptors: Vec<TestDescriptor>,
    /// Producer's local head (not in "shared memory").
    local_head: u64,
}

impl RingModel {
    /// Create a new ring with given capacity.
    pub fn new(capacity: u32) -> Self {
        let capacity = capacity.next_power_of_two().max(MIN_CAPACITY).min(MAX_CAPACITY);
        Self {
            header: RingHeaderModel::new(capacity),
            descriptors: vec![TestDescriptor { id: 0 }; capacity as usize],
            local_head: 0,
        }
    }

    /// Enqueue a descriptor (producer side).
    pub fn enqueue(&mut self, desc: TestDescriptor) -> Result<(), RingError> {
        if self.header.is_full(self.local_head) {
            return Err(RingError::Full);
        }

        let idx = (self.local_head & self.header.mask()) as usize;

        // INVARIANT: idx must be < capacity
        assert!(idx < self.descriptors.len(), "enqueue index out of bounds");

        self.descriptors[idx] = desc;
        self.local_head += 1;

        // Publish
        self.header.visible_head.store(self.local_head, Ordering::Release);

        Ok(())
    }

    /// Dequeue a descriptor (consumer side).
    pub fn dequeue(&mut self) -> Option<TestDescriptor> {
        let tail = self.header.tail.load(Ordering::Relaxed);
        let visible = self.header.visible_head.load(Ordering::Acquire);

        if tail >= visible {
            return None;
        }

        let idx = (tail & self.header.mask()) as usize;

        // INVARIANT: idx must be < capacity
        assert!(idx < self.descriptors.len(), "dequeue index out of bounds");

        let desc = self.descriptors[idx];

        // Advance tail
        self.header.tail.store(tail + 1, Ordering::Release);

        Some(desc)
    }

    /// Check if empty.
    pub fn is_empty(&self) -> bool {
        self.header.is_empty()
    }

    /// Get current length.
    pub fn len(&self) -> usize {
        self.header.len()
    }

    /// Get capacity.
    pub fn capacity(&self) -> u32 {
        self.header.capacity
    }

    /// Get the producer's local head (for testing).
    pub fn local_head(&self) -> u64 {
        self.local_head
    }

    /// Get the visible head.
    pub fn visible_head(&self) -> u64 {
        self.header.visible_head.load(Ordering::Acquire)
    }

    /// Get the tail.
    pub fn tail(&self) -> u64 {
        self.header.tail.load(Ordering::Acquire)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RingError {
    Full,
}

/// Operations that can be performed on the ring.
#[derive(Clone, Copy, Debug)]
pub enum RingOp {
    Enqueue(u64),
    Dequeue,
}

/// Execute a sequence of operations and verify invariants.
pub fn execute_and_verify(capacity: u32, ops: &[RingOp]) -> Result<(), String> {
    let mut ring = RingModel::new(capacity);
    let mut expected_contents: std::collections::VecDeque<u64> = std::collections::VecDeque::new();

    for (i, op) in ops.iter().enumerate() {
        match op {
            RingOp::Enqueue(id) => {
                let result = ring.enqueue(TestDescriptor { id: *id });
                match result {
                    Ok(()) => {
                        expected_contents.push_back(*id);
                    }
                    Err(RingError::Full) => {
                        // Ring is full - this is expected, verify the invariant
                        if expected_contents.len() < ring.capacity() as usize {
                            return Err(format!(
                                "op {}: ring reported full but only has {} items (capacity {})",
                                i,
                                expected_contents.len(),
                                ring.capacity()
                            ));
                        }
                    }
                }
            }
            RingOp::Dequeue => {
                let result = ring.dequeue();
                match (result, expected_contents.pop_front()) {
                    (Some(desc), Some(expected_id)) => {
                        if desc.id != expected_id {
                            return Err(format!(
                                "op {}: dequeued id {} but expected {}",
                                i, desc.id, expected_id
                            ));
                        }
                    }
                    (None, None) => {
                        // Both empty - good
                    }
                    (Some(desc), None) => {
                        return Err(format!(
                            "op {}: dequeued {:?} but expected empty",
                            i, desc
                        ));
                    }
                    (None, Some(expected_id)) => {
                        return Err(format!(
                            "op {}: got empty but expected id {}",
                            i, expected_id
                        ));
                    }
                }
            }
        }

        // Verify invariants after each operation
        verify_ring_invariants(&ring, &expected_contents, i)?;
    }

    Ok(())
}

/// Verify ring invariants.
fn verify_ring_invariants(
    ring: &RingModel,
    expected: &std::collections::VecDeque<u64>,
    op_idx: usize,
) -> Result<(), String> {
    // Invariant 1: len matches expected
    if ring.len() != expected.len() {
        return Err(format!(
            "after op {}: ring.len()={} but expected.len()={}",
            op_idx,
            ring.len(),
            expected.len()
        ));
    }

    // Invariant 2: visible_head >= tail
    let head = ring.visible_head();
    let tail = ring.tail();
    if head < tail {
        return Err(format!(
            "after op {}: visible_head ({}) < tail ({})",
            op_idx, head, tail
        ));
    }

    // Invariant 3: len = visible_head - tail
    let computed_len = (head - tail) as usize;
    if computed_len != ring.len() {
        return Err(format!(
            "after op {}: head-tail={} but len()={}",
            op_idx, computed_len, ring.len()
        ));
    }

    // Invariant 4: len <= capacity
    if ring.len() > ring.capacity() as usize {
        return Err(format!(
            "after op {}: len ({}) > capacity ({})",
            op_idx,
            ring.len(),
            ring.capacity()
        ));
    }

    // Invariant 5: local_head >= visible_head (producer may have unpublished items)
    // In our model we always publish immediately, so they should be equal
    if ring.local_head() != ring.visible_head() {
        return Err(format!(
            "after op {}: local_head ({}) != visible_head ({})",
            op_idx,
            ring.local_head(),
            ring.visible_head()
        ));
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_basic_enqueue_dequeue() {
        let mut ring = RingModel::new(4);

        ring.enqueue(TestDescriptor { id: 1 }).unwrap();
        ring.enqueue(TestDescriptor { id: 2 }).unwrap();

        assert_eq!(ring.len(), 2);
        assert_eq!(ring.dequeue(), Some(TestDescriptor { id: 1 }));
        assert_eq!(ring.dequeue(), Some(TestDescriptor { id: 2 }));
        assert_eq!(ring.dequeue(), None);
    }

    #[test]
    fn test_full_ring() {
        let mut ring = RingModel::new(4);

        for i in 0..4 {
            ring.enqueue(TestDescriptor { id: i }).unwrap();
        }

        assert_eq!(ring.enqueue(TestDescriptor { id: 99 }), Err(RingError::Full));
    }

    #[test]
    fn test_wrap_around() {
        let mut ring = RingModel::new(4);

        // Fill and drain multiple times to test wrap-around
        for round in 0..10 {
            for i in 0..4 {
                ring.enqueue(TestDescriptor { id: round * 4 + i }).unwrap();
            }
            for i in 0..4 {
                assert_eq!(ring.dequeue(), Some(TestDescriptor { id: round * 4 + i }));
            }
        }
    }
}
