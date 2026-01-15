//! Test that skip_serializing_if attribute errors point to the correct span.
//!
//! The error should point to `nonexistent_predicate`, not the macro expansion site.

use facet::Facet;

#[derive(Facet)]
struct TestSkipIf {
    #[facet(skip_serializing_if = nonexistent_predicate)]
    value: i32,
}

fn main() {}
