//! Test that `list(shape_type)` extension-attribute payloads reject a
//! malformed list (a doubled top-level comma) with a friendly error, instead
//! of silently accepting the empty item between the commas.
//!
//! The error should point out that a type was expected between commas.

use facet::Facet;

extern crate self as facet_test_project;

facet::define_attr_grammar! {
    ns "listtest";
    crate_path ::facet_test_project;

    /// Minimal grammar exposing a single `list(shape_type)` variant.
    pub enum Attr {
        /// List-of-shape_type payload used to test the malformed-list error.
        Widths(list(shape_type)),
    }
}

#[derive(Facet)]
struct TestMalformedList {
    #[facet(facet_test_project::widths(f32,, i32))]
    value: i32,
}

fn main() {}
