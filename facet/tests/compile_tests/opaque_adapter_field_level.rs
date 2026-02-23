//! Test that `#[facet(opaque = ...)]` on fields is rejected in MVP.

use facet::Facet;

struct Adapter;

#[derive(Facet)]
struct BadFieldUsage {
    #[facet(opaque = Adapter)]
    payload: u32,
}

fn main() {}
