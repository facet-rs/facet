//! Test that truthy attribute errors point to the correct span.
//!
//! The error should point to `missing_truthy_fn`, not the macro expansion site.

use facet::Facet;

#[derive(Facet)]
#[facet(truthy = missing_truthy_fn)]
struct TestTruthy {
    value: bool,
}

fn main() {}
