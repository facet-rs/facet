//! Comprehensive variance tests based on the Rust Reference.
//!
//! See: <https://doc.rust-lang.org/reference/subtyping.html>
//!
//! This file tests variance based on the Rust Reference table:
//! <https://doc.rust-lang.org/reference/subtyping.html#r-subtyping.variance.builtin-types>
//!
//! ## Important Note on Bivariance
//!
//! The Rust Reference describes variance "in T" - how a container relates to its
//! type parameter. For example, Vec<T> is "covariant in T", meaning Vec preserves
//! T's subtyping relationship.
//!
//! However, what we compute with `computed_variance()` is the overall variance
//! of the type with respect to lifetimes. When T has no lifetime constraints
//! (is bivariant), the container also has no lifetime constraints.
//!
//! Examples:
//! - i32 is bivariant (no lifetime constraints)
//! - Vec<T> is covariant in T, so Vec<i32> is bivariant (bivariant.combine(bivariant) = bivariant)
//! - *const T is covariant in T, so *const i32 is bivariant
//! - *mut T is invariant in T, so *mut i32 is invariant (invariance dominates)

#![allow(dead_code)] // Test types don't need all fields to be read

use facet::{Facet, Variance};

// =============================================================================
// Table from Rust Reference § Variance of Built-in Types
// https://doc.rust-lang.org/reference/subtyping.html#r-subtyping.variance.builtin-types
//
// | Type                          | Variance in 'a  | Variance in T   |
// |-------------------------------|-----------------|-----------------|
// | &'a T                         | covariant       | covariant       |
// | &'a mut T                     | covariant       | invariant       |
// | *const T                      |                 | covariant       |
// | *mut T                        |                 | invariant       |
// | [T; N]                        |                 | covariant       |
// | [T]                           |                 | covariant       |
// | fn() -> T                     |                 | covariant       |
// | fn(T) -> ()                   |                 | contravariant   |
// | Cell<T>                       |                 | invariant       |
// | UnsafeCell<T>                 |                 | invariant       |
// | PhantomData<T>                |                 | covariant       |
// | dyn Trait<T> + 'a             | covariant       | invariant       |
//
// Note: Cell<T>, UnsafeCell<T>, RefCell<T>, fn types, and dyn Trait do not
// currently implement Facet, so we test them indirectly or skip them.
// =============================================================================

// -----------------------------------------------------------------------------
// *const T - covariant in T (propagates T's variance)
// -----------------------------------------------------------------------------

#[test]
fn const_ptr_propagates_variance() {
    // *const T is covariant in T, meaning it propagates T's variance
    // Since i32 is bivariant (no lifetime constraints), *const i32 is also bivariant
    let shape = <*const i32>::SHAPE;
    assert_eq!(
        shape.computed_variance(),
        Variance::Bivariant,
        "*const T propagates T's variance; *const i32 is bivariant"
    );
}

#[test]
fn const_ptr_propagates_inner_bivariance() {
    // *const of a bivariant type should be bivariant
    let shape = <*const String>::SHAPE;
    assert_eq!(
        shape.computed_variance(),
        Variance::Bivariant,
        "*const String propagates String's bivariance"
    );
}

// -----------------------------------------------------------------------------
// *mut T - invariant in T (always invariant)
// -----------------------------------------------------------------------------

#[test]
fn mut_ptr_invariant_in_t() {
    // *mut T is invariant in T
    let shape = <*mut i32>::SHAPE;
    assert_eq!(
        shape.computed_variance(),
        Variance::Invariant,
        "*mut T should be invariant in T (Rust Reference)"
    );
}

#[test]
fn mut_ptr_stays_invariant_regardless_of_inner() {
    // Even if inner type is bivariant, *mut T stays invariant
    let shape = <*mut String>::SHAPE;
    assert_eq!(
        shape.computed_variance(),
        Variance::Invariant,
        "*mut T must stay invariant regardless of T's variance"
    );
}

// -----------------------------------------------------------------------------
// [T; N] - covariant in T (propagates T's variance)
// -----------------------------------------------------------------------------

#[test]
fn array_propagates_variance() {
    // [T; N] is covariant in T, so [bivariant; N] is bivariant
    let shape = <[i32; 5]>::SHAPE;
    assert_eq!(
        shape.computed_variance(),
        Variance::Bivariant,
        "[i32; N] propagates i32's bivariance"
    );
}

#[test]
fn array_propagates_inner_invariance() {
    // Array of invariant type should be invariant
    #[derive(Facet)]
    struct InvariantWrapper {
        ptr: *mut i32,
    }

    let shape = <[InvariantWrapper; 3]>::SHAPE;
    assert_eq!(
        shape.computed_variance(),
        Variance::Invariant,
        "[T; N] should propagate T's invariance"
    );
}

// -----------------------------------------------------------------------------
// Derived struct variance
// -----------------------------------------------------------------------------

#[derive(Facet)]
struct AllBivariantFields {
    a: i32,
    b: String,
    c: bool,
}

#[test]
fn struct_all_bivariant_fields() {
    let shape = AllBivariantFields::SHAPE;
    assert_eq!(
        shape.computed_variance(),
        Variance::Bivariant,
        "Struct with all bivariant fields should be bivariant"
    );
}

#[derive(Facet)]
struct HasInvariantField {
    a: i32,
    b: *mut u8, // invariant
}

#[test]
fn struct_with_invariant_field() {
    let shape = HasInvariantField::SHAPE;
    assert_eq!(
        shape.computed_variance(),
        Variance::Invariant,
        "Struct with any invariant field should be invariant"
    );
}

#[derive(Facet)]
struct NestedInvariant {
    inner: HasInvariantField,
}

#[test]
fn struct_nested_invariant() {
    let shape = NestedInvariant::SHAPE;
    assert_eq!(
        shape.computed_variance(),
        Variance::Invariant,
        "Struct containing invariant struct should be invariant"
    );
}

// -----------------------------------------------------------------------------
// Vec<T> - covariant in T (propagates T's variance)
// -----------------------------------------------------------------------------

#[test]
fn vec_propagates_bivariance() {
    let shape = <Vec<i32>>::SHAPE;
    assert_eq!(
        shape.computed_variance(),
        Variance::Bivariant,
        "Vec<i32> propagates i32's bivariance"
    );
}

#[test]
fn vec_propagates_invariance() {
    let shape = <Vec<*mut i32>>::SHAPE;
    assert_eq!(
        shape.computed_variance(),
        Variance::Invariant,
        "Vec<*mut T> should be invariant because *mut T is invariant"
    );
}

// -----------------------------------------------------------------------------
// Box<T> - covariant in T (propagates T's variance)
// -----------------------------------------------------------------------------

#[test]
fn box_propagates_bivariance() {
    let shape = <Box<i32>>::SHAPE;
    assert_eq!(
        shape.computed_variance(),
        Variance::Bivariant,
        "Box<i32> propagates i32's bivariance"
    );
}

#[test]
fn box_propagates_invariance() {
    let shape = <Box<*mut i32>>::SHAPE;
    assert_eq!(
        shape.computed_variance(),
        Variance::Invariant,
        "Box<*mut T> should be invariant because *mut T is invariant"
    );
}

// -----------------------------------------------------------------------------
// Option<T> - covariant in T (propagates T's variance)
// -----------------------------------------------------------------------------

#[test]
fn option_propagates_bivariance() {
    let shape = <Option<i32>>::SHAPE;
    assert_eq!(
        shape.computed_variance(),
        Variance::Bivariant,
        "Option<i32> propagates i32's bivariance"
    );
}

#[test]
fn option_propagates_invariance() {
    let shape = <Option<*mut i32>>::SHAPE;
    assert_eq!(
        shape.computed_variance(),
        Variance::Invariant,
        "Option<*mut T> should be invariant because *mut T is invariant"
    );
}

// -----------------------------------------------------------------------------
// Tuple variance - covariant in each element (combines all variances)
// -----------------------------------------------------------------------------

#[test]
fn tuple_all_bivariant() {
    let shape = <(i32, String, bool)>::SHAPE;
    assert_eq!(
        shape.computed_variance(),
        Variance::Bivariant,
        "Tuple of bivariant types should be bivariant"
    );
}

#[test]
fn tuple_with_invariant() {
    let shape = <(i32, *mut u8)>::SHAPE;
    assert_eq!(
        shape.computed_variance(),
        Variance::Invariant,
        "Tuple containing invariant type should be invariant"
    );
}

// -----------------------------------------------------------------------------
// Enum variance
// -----------------------------------------------------------------------------

#[derive(Facet)]
#[repr(u8)]
enum AllBivariantVariants {
    A(i32),
    B(String),
    C { x: bool, y: u64 },
}

#[test]
fn enum_all_bivariant_variants() {
    let shape = AllBivariantVariants::SHAPE;
    assert_eq!(
        shape.computed_variance(),
        Variance::Bivariant,
        "Enum with all bivariant variants should be bivariant"
    );
}

#[derive(Facet)]
#[repr(u8)]
enum HasInvariantVariant {
    A(i32),
    B(*mut u8), // invariant
}

#[test]
fn enum_with_invariant_variant() {
    let shape = HasInvariantVariant::SHAPE;
    assert_eq!(
        shape.computed_variance(),
        Variance::Invariant,
        "Enum with any invariant variant should be invariant"
    );
}

// -----------------------------------------------------------------------------
// Scalars - bivariant (no lifetime parameters)
// -----------------------------------------------------------------------------

#[test]
fn scalars_are_bivariant() {
    assert_eq!(i8::SHAPE.computed_variance(), Variance::Bivariant, "i8");
    assert_eq!(i16::SHAPE.computed_variance(), Variance::Bivariant, "i16");
    assert_eq!(i32::SHAPE.computed_variance(), Variance::Bivariant, "i32");
    assert_eq!(i64::SHAPE.computed_variance(), Variance::Bivariant, "i64");
    assert_eq!(i128::SHAPE.computed_variance(), Variance::Bivariant, "i128");
    assert_eq!(
        isize::SHAPE.computed_variance(),
        Variance::Bivariant,
        "isize"
    );
    assert_eq!(u8::SHAPE.computed_variance(), Variance::Bivariant, "u8");
    assert_eq!(u16::SHAPE.computed_variance(), Variance::Bivariant, "u16");
    assert_eq!(u32::SHAPE.computed_variance(), Variance::Bivariant, "u32");
    assert_eq!(u64::SHAPE.computed_variance(), Variance::Bivariant, "u64");
    assert_eq!(u128::SHAPE.computed_variance(), Variance::Bivariant, "u128");
    assert_eq!(
        usize::SHAPE.computed_variance(),
        Variance::Bivariant,
        "usize"
    );
    assert_eq!(f32::SHAPE.computed_variance(), Variance::Bivariant, "f32");
    assert_eq!(f64::SHAPE.computed_variance(), Variance::Bivariant, "f64");
    assert_eq!(bool::SHAPE.computed_variance(), Variance::Bivariant, "bool");
    assert_eq!(char::SHAPE.computed_variance(), Variance::Bivariant, "char");
    assert_eq!(<()>::SHAPE.computed_variance(), Variance::Bivariant, "unit");
}

#[test]
fn string_is_bivariant() {
    assert_eq!(
        String::SHAPE.computed_variance(),
        Variance::Bivariant,
        "String should be bivariant (owns its data, no lifetime)"
    );
}

// -----------------------------------------------------------------------------
// Nested containers - propagate inner variance
// -----------------------------------------------------------------------------

#[test]
fn nested_vec_bivariant() {
    let shape = <Vec<Vec<i32>>>::SHAPE;
    assert_eq!(
        shape.computed_variance(),
        Variance::Bivariant,
        "Vec<Vec<i32>> propagates i32's bivariance"
    );
}

#[test]
fn nested_vec_invariant() {
    let shape = <Vec<Vec<*mut i32>>>::SHAPE;
    assert_eq!(
        shape.computed_variance(),
        Variance::Invariant,
        "Vec<Vec<*mut T>> should be invariant"
    );
}

#[test]
fn box_of_vec_bivariant() {
    let shape = <Box<Vec<i32>>>::SHAPE;
    assert_eq!(
        shape.computed_variance(),
        Variance::Bivariant,
        "Box<Vec<i32>> propagates i32's bivariance"
    );
}

#[test]
fn option_of_box_bivariant() {
    let shape = <Option<Box<i32>>>::SHAPE;
    assert_eq!(
        shape.computed_variance(),
        Variance::Bivariant,
        "Option<Box<i32>> propagates i32's bivariance"
    );
}

// -----------------------------------------------------------------------------
// Complex struct with multiple container types
// -----------------------------------------------------------------------------

#[derive(Facet)]
struct ComplexBivariant {
    vec: Vec<i32>,
    boxed: Box<i32>,
    opt: Option<bool>,
    arr: [u8; 4],
}

#[test]
fn complex_struct_all_bivariant() {
    let shape = ComplexBivariant::SHAPE;
    assert_eq!(
        shape.computed_variance(),
        Variance::Bivariant,
        "Struct with all bivariant container fields should be bivariant"
    );
}

#[derive(Facet)]
struct ComplexWithOneInvariant {
    vec: Vec<i32>,
    ptr: *mut u8, // This makes the whole struct invariant
    opt: Option<bool>,
}

#[test]
fn complex_struct_one_invariant_field() {
    let shape = ComplexWithOneInvariant::SHAPE;
    assert_eq!(
        shape.computed_variance(),
        Variance::Invariant,
        "Struct with one invariant field should be invariant"
    );
}
