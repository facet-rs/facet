//! Test that an explicit inline `Facet` bound on a type parameter is reported clearly.

use facet::Facet;

#[derive(Facet)]
enum Wrapper<X: Facet<'static>> {
    Items(Vec<X>),
}

fn main() {}
