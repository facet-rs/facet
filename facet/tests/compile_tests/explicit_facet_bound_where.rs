//! Test that an explicit `Facet` bound in a `where` clause is reported clearly.
//!
//! The derive adds `T: Facet` itself, so the user-written bound should be
//! rejected at its span instead of failing with an opaque E0283.

use facet::Facet;

#[derive(Facet)]
struct Wrapper<X>
where
    X: Facet<'static>,
{
    items: Vec<X>,
}

fn main() {}
