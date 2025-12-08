//! Tests demonstrating that Peek is invariant over 'facet.
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

use facet::Facet;
use facet_reflect::Peek;

#[derive(Debug, Facet)]
struct Borrowed<'a> {
    data: &'a str,
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
