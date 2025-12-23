//! Test variance computation with recursive types.

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
    // This should NOT blow the stack - depth limit should kick in
    let shape = Node::SHAPE;
    let variance = (shape.variance)(shape);

    // i32 is Covariant (scalar with no lifetime), so the whole struct should be Covariant
    assert_eq!(
        variance,
        Variance::Covariant,
        "Node should be Covariant since i32 has no lifetime parameters"
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
