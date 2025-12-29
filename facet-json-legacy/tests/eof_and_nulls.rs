use facet::Facet;
use facet_json_legacy::{JsonErrorKind, from_slice, from_str};
use facet_testhelpers::test;
use std::fmt::Debug;

#[test]
fn test_eof_errors() {
    // Test empty input
    let result = from_str::<String>("");
    let err = result.unwrap_err();
    // Empty input produces an unexpected token error (EOF)
    assert!(
        matches!(err.kind, JsonErrorKind::UnexpectedToken { .. })
            || matches!(err.kind, JsonErrorKind::Scan(_))
    );

    // Test partial input for various types
    let result = from_str::<String>("\"hello");
    let err = result.unwrap_err();
    // Unterminated string should produce a scan error
    assert!(
        matches!(err.kind, JsonErrorKind::Scan(_))
            || matches!(err.kind, JsonErrorKind::ScanWithContext { .. })
    );

    let result = from_str::<Vec<i32>>("[1, 2,");
    let err = result.unwrap_err();
    // Unexpected EOF in list produces an unexpected token error
    assert!(
        matches!(err.kind, JsonErrorKind::UnexpectedToken { .. })
            || matches!(err.kind, JsonErrorKind::Scan(_))
            || matches!(err.kind, JsonErrorKind::ScanWithContext { .. })
    );

    let result = from_str::<Vec<i32>>("[");
    let err = result.unwrap_err();
    assert!(
        matches!(err.kind, JsonErrorKind::UnexpectedToken { .. })
            || matches!(err.kind, JsonErrorKind::Scan(_))
            || matches!(err.kind, JsonErrorKind::ScanWithContext { .. })
    );

    // Test object with EOF after opening {
    #[derive(Facet, Debug)]
    struct SimpleObject {
        key: String,
    }

    let result = from_str::<SimpleObject>("{");
    let err = result.unwrap_err();
    assert!(
        matches!(err.kind, JsonErrorKind::UnexpectedToken { .. })
            || matches!(err.kind, JsonErrorKind::Scan(_))
            || matches!(err.kind, JsonErrorKind::ScanWithContext { .. })
    );

    // Test object with EOF after key
    let result = from_str::<SimpleObject>("{\"key\"");
    let err = result.unwrap_err();
    assert!(
        matches!(err.kind, JsonErrorKind::UnexpectedToken { .. })
            || matches!(err.kind, JsonErrorKind::Scan(_))
            || matches!(err.kind, JsonErrorKind::ScanWithContext { .. })
    );

    // Test object with EOF after colon
    let result = from_str::<SimpleObject>("{\"key\":");
    let err = result.unwrap_err();
    assert!(
        matches!(err.kind, JsonErrorKind::UnexpectedToken { .. })
            || matches!(err.kind, JsonErrorKind::Scan(_))
            || matches!(err.kind, JsonErrorKind::ScanWithContext { .. })
    );

    // Test string with escape followed by EOF
    let result = from_str::<String>("\"hello\\");
    let err = result.unwrap_err();
    assert!(
        matches!(err.kind, JsonErrorKind::Scan(_))
            || matches!(err.kind, JsonErrorKind::ScanWithContext { .. })
    );
}

// Adjusted test for UTF-8 handling based on actual behavior
#[test]
fn test_invalid_utf8_handling() {
    // Create invalid UTF-8 bytes - this should be truly invalid
    let invalid_bytes = &[b'"', 0xFF, 0xC0, 0x80, b'"'][..]; // Invalid UTF-8 sequence
    let result = from_slice::<String>(invalid_bytes);

    // Simply assert there's an error (the exact type isn't important)
    assert!(result.is_err());
}

#[test]
fn test_null_handling() {
    // Test with invalid null value - "nul" starts with 'n' but isn't "null"
    let result = from_str::<Option<i32>>("nul");
    let err = result.unwrap_err();
    // This should be a scan error since "nul" isn't a valid token
    assert!(
        matches!(err.kind, JsonErrorKind::Scan(_))
            || matches!(err.kind, JsonErrorKind::ScanWithContext { .. })
    );

    // Test with correct null handling
    #[derive(Facet, Debug)]
    struct OptionalStruct {
        val: Option<i32>,
    }

    let json = r#"{"val": null}"#;
    let ok = from_str::<OptionalStruct>(json).unwrap();
    assert_eq!(ok.val, None);
}
