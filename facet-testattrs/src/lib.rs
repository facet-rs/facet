//! Test attributes for facet extension attribute testing.
//!
//! This crate exists solely to test that extension attributes work correctly
//! in the facet derive macro. It is not published.

#![deny(unsafe_code)]

extern crate self as facet_testattrs;

// Generate test attribute grammar using the grammar DSL.
facet::define_attr_grammar! {
    ns "testattrs";
    crate_path ::facet_testattrs;

    /// Test attribute types for testing extension attribute handling.
    pub enum Attr {
        /// A simple marker attribute for testing.
        Positional,
        /// Another marker attribute for testing.
        Named,
        /// An attribute with an optional value.
        Short(Option<char>),
        /// Optional string payload used for generic extension-attr tests.
        GenericName(Option<&'static str>),
    }
}
