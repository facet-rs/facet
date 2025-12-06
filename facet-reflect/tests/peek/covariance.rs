//! Tests demonstrating that Peek is covariant over 'facet.
//!
//! Covariance means Peek<'mem, 'static> can be used where Peek<'mem, 'a> is expected.
//! This is safe because:
//! - Covariance only allows shrinking lifetimes ('static -> 'a), not growing
//! - Data valid for 'static is valid for any shorter lifetime
//! - The 'mem lifetime and borrow checker prevent references from escaping
//!
//! See: https://github.com/facet-rs/facet/discussions/1128

use facet::Facet;
use facet_reflect::Peek;

#[derive(Debug, Facet)]
struct Borrowed<'a> {
    data: &'a str,
}

/// Demonstrates that Peek<'_, 'static> can be passed where Peek<'_, 'a> is expected.
#[test]
fn peek_static_usable_as_shorter_lifetime() {
    static STATIC_DATA: &str = "I am truly static";
    let borrowed_static: Borrowed<'static> = Borrowed { data: STATIC_DATA };

    // Create a Peek<'_, 'static>
    let peek_static: Peek<'_, 'static> = Peek::new(&borrowed_static);

    // This function accepts Peek<'_, 'a> for any 'a
    fn use_peek<'a>(peek: Peek<'_, 'a>) -> &'a str {
        let borrowed: &Borrowed<'a> = peek.get::<Borrowed<'a>>().unwrap();
        borrowed.data
    }

    // With covariance, peek_static can be used here
    // The 'static lifetime shrinks to match 'a
    let result = use_peek(peek_static);
    assert_eq!(result, "I am truly static");
}

/// Shows that Peek with different 'facet lifetimes can be unified.
#[test]
fn peek_lifetime_unification() {
    static STATIC_DATA: &str = "static";
    let owned = String::from("owned");

    let static_borrowed: Borrowed<'static> = Borrowed { data: STATIC_DATA };
    let owned_borrowed: Borrowed<'_> = Borrowed { data: &owned };

    let peek_static: Peek<'_, 'static> = Peek::new(&static_borrowed);
    let peek_owned: Peek<'_, '_> = Peek::new(&owned_borrowed);

    // Both peeks can be passed to a function expecting the same lifetime
    // peek_static's 'static shrinks to match peek_owned's shorter lifetime
    fn takes_two<'a>(_p1: Peek<'_, 'a>, _p2: Peek<'_, 'a>) {}

    takes_two(peek_static, peek_owned);
}
