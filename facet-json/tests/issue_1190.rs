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
    let result: BreakDown = facet_json::from_str(json).unwrap();
    assert_eq!(result.sort, Sort::AB);

    // Serialization
    let breakdown = BreakDown {
        scale: 1.0,
        sort: Sort::AB,
    };
    let serialized = facet_json::to_string(&breakdown);
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

    // This is what the user tried - it fails because untagged unit variants
    // serialize to null in serde, not strings
    let json = r#"{"scale": 1.0, "sort": "AB"}"#;
    let result: Result<BreakDown, _> = facet_json::from_str(json);
    assert!(result.is_err()); // Expected to fail

    // With null it now works (like serde)
    let json_null = r#"{"scale": 1.0, "sort": null}"#;
    let result_null: BreakDown = facet_json::from_str(json_null).unwrap();
    assert_eq!(result_null.sort, Sort::AA); // First unit variant gets selected
}

#[test]
fn test_untagged_unit_variant_deserialize_null() {
    // Test that we can deserialize null into untagged unit variant (like serde)
    #[derive(Debug, Facet, PartialEq)]
    #[repr(u8)]
    #[facet(untagged)]
    enum MaybeNull {
        Null,
        Value(i32),
    }

    // Deserialize from null should give the first unit variant
    let result: MaybeNull = facet_json::from_str("null").unwrap();
    assert_eq!(result, MaybeNull::Null);

    // Deserialize from number should give Value variant
    let result_val: MaybeNull = facet_json::from_str("42").unwrap();
    assert_eq!(result_val, MaybeNull::Value(42));

    // Roundtrip
    let null_val = MaybeNull::Null;
    let json = facet_json::to_string(&null_val);
    assert_eq!(json, "null");
    let roundtrip: MaybeNull = facet_json::from_str(&json).unwrap();
    assert_eq!(roundtrip, MaybeNull::Null);
}
