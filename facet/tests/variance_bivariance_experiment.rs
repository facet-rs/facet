//! Tests for issue #1708: Bivariance support
//!
//! ## The Problem (Before)
//!
//! `facet_core::Variance` only had Covariant, Contravariant, and Invariant.
//!
//! Some types are "bivariant" - they are both covariant (`T<'long> <: T<'short>`)
//! AND contravariant (`T<'short> <: T<'long>`). This includes all types containing
//! none of: lifetimes, references, or interior mutability.
//!
//! Without bivariance, we couldn't accurately compute `fn(T)` variance when T had
//! no lifetime constraints.
//!
//! ## The Solution (After)
//!
//! Added `Variance::Bivariant` - the "top" of the variance lattice:
//!
//! ```text
//!        Bivariant (top - no constraints)
//!        /        \
//!   Covariant    Contravariant
//!        \        /
//!        Invariant (bottom - maximum constraints)
//! ```
//!
//! Properties:
//! - Bivariant is the identity for combine: `Bivariant.combine(X) == X`
//! - Bivariant is stable under flip: `Bivariant.flip() == Bivariant`
//! - Bivariant means "no lifetime constraints"

use facet::{Facet, Variance};

// =============================================================================
// Bivariance for lifetime-free types
// =============================================================================

#[test]
fn primitives_are_bivariant() {
    // Types with no lifetime dependency are bivariant (can go either direction)
    assert_eq!(i32::SHAPE.computed_variance(), Variance::Bivariant);
    assert_eq!(String::SHAPE.computed_variance(), Variance::Bivariant);
    assert_eq!(bool::SHAPE.computed_variance(), Variance::Bivariant);
    assert_eq!(u64::SHAPE.computed_variance(), Variance::Bivariant);
    assert_eq!(f64::SHAPE.computed_variance(), Variance::Bivariant);
    assert_eq!(<()>::SHAPE.computed_variance(), Variance::Bivariant);
}

#[test]
fn unit_struct_is_bivariant() {
    #[derive(Facet)]
    struct Unit;

    assert_eq!(
        Unit::SHAPE.computed_variance(),
        Variance::Bivariant,
        "Empty struct with no lifetime-carrying fields is bivariant"
    );
}

#[test]
fn struct_with_only_primitives_is_bivariant() {
    #[derive(Facet)]
    struct OnlyPrimitives {
        a: i32,
        b: bool,
        c: f64,
    }

    assert_eq!(
        OnlyPrimitives::SHAPE.computed_variance(),
        Variance::Bivariant,
        "Struct with only primitive fields is bivariant"
    );
}

// =============================================================================
// Function pointer variance (the key improvement from issue #1708)
// =============================================================================

#[test]
#[cfg(feature = "fn-ptr")]
fn fn_ptr_with_bivariant_args_is_bivariant() {
    // fn(i32) -> i32 has no lifetime constraints
    // - i32 argument is bivariant, flip(bivariant) = bivariant
    // - i32 return is bivariant
    // - bivariant.combine(bivariant) = bivariant
    let shape = <fn(i32) -> i32>::SHAPE;
    assert_eq!(
        shape.computed_variance(),
        Variance::Bivariant,
        "fn(i32) -> i32 should be bivariant (no lifetime constraints)"
    );
}

#[test]
#[cfg(feature = "fn-ptr")]
fn fn_ptr_returning_unit_is_bivariant() {
    // fn() -> () has no lifetime constraints
    let shape = <fn()>::SHAPE;
    assert_eq!(
        shape.computed_variance(),
        Variance::Bivariant,
        "fn() should be bivariant"
    );
}

#[test]
#[cfg(feature = "fn-ptr")]
fn fn_ptr_with_multiple_bivariant_args_is_bivariant() {
    // fn(i32, String, bool) -> f64 has no lifetime constraints
    let shape = <fn(i32, String, bool) -> f64>::SHAPE;
    assert_eq!(
        shape.computed_variance(),
        Variance::Bivariant,
        "fn with all bivariant args and return is bivariant"
    );
}

// =============================================================================
// Struct with fn pointer that has no lifetime constraints
// =============================================================================

#[test]
#[cfg(feature = "fn-ptr")]
fn struct_with_bivariant_fn_ptr_is_bivariant() {
    #[derive(Facet)]
    struct Callback {
        f: fn(i32) -> i32,
    }

    // This struct has no actual lifetime dependency!
    // With bivariance support, we can now correctly identify this.
    let shape = Callback::SHAPE;
    assert_eq!(
        shape.computed_variance(),
        Variance::Bivariant,
        "Struct with bivariant fn pointer is bivariant"
    );
}

// =============================================================================
// Variance algebra with bivariance
// =============================================================================

#[test]
fn bivariant_is_identity_for_combine() {
    // Bivariant.combine(X) == X for all X
    assert_eq!(
        Variance::Bivariant.combine(Variance::Bivariant),
        Variance::Bivariant
    );
    assert_eq!(
        Variance::Bivariant.combine(Variance::Covariant),
        Variance::Covariant
    );
    assert_eq!(
        Variance::Bivariant.combine(Variance::Contravariant),
        Variance::Contravariant
    );
    assert_eq!(
        Variance::Bivariant.combine(Variance::Invariant),
        Variance::Invariant
    );

    // X.combine(Bivariant) == X for all X (commutativity)
    assert_eq!(
        Variance::Covariant.combine(Variance::Bivariant),
        Variance::Covariant
    );
    assert_eq!(
        Variance::Contravariant.combine(Variance::Bivariant),
        Variance::Contravariant
    );
    assert_eq!(
        Variance::Invariant.combine(Variance::Bivariant),
        Variance::Invariant
    );
}

#[test]
fn invariant_still_dominates() {
    // Invariant is the "bottom" of the lattice
    assert_eq!(
        Variance::Covariant.combine(Variance::Invariant),
        Variance::Invariant
    );
    assert_eq!(
        Variance::Contravariant.combine(Variance::Invariant),
        Variance::Invariant
    );
    assert_eq!(
        Variance::Bivariant.combine(Variance::Invariant),
        Variance::Invariant
    );
}

#[test]
fn mixed_co_contra_is_invariant() {
    // Covariant + Contravariant = Invariant (conflicting constraints)
    assert_eq!(
        Variance::Covariant.combine(Variance::Contravariant),
        Variance::Invariant
    );
    assert_eq!(
        Variance::Contravariant.combine(Variance::Covariant),
        Variance::Invariant
    );
}

// =============================================================================
// Flip behavior with bivariance
// =============================================================================

#[test]
fn bivariant_flip_is_bivariant() {
    // Bivariant has no constraints to flip
    assert_eq!(Variance::Bivariant.flip(), Variance::Bivariant);
}

#[test]
fn flip_swaps_co_and_contra() {
    assert_eq!(Variance::Covariant.flip(), Variance::Contravariant);
    assert_eq!(Variance::Contravariant.flip(), Variance::Covariant);
}

#[test]
fn invariant_flip_is_invariant() {
    assert_eq!(Variance::Invariant.flip(), Variance::Invariant);
}

// =============================================================================
// can_shrink and can_grow with bivariance
// =============================================================================

#[test]
fn bivariant_can_shrink_and_grow() {
    // Bivariant types have no lifetime constraints, so both are allowed
    assert!(Variance::Bivariant.can_shrink(), "Bivariant can shrink");
    assert!(Variance::Bivariant.can_grow(), "Bivariant can grow");
}

#[test]
fn covariant_can_only_shrink() {
    assert!(Variance::Covariant.can_shrink(), "Covariant can shrink");
    assert!(!Variance::Covariant.can_grow(), "Covariant cannot grow");
}

#[test]
fn contravariant_can_only_grow() {
    assert!(
        !Variance::Contravariant.can_shrink(),
        "Contravariant cannot shrink"
    );
    assert!(Variance::Contravariant.can_grow(), "Contravariant can grow");
}

#[test]
fn invariant_cannot_shrink_or_grow() {
    assert!(!Variance::Invariant.can_shrink(), "Invariant cannot shrink");
    assert!(!Variance::Invariant.can_grow(), "Invariant cannot grow");
}

// =============================================================================
// Covariance still works correctly
// =============================================================================

#[test]
fn reference_is_covariant() {
    // From the Rust Reference:
    // &'a T is covariant in 'a and covariant in T
    // https://doc.rust-lang.org/reference/subtyping.html#r-subtyping.variance.builtin-types
    //
    // Even though i32 is bivariant (no lifetime constraints), &i32 introduces
    // a reference lifetime, making it covariant.
    // Covariant.combine(Bivariant) = Covariant
    let shape = <&i32>::SHAPE;
    assert_eq!(
        shape.computed_variance(),
        Variance::Covariant,
        "&i32 is covariant (reference introduces lifetime)"
    );
}

#[test]
fn box_propagates_inner_variance() {
    // Box<T> propagates T's variance
    let shape = <Box<i32>>::SHAPE;
    assert_eq!(
        shape.computed_variance(),
        Variance::Bivariant,
        "Box<i32> propagates i32's bivariance"
    );
}

#[test]
fn vec_propagates_inner_variance() {
    let shape = <Vec<i32>>::SHAPE;
    assert_eq!(
        shape.computed_variance(),
        Variance::Bivariant,
        "Vec<i32> propagates i32's bivariance"
    );
}

// =============================================================================
// Invariance still works correctly
// =============================================================================

#[test]
fn mut_ptr_is_invariant() {
    // *mut T is invariant (can't change the pointee)
    let shape = <*mut i32>::SHAPE;
    assert_eq!(
        shape.computed_variance(),
        Variance::Invariant,
        "*mut T is invariant regardless of T"
    );
}

#[test]
fn struct_with_mut_ptr_is_invariant() {
    #[derive(Facet)]
    struct WithMutPtr {
        ptr: *mut i32,
    }

    assert_eq!(
        WithMutPtr::SHAPE.computed_variance(),
        Variance::Invariant,
        "Struct containing *mut T is invariant"
    );
}
