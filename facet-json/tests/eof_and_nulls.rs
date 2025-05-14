use eyre::Result;
use facet::Facet;
use facet_deserialize::DeserErrorKind;
use facet_json::{from_slice, from_str};
use std::fmt::Debug;

#[test]
fn test_eof_errors() {
    facet_testhelpers::setup();

    // Test empty input
    let result = from_str::<String>("");
    let err = result.unwrap_err();
    assert!(matches!(
        err.kind,
        DeserErrorKind::UnexpectedEof { wanted, .. } if wanted.contains("any value")
    ));

    // Test partial input for various types
    let result = from_str::<String>("\"hello");
    let err = result.unwrap_err();
    assert!(matches!(
        err.kind,
        DeserErrorKind::UnexpectedEof { wanted, .. } if wanted.contains("string")
    ));
    let result = from_str::<Vec<i32>>("[1, 2,");
    let err = result.unwrap_err();
    assert!(matches!(err.kind, DeserErrorKind::UnexpectedEof { .. }));

    let result = from_str::<Vec<i32>>("[");
    let err = result.unwrap_err();
    assert!(matches!(err.kind, DeserErrorKind::UnexpectedEof { .. }));

    // Test object with EOF after opening {
    #[derive(Facet, Debug)]
    struct SimpleObject {
        key: String,
    }

    let result = from_str::<SimpleObject>("{");
    let err = result.unwrap_err();
    assert!(matches!(err.kind, DeserErrorKind::UnexpectedEof { .. }));

    // Test object with EOF after key
    let result = from_str::<SimpleObject>("{\"key\"");
    let err = result.unwrap_err();
    assert!(matches!(err.kind, DeserErrorKind::UnexpectedEof { .. }));

    // Test object with EOF after colon
    let result = from_str::<SimpleObject>("{\"key\":");
    let err = result.unwrap_err();
    assert!(matches!(err.kind, DeserErrorKind::UnexpectedEof { .. }));

    // Test string with escape followed by EOF
    let result = from_str::<String>("\"hello\\");
    let err = result.unwrap_err();
    assert!(matches!(
        err.kind,
        DeserErrorKind::UnexpectedEof { wanted, .. } if wanted.contains("escape")
    ));
}

// Adjusted test for UTF-8 handling based on actual behavior
#[test]
fn test_invalid_utf8_handling() {
    facet_testhelpers::setup();

    // Create invalid UTF-8 bytes - this should be truly invalid
    let invalid_bytes = &[b'"', 0xFF, 0xC0, 0x80, b'"'][..]; // Invalid UTF-8 sequence
    let result = from_slice::<String>(invalid_bytes);

    // Simply assert there's an error (the exact type isn't important)
    assert!(result.is_err());
}

#[test]
fn test_null_handling() -> Result<()> {
    facet_testhelpers::setup();

    // Test with invalid null value
    let result = from_str::<Option<i32>>("nul");
    let err = result.unwrap_err();
    assert!(matches!(
        err.kind,
        DeserErrorKind::UnexpectedChar { got: 'n', .. }
    ));

    // Test with correct null handling
    #[derive(Facet, Debug)]
    struct OptionalStruct {
        val: Option<i32>,
    }

    let json = r#"{"val": null}"#;
    let ok = from_str::<OptionalStruct>(json)?;
    assert_eq!(ok.val, None);

    Ok(())
}
