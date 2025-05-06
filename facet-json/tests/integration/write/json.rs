#![cfg(feature = "std")]

use eyre::Result;
use facet::Facet;
use facet_json::{to_string, to_writer};

#[test]
fn test_to_json() -> Result<()> {
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
    let json = to_string(&test_struct);
    assert_eq!(json, expected_json);

    // Test with indentation (using to_writer directly with a custom writer)
    let mut buffer = Vec::new();
    to_writer(&test_struct, &mut buffer)?;
    let json = String::from_utf8(buffer)?;
    assert_eq!(json, expected_json);

    Ok(())
}
