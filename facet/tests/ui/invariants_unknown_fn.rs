//! Test that invariants attribute errors point to the correct span.
//!
//! The error should point to `missing_validator`, not the macro expansion site.

use facet::Facet;

#[derive(Facet)]
struct TestInvariants {
    #[facet(invariants = missing_validator)]
    value: i32,
}

fn main() {}
