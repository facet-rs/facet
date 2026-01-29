//! Opaque fields require 'static for now.
//!
//! This should fail because `#[facet(opaque)]` on a borrowed reference
//! produces `Opaque<&'a T>`, and Opaque<T> requires T: 'static.

use facet::Facet;

struct NotFacet;

#[derive(Facet)]
struct Wrap<'a> {
    #[facet(opaque)]
    inner: &'a NotFacet,
}

fn main() {
    let value = NotFacet;
    let _wrap = Wrap { inner: &value };
}
