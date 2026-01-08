//! Tests demonstrating that Peek is invariant with respect to 'facet, with variance-aware
//! lifetime transformation methods.
//!
//! Invariance means Peek<'mem, 'a> cannot be cast to Peek<'mem, 'b> even if 'a: 'b.
//!
//! This is REQUIRED for soundness! If Peek were covariant with respect to 'facet, we could:
//! 1. Create Peek<'mem, 'static> from FnWrapper<'static> (contains fn(&'static str))
//! 2. Use covariance to cast it to Peek<'mem, 'short>
//! 3. Call get::<FnWrapper<'short>>() to get &FnWrapper<'short>
//! 4. This would allow calling the function with a &'short str that goes out of scope
//!    while the original function pointer still holds it as 'static
//!
//! See: https://github.com/facet-rs/facet/issues/1168
//!
//! However, for types that are *known* to be covariant (like `&'a str` or structs
//! containing only covariant fields), we can safely shrink lifetimes. The
//! `shrink_lifetime` and `try_shrink_lifetime` methods enable this by checking
//! the type's variance at runtime.

use facet::{Facet, Variance};
use facet_reflect::Peek;

#[derive(Debug, Facet)]
struct Borrowed<'a> {
    data: &'a str,
}

/// A struct with an invariant field (*mut T is invariant)
#[derive(Debug, Facet)]
struct WithMutPtr {
    ptr: *mut i32,
}

/// Demonstrates that Peek can be created and used with the same lifetime.
/// This is the safe way to use Peek - keeping the 'facet lifetime consistent.
#[test]
fn peek_same_lifetime_works() {
    static STATIC_DATA: &str = "I am truly static";
    let borrowed_static: Borrowed<'static> = Borrowed { data: STATIC_DATA };

    // Create a Peek<'_, 'static>
    let peek_static: Peek<'_, 'static> = Peek::new(&borrowed_static);

    // Using the same 'static lifetime works fine
    let borrowed: &Borrowed<'static> = peek_static.get::<Borrowed<'static>>().unwrap();
    assert_eq!(borrowed.data, "I am truly static");
}

/// Shows that Peek works correctly with non-static lifetimes too.
#[test]
fn peek_with_shorter_lifetime() {
    let owned = String::from("owned data");
    let borrowed: Borrowed<'_> = Borrowed { data: &owned };

    // Peek's 'facet lifetime is tied to the owned string's lifetime
    let peek: Peek<'_, '_> = Peek::new(&borrowed);

    // We can get the value back with the correct lifetime
    let result: &Borrowed<'_> = peek.get::<Borrowed<'_>>().unwrap();
    assert_eq!(result.data, "owned data");
}

// Note: The compile_tests/ directory contains tests that verify Peek's
// invariance is properly enforced at compile time. Those tests ensure
// that code attempting to launder lifetimes through Peek fails to compile.

// =============================================================================
// Variance-aware lifetime transformation tests
// =============================================================================

/// Test that we can query the variance of a Peek
#[test]
fn peek_variance_query() {
    // Borrowed<'a> contains &'a str which is covariant
    let borrowed = Borrowed { data: "hello" };
    let peek = Peek::new(&borrowed);
    assert_eq!(peek.variance(), Variance::Covariant);

    // WithMutPtr contains *mut i32 which is invariant
    let with_ptr = WithMutPtr {
        ptr: std::ptr::null_mut(),
    };
    let peek = Peek::new(&with_ptr);
    assert_eq!(peek.variance(), Variance::Invariant);
}

/// Test that shrink_lifetime works for covariant types
#[test]
fn shrink_lifetime_covariant() {
    static STATIC_DATA: &str = "I am truly static";
    let borrowed: Borrowed<'static> = Borrowed { data: STATIC_DATA };
    let peek: Peek<'_, 'static> = Peek::new(&borrowed);

    // This function requires a shorter lifetime
    fn use_shorter<'a>(peek: Peek<'_, 'a>) -> &'a str {
        peek.get::<Borrowed<'a>>().unwrap().data
    }

    // shrink_lifetime should work because Borrowed is covariant
    let result = use_shorter(peek.shrink_lifetime());
    assert_eq!(result, "I am truly static");
}

/// Test that try_shrink_lifetime returns Some for covariant types
#[test]
fn try_shrink_lifetime_covariant() {
    static STATIC_DATA: &str = "I am truly static";
    let borrowed: Borrowed<'static> = Borrowed { data: STATIC_DATA };
    let peek: Peek<'_, 'static> = Peek::new(&borrowed);

    fn use_shorter<'a>(peek: Peek<'_, 'a>) -> &'a str {
        peek.get::<Borrowed<'a>>().unwrap().data
    }

    // try_shrink_lifetime should return Some because Borrowed is covariant
    let shrunk = peek.try_shrink_lifetime();
    assert!(shrunk.is_some());
    let result = use_shorter(shrunk.unwrap());
    assert_eq!(result, "I am truly static");
}

/// Test that try_shrink_lifetime returns None for invariant types
#[test]
fn try_shrink_lifetime_invariant_returns_none() {
    let with_ptr = WithMutPtr {
        ptr: std::ptr::null_mut(),
    };
    let peek: Peek<'_, 'static> = Peek::new(&with_ptr);

    // try_shrink_lifetime should return None because WithMutPtr is invariant
    let result: Option<Peek<'_, '_>> = peek.try_shrink_lifetime();
    assert!(result.is_none());
}

/// Test that shrink_lifetime panics for invariant types
#[test]
#[should_panic(expected = "shrink_lifetime requires a covariant type")]
fn shrink_lifetime_invariant_panics() {
    let with_ptr = WithMutPtr {
        ptr: std::ptr::null_mut(),
    };
    let peek: Peek<'_, 'static> = Peek::new(&with_ptr);

    // This should panic because WithMutPtr is invariant
    let _: Peek<'_, '_> = peek.shrink_lifetime();
}

/// Soundness test for GitHub issue #1664 and #1708
///
/// With bivariance support (issue #1708), function pointers with bivariant
/// arguments and return types are now correctly identified as bivariant.
/// `fn() -> i32` is bivariant because i32 has no lifetime constraints.
///
/// For soundness, function pointers with lifetime-carrying types like `fn(&'a str)`
/// would be contravariant, but we can't easily test that without HRTB.
#[test]
#[cfg(feature = "fn-ptr")]
fn shrink_lifetime_fn_ptr_bivariant_succeeds() {
    // fn() -> i32 has no lifetime constraints - it's bivariant
    // Bivariant types can shrink lifetimes (can_shrink() returns true)
    let fn_ptr: fn() -> i32 = || 42;
    let peek = Peek::new(&fn_ptr);

    // With bivariance support, this should succeed because fn() -> i32 is bivariant
    // Both arguments (none) and return type (i32) are bivariant
    let shrunk: Peek<'_, '_> = peek.shrink_lifetime();
    let _ = shrunk;
}

/// Test that try_grow_lifetime returns None for covariant types
///
/// Borrowed<'a> contains &'a str which is covariant.
/// Covariant types can only shrink lifetimes, not grow them.
#[test]
fn try_grow_lifetime_covariant_returns_none() {
    let borrowed = Borrowed { data: "hello" };
    let peek = Peek::new(&borrowed);

    // Borrowed<'a> is covariant (contains &'a str)
    assert_eq!(peek.variance(), Variance::Covariant);

    // try_grow_lifetime should return None because Borrowed is covariant, not contravariant
    let result: Option<Peek<'_, 'static>> = peek.try_grow_lifetime();
    assert!(result.is_none());
}

/// Test that grow_lifetime panics for covariant types
#[test]
#[should_panic(expected = "grow_lifetime requires a contravariant type")]
fn grow_lifetime_covariant_panics() {
    let borrowed = Borrowed { data: "hello" };
    let peek = Peek::new(&borrowed);

    // Borrowed<'a> is covariant (contains &'a str)
    // This should panic because covariant types cannot grow lifetimes
    let _: Peek<'_, 'static> = peek.grow_lifetime();
}

/// Test that grow_lifetime panics for invariant types
#[test]
#[should_panic(expected = "grow_lifetime requires a contravariant type")]
fn grow_lifetime_invariant_panics() {
    let with_ptr = WithMutPtr {
        ptr: std::ptr::null_mut(),
    };
    let peek = Peek::new(&with_ptr);

    // This should panic because WithMutPtr is invariant, not contravariant
    let _: Peek<'_, 'static> = peek.grow_lifetime();
}

/// Test shrink_lifetime with nested bivariant types
///
/// Vec<String> is bivariant because String is bivariant (no lifetime constraints)
/// and Vec preserves String's variance.
/// Bivariant types can shrink lifetimes (can_shrink() returns true).
#[test]
fn shrink_lifetime_nested_bivariant() {
    // Vec<String> is bivariant - Vec propagates String's bivariance
    let vec: Vec<String> = vec!["hello".to_string(), "world".to_string()];
    let peek: Peek<'_, 'static> = Peek::new(&vec);

    assert_eq!(
        peek.variance(),
        Variance::Bivariant,
        "Vec<String> should be bivariant (String is bivariant)"
    );

    fn use_shorter<'a>(peek: Peek<'_, 'a>) {
        let _ = peek;
    }

    // Should work because Vec<String> is bivariant (can_shrink() returns true)
    use_shorter(peek.shrink_lifetime());
}

/// Test that Option<T> propagates variance correctly
///
/// Option<T> is covariant with respect to T, meaning it preserves T's variance:
/// - Option<bivariant> = bivariant
/// - Option<covariant> = covariant
/// - Option<invariant> = invariant
#[test]
fn option_variance_propagation() {
    // String is bivariant (no lifetime constraints)
    // Option<String> propagates String's bivariance = bivariant
    let opt: Option<String> = Some("hello".to_string());
    let peek = Peek::new(&opt);
    assert_eq!(
        peek.variance(),
        Variance::Bivariant,
        "Option<String> should be bivariant (String is bivariant)"
    );

    // *mut i32 is invariant
    // Option<*mut i32> propagates *mut i32's invariance = invariant
    let opt_ptr: Option<*mut i32> = None;
    let peek = Peek::new(&opt_ptr);
    assert_eq!(
        peek.variance(),
        Variance::Invariant,
        "Option<*mut i32> should be invariant (*mut i32 is invariant)"
    );
}

/// Soundness test for GitHub issues #1696 and #1708
///
/// With bivariance support, `fn() -> i32` is bivariant (no lifetime constraints).
/// `&T` is covariant with respect to T, so `&fn() -> i32` combines Covariant with Bivariant = Covariant.
///
/// From the Rust Reference (https://doc.rust-lang.org/reference/subtyping.html):
/// - &'a T is covariant with respect to 'a and covariant with respect to T
/// - fn() -> i32 is bivariant (i32 has no lifetime constraints)
/// - Covariant.combine(Bivariant) = Covariant
#[test]
#[cfg(feature = "fn-ptr")]
fn reference_to_bivariant_fn_ptr_is_covariant() {
    // fn() -> i32 is bivariant (no lifetime constraints in args or return)
    let fn_ptr: fn() -> i32 = || 42;
    let ref_to_fn: &fn() -> i32 = &fn_ptr;
    let peek = Peek::new(&ref_to_fn);

    // &fn() should be covariant because:
    // - &T is covariant with respect to T
    // - fn() -> i32 is bivariant
    // - Covariant.combine(Bivariant) = Covariant
    assert_eq!(
        peek.variance(),
        Variance::Covariant,
        "Reference to bivariant fn pointer should be covariant"
    );
}

/// With bivariance support (issue #1708), &fn() -> i32 is now covariant
/// because fn() -> i32 is bivariant (no lifetime constraints).
///
/// shrink_lifetime should succeed for covariant types.
#[test]
#[cfg(feature = "fn-ptr")]
fn shrink_lifetime_ref_to_bivariant_fn_ptr_succeeds() {
    let fn_ptr: fn() -> i32 = || 42;
    let ref_to_fn: &fn() -> i32 = &fn_ptr;
    let peek = Peek::new(&ref_to_fn);

    // &fn() -> i32 is covariant (Covariant.combine(Bivariant) = Covariant)
    // shrink_lifetime should succeed
    let shrunk: Peek<'_, '_> = peek.shrink_lifetime();
    let _ = shrunk;
}

/// Test that &'a mut T variance depends on T's variance
///
/// - `&'a mut T` is covariant with respect to 'a and invariant with respect to T
/// - If `T` contributes `Bivariant`, then `&mut T` is `Covariant` (only the lifetime matters)
/// - Otherwise, `&mut T` is `Invariant` (the invariant dependency forces it)
#[test]
fn mut_ref_variance_depends_on_inner() {
    // i32 is bivariant (no lifetime constraints)
    // &mut i32 is covariant (covariant with respect to 'a, invariant dependency on bivariant T = covariant)
    let mut value: i32 = 42;
    let mut_ref: &mut i32 = &mut value;
    let peek = Peek::new(&mut_ref);
    assert_eq!(
        peek.variance(),
        Variance::Covariant,
        "&mut bivariant should be covariant (from the lifetime)"
    );
}

/// Test that &T combines Covariant with T's variance correctly
///
/// From the Rust Reference (https://doc.rust-lang.org/reference/subtyping.html):
/// &'a T is covariant with respect to 'a and covariant with respect to T
///
/// This means &T combines Covariant with T's variance:
/// - &bivariant = covariant (Covariant.combine(Bivariant) = Covariant)
/// - &covariant = covariant (Covariant.combine(Covariant) = Covariant)
/// - &invariant = invariant (Covariant.combine(Invariant) = Invariant)
#[test]
fn shared_ref_combines_variance() {
    // i32 is bivariant (no lifetime constraints)
    // &i32 combines Covariant with Bivariant = Covariant
    let value: i32 = 42;
    let ref_to_i32: &i32 = &value;
    let peek = Peek::new(&ref_to_i32);
    assert_eq!(
        peek.variance(),
        Variance::Covariant,
        "&i32 should be covariant (Covariant.combine(Bivariant) = Covariant)"
    );

    // &i32 is covariant, so &&i32 combines Covariant with Covariant = Covariant
    let ref_ref_to_i32: &&i32 = &&42;
    let peek = Peek::new(&ref_ref_to_i32);
    assert_eq!(
        peek.variance(),
        Variance::Covariant,
        "&&i32 should be covariant (Covariant.combine(Covariant) = Covariant)"
    );

    // *mut i32 is invariant
    // &*mut i32 combines Covariant with Invariant = Invariant
    let ptr: *mut i32 = std::ptr::null_mut();
    let ref_to_ptr: &*mut i32 = &ptr;
    let peek = Peek::new(&ref_to_ptr);
    assert_eq!(
        peek.variance(),
        Variance::Invariant,
        "&*mut i32 should be invariant (Covariant.combine(Invariant) = Invariant)"
    );
}
