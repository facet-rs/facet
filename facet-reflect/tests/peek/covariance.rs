//! Tests demonstrating that Peek is invariant over 'facet, with variance-aware
//! lifetime transformation methods.
//!
//! Invariance means Peek<'mem, 'a> cannot be cast to Peek<'mem, 'b> even if 'a: 'b.
//!
//! This is REQUIRED for soundness! If Peek were covariant over 'facet, we could:
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

/// Soundness test for GitHub issue #1664
///
/// Function pointers must be invariant to prevent lifetime manipulation that
/// could allow storing short-lived references in static storage.
#[test]
#[cfg(feature = "fn-ptr")]
#[should_panic(expected = "shrink_lifetime requires a covariant type")]
fn shrink_lifetime_fn_ptr_panics() {
    // Create a Peek wrapping a function pointer
    // Use a function pointer with no parameters to avoid HRTB issues
    let fn_ptr: fn() -> i32 = || 42;
    let peek = Peek::new(&fn_ptr);

    // This should panic because function pointers are invariant
    // Before the fix for #1664, this would succeed and allow UB
    let _: Peek<'_, '_> = peek.shrink_lifetime();
}

/// Test that try_grow_lifetime returns None for covariant types
#[test]
fn try_grow_lifetime_covariant_returns_none() {
    let borrowed = Borrowed { data: "hello" };
    let peek = Peek::new(&borrowed);

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

    // This should panic because Borrowed is covariant, not contravariant
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

/// Test shrink_lifetime with nested covariant types
#[test]
fn shrink_lifetime_nested_covariant() {
    // Vec<String> is covariant - Vec is covariant in T, String has no lifetime
    let vec: Vec<String> = vec!["hello".to_string(), "world".to_string()];
    let peek: Peek<'_, 'static> = Peek::new(&vec);

    assert_eq!(peek.variance(), Variance::Covariant);

    fn use_shorter<'a>(peek: Peek<'_, 'a>) {
        let _ = peek;
    }

    // Should work because Vec<String> is covariant
    use_shorter(peek.shrink_lifetime());
}

/// Test that Option<T> propagates variance correctly
#[test]
fn option_variance_propagation() {
    // Option<String> should be covariant
    let opt: Option<String> = Some("hello".to_string());
    let peek = Peek::new(&opt);
    assert_eq!(peek.variance(), Variance::Covariant);

    // Option<*mut i32> should be invariant
    let opt_ptr: Option<*mut i32> = None;
    let peek = Peek::new(&opt_ptr);
    assert_eq!(peek.variance(), Variance::Invariant);
}

/// Soundness test for GitHub issue #1696
///
/// References to function pointers must propagate the function pointer's variance.
/// `&fn(...)` should be invariant because `fn(...)` is invariant.
/// Before the fix, `&T` always reported Covariant regardless of T's variance.
#[test]
#[cfg(feature = "fn-ptr")]
fn reference_to_fn_ptr_variance() {
    // A reference to a function pointer should be invariant
    let fn_ptr: fn() -> i32 = || 42;
    let ref_to_fn: &fn() -> i32 = &fn_ptr;
    let peek = Peek::new(&ref_to_fn);

    // &fn() should be invariant because fn() is invariant
    assert_eq!(
        peek.variance(),
        Variance::Invariant,
        "Reference to fn pointer should propagate fn's invariance"
    );
}

/// Soundness test for GitHub issue #1696
///
/// This test verifies that shrink_lifetime correctly rejects references to
/// function pointers, which prevented a soundness bug where contravariant
/// function arguments could be exploited.
#[test]
#[cfg(feature = "fn-ptr")]
#[should_panic(expected = "shrink_lifetime requires a covariant type")]
fn shrink_lifetime_ref_to_fn_ptr_panics() {
    let fn_ptr: fn() -> i32 = || 42;
    let ref_to_fn: &fn() -> i32 = &fn_ptr;
    let peek = Peek::new(&ref_to_fn);

    // This should panic because &fn() is invariant (fn is invariant)
    let _: Peek<'_, '_> = peek.shrink_lifetime();
}

/// Test that &mut T is invariant regardless of T's variance
#[test]
fn mut_ref_is_invariant() {
    let mut value: i32 = 42;
    let mut_ref: &mut i32 = &mut value;
    let peek = Peek::new(&mut_ref);

    // &mut T is always invariant in T
    assert_eq!(
        peek.variance(),
        Variance::Invariant,
        "&mut T should always be invariant"
    );
}

/// Test that &T propagates T's variance correctly
#[test]
fn shared_ref_propagates_variance() {
    // &i32 should be covariant (i32 is covariant)
    let value: i32 = 42;
    let ref_to_i32: &i32 = &value;
    let peek = Peek::new(&ref_to_i32);
    assert_eq!(
        peek.variance(),
        Variance::Covariant,
        "&i32 should be covariant"
    );

    // &&i32 should also be covariant
    let ref_ref_to_i32: &&i32 = &&42;
    let peek = Peek::new(&ref_ref_to_i32);
    assert_eq!(
        peek.variance(),
        Variance::Covariant,
        "&&i32 should be covariant"
    );

    // &*mut i32 should be invariant (*mut i32 is invariant)
    let ptr: *mut i32 = std::ptr::null_mut();
    let ref_to_ptr: &*mut i32 = &ptr;
    let peek = Peek::new(&ref_to_ptr);
    assert_eq!(
        peek.variance(),
        Variance::Invariant,
        "&*mut i32 should be invariant"
    );
}
