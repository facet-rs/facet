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
        /// Newtype static string payload used to test direct payload storage.
        EnvPrefix(&'static str),
        /// Newtype i64 payload used to test direct payload storage.
        Min(i64),
        /// Newtype usize payload used to test direct payload storage.
        MaxLen(usize),
        /// An attribute with an optional value.
        Short(Option<char>),
        /// Structured payload used to test enum-wrapped runtime decoding.
        Column(Column),
        /// Optional string payload used for generic extension-attr tests.
        GenericName(Option<&'static str>),
        /// Arbitrary payload used for generic extension-attr tests.
        GenericSize(Option<usize>),
    }

    /// Structured configuration payload used by `Attr::Column`.
    pub struct Column {
        /// Optional renamed field name used by format adapters.
        pub rename: Option<&'static str>,
        /// Whether the field should be treated as indexed.
        pub indexed: bool,
    }
}
