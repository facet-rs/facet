//! Scaling certificate for the compact inline-sequence representation.
//!
//! A million-element inline scalar sequence must NOT become a million heap
//! nodes. The owned representation is one packed buffer, and hashing it through
//! the closed writer performs a bounded number of heap allocations that does
//! not grow with the element count.

use std::alloc::{GlobalAlloc, Layout, System};
use std::cell::Cell;

use vix::runtime::{FramedNode, SchemaId};

thread_local! {
    static COUNT_ALLOCATIONS: Cell<bool> = const { Cell::new(false) };
    static ALLOCS: Cell<usize> = const { Cell::new(0) };
}

fn record_allocation() {
    COUNT_ALLOCATIONS.with(|counting| {
        if counting.get() {
            ALLOCS.with(|allocs| allocs.set(allocs.get() + 1));
        }
    });
}

struct CountingAllocator;

// SAFETY: forwards every request to the system allocator, only incrementing a
// relaxed counter on the allocating paths.
unsafe impl GlobalAlloc for CountingAllocator {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        record_allocation();
        unsafe { System.alloc(layout) }
    }
    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        unsafe { System.dealloc(ptr, layout) }
    }
    unsafe fn realloc(&self, ptr: *mut u8, layout: Layout, new_size: usize) -> *mut u8 {
        record_allocation();
        unsafe { System.realloc(ptr, layout, new_size) }
    }
}

#[global_allocator]
static GLOBAL: CountingAllocator = CountingAllocator;

fn inline_sequence(count: usize) -> FramedNode {
    // Exact-capacity buffer: one allocation, no growth reallocations.
    let mut canonical_bytes = Vec::with_capacity(count * 8);
    for i in 0..count as u64 {
        canonical_bytes.extend_from_slice(&i.to_le_bytes());
    }
    FramedNode::SeqInline {
        schema: SchemaId::named("scaling.seq"),
        element_schema: SchemaId::named("scaling.element"),
        element_width: 8,
        canonical_bytes,
    }
}

/// Measure the heap allocations performed while hashing an already-built inline
/// sequence node. The count must be independent of the element count.
fn hash_allocations(node: &FramedNode) -> usize {
    ALLOCS.with(|allocs| allocs.set(0));
    COUNT_ALLOCATIONS.with(|counting| {
        assert!(!counting.replace(true), "allocation scopes do not nest");
    });
    let id = node.identity();
    std::hint::black_box(&id);
    COUNT_ALLOCATIONS.with(|counting| counting.set(false));
    ALLOCS.with(Cell::get)
}

#[test]
fn inline_sequence_is_one_packed_buffer_not_per_element_nodes() {
    let small = inline_sequence(1_024);
    let large = inline_sequence(1_048_576);

    // Structural: the large node stores exactly one contiguous 8 MiB buffer,
    // not a million child nodes.
    match &large {
        FramedNode::SeqInline {
            canonical_bytes,
            element_width,
            ..
        } => {
            assert_eq!(*element_width, 8);
            assert_eq!(canonical_bytes.len(), 1_048_576 * 8);
            assert_eq!(canonical_bytes.capacity(), 1_048_576 * 8);
        }
        _ => panic!("inline_sequence must build a SeqInline node"),
    }

    // Allocation-sensitive: hashing a 1M-element sequence allocates the same
    // (small, constant) number of times as a 1K-element one -> O(1) in n.
    let small_allocs = hash_allocations(&small);
    let large_allocs = hash_allocations(&large);
    assert_eq!(
        small_allocs, large_allocs,
        "hash allocations must not grow with element count (got {small_allocs} vs {large_allocs})"
    );
    assert!(
        large_allocs <= 2,
        "hashing an inline sequence is allocation-free up to a tiny constant, got {large_allocs}"
    );

    // The two nodes still have distinct identities (sanity: not a degenerate
    // constant hash).
    assert_ne!(small.identity(), large.identity());
}
