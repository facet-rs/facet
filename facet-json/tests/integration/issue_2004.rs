//! Test for issue #2004: #[facet(other)] variants are fallbacks for unknown tags
//!
//! The `#[facet(other)]` attribute marks a variant as a fallback for unknown tags.
//! This means:
//! 1. The variant is NOT in the normal variant lookup
//! 2. Any unknown tag (including the variant's own name) falls back to this variant
//! 3. The fallback path handles deserialization differently (e.g., captures the tag)

use facet::Facet;
use facet_json::{from_str, to_string};
use facet_testhelpers::test;

#[derive(Facet, Debug, PartialEq)]
#[facet(rename_all = "kebab-case")]
#[repr(u8)]
enum FilterValue {
    Null,
    Gt(Vec<String>),
    #[facet(other)]
    EqBare(Option<String>),
}

#[test]
fn test_known_variant_works() {
    // Known variants should work normally
    let input = r#"{"gt":["$value"]}"#;
    let result: FilterValue = from_str(input).unwrap();
    assert_eq!(result, FilterValue::Gt(vec!["$value".to_string()]));
}

#[test]
fn test_unknown_variant_falls_back_to_other() {
    // Unknown variant names should fall back to the #[facet(other)] variant
    let input = r#"{"custom":"$id"}"#;
    let result: FilterValue = from_str(input).unwrap();
    assert_eq!(result, FilterValue::EqBare(Some("$id".to_string())));
}

#[test]
fn test_other_variant_name_also_falls_back() {
    // Even the #[facet(other)] variant's own name is treated as unknown
    // (because it's excluded from the variant lookup)
    let input = r#"{"eq-bare":"$id"}"#;
    let result: FilterValue = from_str(input).unwrap();
    assert_eq!(result, FilterValue::EqBare(Some("$id".to_string())));
}

#[test]
fn test_round_trip_other_variant() {
    // Round-trip: serialize and deserialize #[facet(other)] variant
    let value = FilterValue::EqBare(Some("$id".to_string()));
    let json = to_string(&value).unwrap();
    // #[facet(other)] variants serialize untagged (just the payload)
    assert_eq!(json, r#""$id""#);
    // Deserialization of bare value falls back to #[facet(other)]
    let result: FilterValue = from_str(&json).unwrap();
    assert_eq!(result, value);
}

#[test]
fn test_round_trip_known_variant() {
    // Round-trip: known variants work normally
    let value = FilterValue::Gt(vec!["$value".to_string()]);
    let json = to_string(&value).unwrap();
    assert_eq!(json, r#"{"gt":["$value"]}"#);
    let result: FilterValue = from_str(&json).unwrap();
    assert_eq!(result, value);
}
