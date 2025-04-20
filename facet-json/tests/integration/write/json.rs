#![cfg(feature = "std")]

use facet::Facet;

#[test]
fn test_to_json() {
    facet_testhelpers::setup();

    #[derive(Debug, PartialEq, Clone, Facet)]
    struct LinearFunction {
        variable: String,
        slope: f32,
        intercept: i32,
    }

    let test_struct = LinearFunction {
        variable: "x".to_string(),
        slope: -3.5,
        intercept: -5,
    };

    let expected_json = r#"{"variable":"x","slope":-3.5,"intercept":-5}"#;

    // Test without indentation (using to_string)
    let json = facet_json::to_string(&test_struct);
    assert_eq!(json, expected_json);

    // Test with indentation (using to_writer directly with a custom writer)
    let mut buffer = Vec::new();
    facet_json::to_writer(&test_struct, &mut buffer).unwrap();
    let json = String::from_utf8(buffer).unwrap();
    assert_eq!(json, expected_json);
}
