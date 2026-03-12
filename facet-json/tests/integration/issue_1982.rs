//! Regression test for <https://github.com/facet-rs/facet/issues/1982>
//!
//! `#[facet(flatten)]` on an externally-tagged enum field must not duplicate
//! the variant key during serialization.

use facet::Facet;
use facet_json::{from_str as from_json, to_string as to_json};
use facet_testhelpers::test;

#[derive(Facet, Debug, PartialEq)]
struct Outer {
    #[facet(flatten)]
    flatten: Inner,
}

#[derive(Facet, Debug, PartialEq)]
#[repr(u8)]
enum Inner {
    Variant { field: u8 },
}

#[test]
fn flattened_externally_tagged_enum_serialization_has_single_variant_key() {
    let value = Outer {
        flatten: Inner::Variant { field: 1 },
    };

    let json = to_json(&value).expect("serialization should succeed");
    assert_eq!(json, r#"{"Variant":{"field":1}}"#);

    let roundtrip: Outer = from_json(&json).expect("deserialization should succeed");
    assert_eq!(roundtrip, value);
}
