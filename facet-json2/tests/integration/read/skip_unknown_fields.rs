use eyre::Result;
use facet::Facet;
use facet_json2::from_str;
use std::fmt::Debug;

#[test]
fn test_skip_over_value() -> Result<()> {
    facet_testhelpers::setup();

    #[derive(Facet, Debug)]
    struct StructWithUnknownField {
        known: String,
    }

    // Test skipping over different value types

    // Skip over object
    let json = r#"{"unknown": {"a": 1, "b": "test"}, "known": "value"}"#;
    let result = from_str::<StructWithUnknownField>(json)?;
    assert_eq!(result.known, "value");

    // Skip over array
    let json = r#"{"unknown": [1, 2, 3, "test"], "known": "value"}"#;
    let result = from_str::<StructWithUnknownField>(json)?;
    assert_eq!(result.known, "value");

    // Skip over string
    let json = r#"{"unknown": "test value", "known": "value"}"#;
    let result = from_str::<StructWithUnknownField>(json)?;
    assert_eq!(result.known, "value");

    // Skip over number
    let json = r#"{"unknown": 12345, "known": "value"}"#;
    let result = from_str::<StructWithUnknownField>(json)?;
    assert_eq!(result.known, "value");

    // Skip over boolean
    let json = r#"{"unknown": true, "known": "value"}"#;
    let result = from_str::<StructWithUnknownField>(json)?;
    assert_eq!(result.known, "value");

    // Skip over null
    let json = r#"{"unknown": null, "known": "value"}"#;
    let result = from_str::<StructWithUnknownField>(json)?;
    assert_eq!(result.known, "value");

    // Skip over nested complex structure
    let json = r#"{"unknown": {"a": [1, 2, {"b": [3, 4, {"c": 5}]}]}, "known": "value"}"#;
    let result = from_str::<StructWithUnknownField>(json)?;
    assert_eq!(result.known, "value");

    Ok(())
}

#[test]
fn test_skip_over_value_with_escape_sequences() -> Result<()> {
    facet_testhelpers::setup();

    #[derive(Facet, Debug)]
    struct StructWithUnknownField {
        known: String,
    }

    // Test skipping over string with escape sequences
    let json = r#"{"unknown": "test with \"escape\" sequences", "known": "value"}"#;
    let result = from_str::<StructWithUnknownField>(json)?;
    assert_eq!(result.known, "value");

    // Test with other escape sequences
    let json = r#"{"unknown": "test with \n\r\t\\ escapes", "known": "value"}"#;
    let result = from_str::<StructWithUnknownField>(json)?;
    assert_eq!(result.known, "value");

    Ok(())
}

#[test]
fn test_skip_over_complex_numbers() -> Result<()> {
    facet_testhelpers::setup();

    #[derive(Facet, Debug)]
    struct StructWithUnknownField {
        known: String,
    }

    // Test skipping over different number formats

    // Skip over decimal number
    let json = r#"{"unknown": 123.456, "known": "value"}"#;
    let result = from_str::<StructWithUnknownField>(json)?;
    assert_eq!(result.known, "value");

    // Skip over number with exponent
    let json = r#"{"unknown": 1.23e4, "known": "value"}"#;
    let result = from_str::<StructWithUnknownField>(json)?;
    assert_eq!(result.known, "value");

    // Skip over negative exponent
    let json = r#"{"unknown": 1.23e-4, "known": "value"}"#;
    let result = from_str::<StructWithUnknownField>(json)?;
    assert_eq!(result.known, "value");

    Ok(())
}
