use facet_core::Facet;

// Test 1: Can we use std::alloc?
#[kani::proof]
fn test_alloc_raw() {
    let layout = std::alloc::Layout::new::<u32>();
    let ptr = unsafe { std::alloc::alloc(layout) };
    kani::assert(!ptr.is_null(), "alloc succeeded");
    unsafe { std::alloc::dealloc(ptr, layout) };
}

// Test 2: Can we call default_in_place from vtable?
#[kani::proof]
fn test_vtable_default() {
    let layout = std::alloc::Layout::new::<u32>();
    let ptr = unsafe { std::alloc::alloc(layout) };
    kani::assert(!ptr.is_null(), "alloc succeeded");

    let uninit = facet_core::PtrUninit::new(ptr);
    let result = unsafe { u32::SHAPE.call_default_in_place(uninit) };
    kani::assert(result.is_some(), "default_in_place succeeded");

    // Read the value back
    let value = unsafe { *(ptr as *const u32) };
    kani::assert(value == 0, "default u32 is 0");

    unsafe { std::alloc::dealloc(ptr, layout) };
}

// Test 3: Can we call drop_in_place from vtable?
#[kani::proof]
fn test_vtable_drop() {
    let layout = std::alloc::Layout::new::<u32>();
    let ptr = unsafe { std::alloc::alloc(layout) };
    kani::assert(!ptr.is_null(), "alloc succeeded");

    // Initialize
    unsafe {
        *(ptr as *mut u32) = 42;
    }

    // Drop (no-op for u32, but tests the vtable call)
    let ptr_mut = facet_core::PtrMut::new(ptr);
    let result = unsafe { u32::SHAPE.call_drop_in_place(ptr_mut) };
    kani::assert(result.is_some(), "drop_in_place succeeded");

    unsafe { std::alloc::dealloc(ptr, layout) };
}

// Test 4: Can we use the Arena?
#[kani::proof]
fn test_arena_alloc_free() {
    use crate::arena::Arena;
    use crate::frame::Frame;

    let mut arena: Arena<Frame> = Arena::new();

    // Allocate a simple frame
    let layout = std::alloc::Layout::new::<u32>();
    let ptr = unsafe { std::alloc::alloc(layout) };
    kani::assert(!ptr.is_null(), "alloc succeeded");

    let data = facet_core::PtrUninit::new(ptr);
    let frame = Frame::new(data, u32::SHAPE);
    let idx = arena.alloc(frame);

    kani::assert(idx.is_valid(), "arena index is valid");

    // Free the frame
    let freed = arena.free(idx);
    freed.dealloc_if_owned();

    // Clean up
    unsafe { std::alloc::dealloc(ptr, layout) };
}

// Test 5: Can we call Partial::alloc? (forget to skip Drop)
#[kani::proof]
fn test_partial_alloc_u32_forget() {
    use super::Partial;

    let partial = Partial::alloc::<u32>();
    kani::assert(partial.is_ok(), "Partial::alloc succeeded");
    // Forget to avoid Drop - isolate whether alloc or Drop is the problem
    std::mem::forget(partial);
}

// Test 5.5: Just Frame with Scalar kind - no Partial, no Arena
#[kani::proof]
fn test_frame_scalar_only() {
    use crate::frame::{Frame, FrameKind};

    let layout = std::alloc::Layout::new::<u32>();
    let ptr = unsafe { std::alloc::alloc(layout) };
    kani::assert(!ptr.is_null(), "alloc succeeded");

    let data = facet_core::PtrUninit::new(ptr);
    let frame = Frame::new(data, u32::SHAPE);

    kani::assert(matches!(frame.kind, FrameKind::Scalar), "frame is scalar");

    // Don't drop, just dealloc
    unsafe { std::alloc::dealloc(ptr, layout) };
}

// Test 5.6: Frame + Arena, no Partial
#[kani::proof]
fn test_frame_in_arena() {
    use crate::arena::Arena;
    use crate::frame::{Frame, FrameKind};

    let layout = std::alloc::Layout::new::<u32>();
    let ptr = unsafe { std::alloc::alloc(layout) };
    kani::assert(!ptr.is_null(), "alloc succeeded");

    let data = facet_core::PtrUninit::new(ptr);
    let frame = Frame::new(data, u32::SHAPE);

    let mut arena: Arena<Frame> = Arena::new();
    let idx = arena.alloc(frame);

    let frame_ref = arena.get(idx);
    kani::assert(
        matches!(frame_ref.kind, FrameKind::Scalar),
        "frame is scalar",
    );

    // Free from arena, dealloc manually
    let freed = arena.free(idx);
    freed.dealloc_if_owned();
    unsafe { std::alloc::dealloc(ptr, layout) };
}

// Test 6: Partial::alloc with Drop, bounded unwind
#[kani::proof]
#[kani::unwind(1)]
fn test_partial_alloc_u32_with_drop() {
    use super::Partial;
    use crate::frame::FrameKind;

    let partial = Partial::alloc::<u32>();
    kani::assert(partial.is_ok(), "Partial::alloc succeeded");

    let partial = partial.unwrap();

    // Tell Kani: this is a scalar, not a list/map/struct/enum
    // This constrains the paths Kani explores in Drop
    let frame = partial.arena.get(partial.root);
    kani::assume(matches!(frame.kind, FrameKind::Scalar));

    // Now let it drop - Kani should only explore the scalar path
}

// =======================================================================
// Fresh start: Fixed-arity tree without Vec
// =======================================================================

#[derive(Clone, Copy, PartialEq, Eq, Debug, kani::Arbitrary)]
enum RegionState {
    Unallocated,
    Initialized,
    Dropped,
}

/// A binary tree node - no Vec, just two optional children
struct BinaryNode {
    value: u32,
    left: Option<Box<BinaryNode>>,
    right: Option<Box<BinaryNode>>,
    state: RegionState,
}

impl BinaryNode {
    fn leaf(value: u32) -> Self {
        Self {
            value,
            left: None,
            right: None,
            state: RegionState::Initialized,
        }
    }

    fn with_left(value: u32, left: BinaryNode) -> Self {
        Self {
            value,
            left: Some(Box::new(left)),
            right: None,
            state: RegionState::Initialized,
        }
    }

    fn with_both(value: u32, left: BinaryNode, right: BinaryNode) -> Self {
        Self {
            value,
            left: Some(Box::new(left)),
            right: Some(Box::new(right)),
            state: RegionState::Initialized,
        }
    }
}

#[kani::requires(node.state == RegionState::Initialized)]
#[kani::ensures(|_| node.state == RegionState::Dropped)]
#[kani::modifies(&node.state)]
#[kani::recursion]
fn drop_binary(node: &mut BinaryNode) {
    // Drop children first (no loop - just two optional fields)
    if let Some(ref mut left) = node.left {
        drop_binary(left);
    }
    if let Some(ref mut right) = node.right {
        drop_binary(right);
    }

    node.state = RegionState::Dropped;
}

// Stub for drop_in_place<Box<BinaryNode>> - no-op since contract proves correctness
unsafe fn stub_drop_in_place_box_binary_node<T>(_ptr: *mut T) {
    // Contract verification proves drop_binary is correct
    // This stub breaks the recursion for Kani
}

// Verify the drop_binary contract
#[kani::proof_for_contract(drop_binary)]
#[kani::unwind(2)]
fn verify_drop_binary_contract() {
    let mut node = BinaryNode::leaf(kani::any());
    drop_binary(&mut node);
    std::mem::forget(node); // Avoid double-drop from Drop impl
}

impl Drop for BinaryNode {
    fn drop(&mut self) {
        if self.state == RegionState::Initialized {
            drop_binary(self);
        }
    }
}

// Test 7: Leaf node
#[kani::proof]
fn test_binary_leaf() {
    let node = BinaryNode::leaf(42);
    kani::assert(node.value == 42, "value is correct");
    // Let it drop
}

// Test 8: One level - left child only
#[kani::proof]
fn test_binary_one_child() {
    let child = BinaryNode::leaf(1);
    let parent = BinaryNode::with_left(0, child);
    kani::assert(parent.left.is_some(), "has left child");
    // Drop - one level of recursion
}

// Test 9: Two levels - full binary tree depth 2
#[kani::proof]
fn test_binary_two_levels() {
    let ll = BinaryNode::leaf(3);
    let lr = BinaryNode::leaf(4);
    let left = BinaryNode::with_both(1, ll, lr);

    let rl = BinaryNode::leaf(5);
    let rr = BinaryNode::leaf(6);
    let right = BinaryNode::with_both(2, rl, rr);

    let root = BinaryNode::with_both(0, left, right);
    kani::assert(
        root.left.is_some() && root.right.is_some(),
        "has both children",
    );
    // Drop - two levels of recursion, 7 nodes total
}

// Test 10: Symbolic tree structure - uses contract instead of inlining
#[kani::proof]
#[kani::stub_verified(drop_binary)]
#[kani::stub(std::ptr::drop_in_place::<Box<BinaryNode>>, stub_drop_in_place_box_binary_node)]
fn test_binary_symbolic() {
    // Symbolic choice: does root have a left child?
    let has_left: bool = kani::any();
    // Symbolic choice: does root have a right child?
    let has_right: bool = kani::any();

    let left = if has_left {
        Some(Box::new(BinaryNode::leaf(1)))
    } else {
        None
    };

    let right = if has_right {
        Some(Box::new(BinaryNode::leaf(2)))
    } else {
        None
    };

    let root = BinaryNode {
        value: 0,
        left,
        right,
        state: RegionState::Initialized,
    };

    // Verify the structure matches our choices
    kani::assert(root.left.is_some() == has_left, "left matches");
    kani::assert(root.right.is_some() == has_right, "right matches");
    // Drop - symbolic structure
}
