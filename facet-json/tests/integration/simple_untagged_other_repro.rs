//! Minimal reproduction: simple enum with an untagged string catch-all.
//!
//! This is intentionally ignored so it can live in the Facet checkout as a
//! copyable GitHub issue repro without breaking the normal test suite.

use facet::Facet;
use facet_json::from_str;

#[derive(Debug, PartialEq, Eq, Facet)]
#[repr(C)]
enum SimpleKind {
    First,
    Second,
    #[facet(other)]
    Other(String),
}

#[test]
fn known_unit_variant_deserializes_from_string() {
    let parsed: SimpleKind = from_str(r#""First""#).unwrap();

    assert_eq!(parsed, SimpleKind::First);
}

#[test]
fn unknown_string_variant_falls_back_to_other() {
    let parsed: SimpleKind = from_str(r#""Third""#).unwrap();

    assert_eq!(parsed, SimpleKind::Other("Third".to_owned()));
}
