//! Test variance computation with recursive types.
//!
//! These tests verify:
//! 1. Cycle detection prevents infinite recursion
//! 2. Exponential blowup is prevented for multi-recursive types
//! 3. Variance is computed correctly for recursive types
//! 4. Early termination works when invariant is detected

use facet::{Facet, Variance};

/// A recursive type
#[derive(Facet)]
struct Node {
    value: i32,
    #[facet(recursive_type)]
    child: Box<Node>,
}

/// A simple non-recursive struct with covariant fields
#[derive(Facet)]
struct Simple {
    x: i32,
    y: i32,
}

#[test]
#[cfg_attr(miri, ignore)] // This is too slow in Miri
fn test_recursive_variance_no_stack_overflow() {
    // This should NOT blow the stack - cycle detection handles it
    let shape = Node::SHAPE;
    let variance = (shape.variance)(shape);

    // Recursive types are handled via cycle detection.
    // When a cycle is detected (same type being computed), we return Covariant
    // as the neutral element - cycles don't contribute new variance information.
    // Since Node only contains covariant fields (i32 and Box<Node>), it's Covariant.
    assert_eq!(
        variance,
        Variance::Covariant,
        "Recursive types with only covariant fields are Covariant"
    );
}

#[test]
fn test_simple_struct_variance() {
    let shape = Simple::SHAPE;
    let variance = (shape.variance)(shape);

    // i32 fields are Covariant, so Simple should be Covariant
    assert_eq!(
        variance,
        Variance::Covariant,
        "Simple should be Covariant since all fields have no lifetime parameters"
    );
}

/// Test that *mut T remains invariant even when T is covariant
#[test]
fn test_mut_ptr_stays_invariant() {
    // *mut T is invariant per Rust reference, regardless of T's variance
    let shape = <*mut i32>::SHAPE;
    let variance = shape.computed_variance();

    assert_eq!(
        variance,
        Variance::Invariant,
        "*mut T must be Invariant regardless of T's variance"
    );
}

/// Test that *const T is covariant (computed from T)
#[test]
fn test_const_ptr_is_covariant() {
    let shape = <*const i32>::SHAPE;
    let variance = shape.computed_variance();

    assert_eq!(
        variance,
        Variance::Covariant,
        "*const T should be Covariant when T is Covariant"
    );
}

/// Test struct containing *mut pointer stays invariant
#[derive(Facet)]
struct WithMutPtr {
    ptr: *mut i32,
}

#[test]
fn test_struct_with_mut_ptr_is_invariant() {
    let shape = WithMutPtr::SHAPE;
    let variance = shape.computed_variance();

    assert_eq!(
        variance,
        Variance::Invariant,
        "Struct containing *mut T must be Invariant"
    );
}

// ============================================================================
// Tests for issue #1704: Exponential variance computation
// ============================================================================

/// A type with multiple self-references - this used to cause exponential blowup.
/// Without cycle detection, computing variance would be O(4^depth) operations.
/// With cycle detection, it's O(number of unique types).
#[derive(Facet)]
struct MultiRecursive {
    #[facet(recursive_type)]
    a: Box<MultiRecursive>,
    #[facet(recursive_type)]
    b: Box<MultiRecursive>,
    #[facet(recursive_type)]
    c: Box<MultiRecursive>,
    #[facet(recursive_type)]
    d: Box<MultiRecursive>,
}

#[test]
fn test_multi_recursive_variance_is_fast() {
    // This test verifies the fix for issue #1704.
    // Before the fix, this would take ~30 seconds.
    // After the fix, it should be instant.
    let start = std::time::Instant::now();
    let shape = MultiRecursive::SHAPE;
    let variance = shape.computed_variance();
    let elapsed = start.elapsed();

    // Should complete in under 100ms (being very generous here)
    assert!(
        cfg!(miri) || elapsed.as_millis() < 100,
        "Variance computation took {:?}, expected < 100ms",
        elapsed
    );

    // All fields are Box<Self> which is covariant, so the result is Covariant
    assert_eq!(
        variance,
        Variance::Covariant,
        "MultiRecursive should be Covariant"
    );
}

/// A recursive type with an invariant field - should be invariant
#[derive(Facet)]
struct RecursiveInvariant {
    ptr: *mut i32,
    #[facet(recursive_type)]
    next: Box<RecursiveInvariant>,
}

#[test]
fn test_recursive_with_invariant_field() {
    let shape = RecursiveInvariant::SHAPE;
    let variance = shape.computed_variance();

    // Should be invariant because it contains *mut i32
    assert_eq!(
        variance,
        Variance::Invariant,
        "RecursiveInvariant should be Invariant due to *mut i32"
    );
}

/// Tests that early termination works - once we see invariant, stop computing
#[derive(Facet)]
struct EarlyTermination {
    // This field makes the struct invariant immediately
    ptr: *mut i32,
    // These fields would take a while to compute without early termination
    #[facet(recursive_type)]
    a: Box<EarlyTermination>,
    #[facet(recursive_type)]
    b: Box<EarlyTermination>,
    #[facet(recursive_type)]
    c: Box<EarlyTermination>,
}

#[test]
fn test_early_termination_on_invariant() {
    let start = std::time::Instant::now();
    let shape = EarlyTermination::SHAPE;
    let variance = shape.computed_variance();
    let elapsed = start.elapsed();

    // Should terminate early when *mut i32 is encountered
    assert!(
        cfg!(miri) || elapsed.as_millis() < 100,
        "Early termination took {:?}, expected < 100ms",
        elapsed
    );

    assert_eq!(
        variance,
        Variance::Invariant,
        "EarlyTermination should be Invariant due to *mut i32"
    );
}

/// Test mutually recursive types
#[derive(Facet)]
struct TreeA {
    value: i32,
    #[facet(recursive_type)]
    children: Vec<TreeB>,
}

#[derive(Facet)]
struct TreeB {
    value: String,
    #[facet(recursive_type)]
    parent: Option<Box<TreeA>>,
}

#[test]
fn test_mutually_recursive_types() {
    let shape_a = TreeA::SHAPE;
    let shape_b = TreeB::SHAPE;

    // Both should complete quickly
    let start = std::time::Instant::now();
    let variance_a = shape_a.computed_variance();
    let variance_b = shape_b.computed_variance();
    let elapsed = start.elapsed();

    assert!(
        cfg!(miri) || elapsed.as_millis() < 100,
        "Mutually recursive variance took {:?}, expected < 100ms",
        elapsed
    );

    // Both contain only covariant fields (i32, String, Vec, Option, Box)
    assert_eq!(variance_a, Variance::Covariant, "TreeA should be Covariant");
    assert_eq!(variance_b, Variance::Covariant, "TreeB should be Covariant");
}

/// Test the exact reproduction case from issue #1704
#[derive(Facet)]
struct IssueNode(
    #[facet(recursive_type)] &'static IssueNode,
    #[facet(recursive_type)] &'static IssueNode,
    #[facet(recursive_type)] &'static IssueNode,
    #[facet(recursive_type)] &'static IssueNode,
);

#[test]
fn test_issue_1704_reproduction() {
    // This is the exact type from issue #1704
    // Before fix: ~30 seconds
    // After fix: instant
    let start = std::time::Instant::now();
    let shape = IssueNode::SHAPE;
    let variance = shape.computed_variance();
    let elapsed = start.elapsed();

    assert!(
        cfg!(miri) || elapsed.as_millis() < 100,
        "Issue #1704 reproduction took {:?}, expected < 100ms",
        elapsed
    );

    // &'static T is covariant, so the result should be Covariant
    assert_eq!(
        variance,
        Variance::Covariant,
        "IssueNode should be Covariant (all fields are &'static Self)"
    );
}
