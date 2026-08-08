//! Test that `list(shape_type)` extension-attribute payloads reject an empty
//! argument list with a friendly error, instead of e.g. panicking or producing
//! an opaque type error.
//!
//! The error should point out that at least one type is required.

use facet::Facet;

extern crate self as facet_test_project;

facet::define_attr_grammar! {
    ns "listtest";
    crate_path ::facet_test_project;

    /// Minimal grammar exposing a single `list(shape_type)` variant.
    pub enum Attr {
        /// List-of-shape_type payload used to test the empty-list error.
        Widths(list(shape_type)),
    }
}

#[derive(Facet)]
struct TestEmptyList {
    #[facet(facet_test_project::widths())]
    value: i32,
}

fn main() {}
