//! Comprehensive variance tests based on the Rust Reference.
//!
//! See: <https://doc.rust-lang.org/reference/subtyping.html>
//!
//! This file tests every row in the variance table from:
//! <https://doc.rust-lang.org/reference/subtyping.html#r-subtyping.variance.builtin-types>

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
// *const T - covariant in T
// -----------------------------------------------------------------------------

#[test]
fn const_ptr_covariant_in_t() {
    // *const T is covariant in T
    let shape = <*const i32>::SHAPE;
    assert_eq!(
        shape.computed_variance(),
        Variance::Covariant,
        "*const T should be covariant in T (Rust Reference)"
    );
}

#[test]
fn const_ptr_propagates_inner_variance() {
    // *const of a covariant type should be covariant
    let shape = <*const String>::SHAPE;
    assert_eq!(shape.computed_variance(), Variance::Covariant);
}

// -----------------------------------------------------------------------------
// *mut T - invariant in T
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
    // Even if inner type is covariant, *mut T stays invariant
    let shape = <*mut String>::SHAPE;
    assert_eq!(
        shape.computed_variance(),
        Variance::Invariant,
        "*mut T must stay invariant regardless of T's variance"
    );
}

// -----------------------------------------------------------------------------
// [T; N] - covariant in T
// -----------------------------------------------------------------------------

#[test]
fn array_covariant_in_t() {
    // [T; N] is covariant in T
    let shape = <[i32; 5]>::SHAPE;
    assert_eq!(
        shape.computed_variance(),
        Variance::Covariant,
        "[T; N] should be covariant in T (Rust Reference)"
    );
}

#[test]
fn array_propagates_inner_variance() {
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
struct AllCovariantFields {
    a: i32,
    b: String,
    c: bool,
}

#[test]
fn struct_all_covariant_fields() {
    let shape = AllCovariantFields::SHAPE;
    assert_eq!(
        shape.computed_variance(),
        Variance::Covariant,
        "Struct with all covariant fields should be covariant"
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
// Vec<T> - covariant in T (standard library wrapper)
// -----------------------------------------------------------------------------

#[test]
fn vec_covariant_in_t() {
    let shape = <Vec<i32>>::SHAPE;
    assert_eq!(
        shape.computed_variance(),
        Variance::Covariant,
        "Vec<T> should be covariant in T"
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
// Box<T> - covariant in T
// -----------------------------------------------------------------------------

#[test]
fn box_covariant_in_t() {
    let shape = <Box<i32>>::SHAPE;
    assert_eq!(
        shape.computed_variance(),
        Variance::Covariant,
        "Box<T> should be covariant in T"
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
// Option<T> - covariant in T
// -----------------------------------------------------------------------------

#[test]
fn option_covariant_in_t() {
    let shape = <Option<i32>>::SHAPE;
    assert_eq!(
        shape.computed_variance(),
        Variance::Covariant,
        "Option<T> should be covariant in T"
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
// Tuple variance - covariant in each element
// -----------------------------------------------------------------------------

#[test]
fn tuple_all_covariant() {
    let shape = <(i32, String, bool)>::SHAPE;
    assert_eq!(
        shape.computed_variance(),
        Variance::Covariant,
        "Tuple of covariant types should be covariant"
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
enum AllCovariantVariants {
    A(i32),
    B(String),
    C { x: bool, y: u64 },
}

#[test]
fn enum_all_covariant_variants() {
    let shape = AllCovariantVariants::SHAPE;
    assert_eq!(
        shape.computed_variance(),
        Variance::Covariant,
        "Enum with all covariant variants should be covariant"
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
// Scalars - covariant (no lifetime parameters)
// -----------------------------------------------------------------------------

#[test]
fn scalars_are_covariant() {
    assert_eq!(i8::SHAPE.computed_variance(), Variance::Covariant, "i8");
    assert_eq!(i16::SHAPE.computed_variance(), Variance::Covariant, "i16");
    assert_eq!(i32::SHAPE.computed_variance(), Variance::Covariant, "i32");
    assert_eq!(i64::SHAPE.computed_variance(), Variance::Covariant, "i64");
    assert_eq!(i128::SHAPE.computed_variance(), Variance::Covariant, "i128");
    assert_eq!(
        isize::SHAPE.computed_variance(),
        Variance::Covariant,
        "isize"
    );
    assert_eq!(u8::SHAPE.computed_variance(), Variance::Covariant, "u8");
    assert_eq!(u16::SHAPE.computed_variance(), Variance::Covariant, "u16");
    assert_eq!(u32::SHAPE.computed_variance(), Variance::Covariant, "u32");
    assert_eq!(u64::SHAPE.computed_variance(), Variance::Covariant, "u64");
    assert_eq!(u128::SHAPE.computed_variance(), Variance::Covariant, "u128");
    assert_eq!(
        usize::SHAPE.computed_variance(),
        Variance::Covariant,
        "usize"
    );
    assert_eq!(f32::SHAPE.computed_variance(), Variance::Covariant, "f32");
    assert_eq!(f64::SHAPE.computed_variance(), Variance::Covariant, "f64");
    assert_eq!(bool::SHAPE.computed_variance(), Variance::Covariant, "bool");
    assert_eq!(char::SHAPE.computed_variance(), Variance::Covariant, "char");
    assert_eq!(<()>::SHAPE.computed_variance(), Variance::Covariant, "unit");
}

#[test]
fn string_is_covariant() {
    assert_eq!(
        String::SHAPE.computed_variance(),
        Variance::Covariant,
        "String should be covariant (owns its data, no lifetime)"
    );
}

// -----------------------------------------------------------------------------
// Nested containers
// -----------------------------------------------------------------------------

#[test]
fn nested_vec_covariant() {
    let shape = <Vec<Vec<i32>>>::SHAPE;
    assert_eq!(
        shape.computed_variance(),
        Variance::Covariant,
        "Vec<Vec<T>> should be covariant when T is covariant"
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
fn box_of_vec_covariant() {
    let shape = <Box<Vec<i32>>>::SHAPE;
    assert_eq!(
        shape.computed_variance(),
        Variance::Covariant,
        "Box<Vec<T>> should be covariant when T is covariant"
    );
}

#[test]
fn option_of_box_covariant() {
    let shape = <Option<Box<i32>>>::SHAPE;
    assert_eq!(
        shape.computed_variance(),
        Variance::Covariant,
        "Option<Box<T>> should be covariant when T is covariant"
    );
}

// -----------------------------------------------------------------------------
// Complex struct with multiple container types
// -----------------------------------------------------------------------------

#[derive(Facet)]
struct ComplexCovariant {
    vec: Vec<i32>,
    boxed: Box<i32>,
    opt: Option<bool>,
    arr: [u8; 4],
}

#[test]
fn complex_struct_all_covariant() {
    let shape = ComplexCovariant::SHAPE;
    assert_eq!(
        shape.computed_variance(),
        Variance::Covariant,
        "Struct with all covariant container fields should be covariant"
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
