use facet::Facet;
use facet_testhelpers::test;

// Test issue #1190: Unit enum variants in struct field

#[test]
fn test_issue_1190_externally_tagged_works() {
    // Externally tagged (default) - should work
    #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Facet)]
    #[repr(u8)]
    pub enum Sort {
        AA,
        AB,
        BC,
    }

    #[derive(Clone, PartialEq, Debug, Facet)]
    pub struct BreakDown {
        pub scale: f32,
        pub sort: Sort,
    }

    // Externally tagged unit variants work as strings
    let json = r#"{"scale": 1.0, "sort": "AB"}"#;
    let result: BreakDown = facet_json_legacy::from_str(json).unwrap();
    assert_eq!(result.sort, Sort::AB);

    // Serialization
    let breakdown = BreakDown {
        scale: 1.0,
        sort: Sort::AB,
    };
    let serialized = facet_json_legacy::to_string(&breakdown);
    assert_eq!(serialized, r#"{"scale":1.0,"sort":"AB"}"#);
}

#[test]
fn test_issue_1190_untagged_fails_with_string() {
    // Untagged - serde also doesn't support strings for unit variants
    #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Facet)]
    #[facet(untagged)]
    #[repr(u8)]
    pub enum Sort {
        AA,
        AB,
        BC,
    }

    #[derive(Clone, PartialEq, Debug, Facet)]
    pub struct BreakDown {
        pub scale: f32,
        pub sort: Sort,
    }

    // Facet diverges from serde here: untagged unit variants serialize/deserialize
    // as variant name strings (not null) to allow distinguishing between variants
    let json = r#"{"scale": 1.0, "sort": "AB"}"#;
    let result: BreakDown = facet_json_legacy::from_str(json).unwrap();
    assert_eq!(result.sort, Sort::AB);

    // Roundtrip test
    let breakdown = BreakDown {
        scale: 1.0,
        sort: Sort::AB,
    };
    let serialized = facet_json_legacy::to_string(&breakdown);
    assert_eq!(serialized, r#"{"scale":1.0,"sort":"AB"}"#);
    let deserialized: BreakDown = facet_json_legacy::from_str(&serialized).unwrap();
    assert_eq!(deserialized.sort, Sort::AB);
}

#[test]
fn test_untagged_unit_variant_deserialize_null() {
    // Facet diverges from serde: untagged unit variants deserialize from variant name strings
    #[derive(Debug, Facet, PartialEq)]
    #[repr(u8)]
    #[facet(untagged)]
    enum MaybeValue {
        Empty,
        Value(i32),
    }

    // Deserialize from variant name string
    let result: MaybeValue = facet_json_legacy::from_str(r#""Empty""#).unwrap();
    assert_eq!(result, MaybeValue::Empty);

    // Deserialize from number should give Value variant
    let result_val: MaybeValue = facet_json_legacy::from_str("42").unwrap();
    assert_eq!(result_val, MaybeValue::Value(42));

    // Roundtrip
    let empty_val = MaybeValue::Empty;
    let json = facet_json_legacy::to_string(&empty_val);
    assert_eq!(json, r#""Empty""#);
    let roundtrip: MaybeValue = facet_json_legacy::from_str(&json).unwrap();
    assert_eq!(roundtrip, MaybeValue::Empty);
}
