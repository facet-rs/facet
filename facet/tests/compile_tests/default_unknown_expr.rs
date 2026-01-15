//! Test that default attribute errors point to the correct span.
//!
//! The error should point to `MissingDefault::create()`, not the macro expansion site.

use facet::Facet;

#[derive(Facet)]
struct TestDefault {
    #[facet(default = MissingDefault::create())]
    value: i32,
}

fn main() {}
