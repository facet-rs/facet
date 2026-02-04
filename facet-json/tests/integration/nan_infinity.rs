//! Test for NaN and Infinity float serialization.
//!
//! JSON does not support NaN or Infinity values. Per serde's behavior,
//! these should serialize as `null`.

use facet::Facet;
use facet_testhelpers::test;

#[derive(Debug, Facet)]
struct Container {
    value: f64,
}

#[derive(Debug, Facet)]
struct ContainerF32 {
    value: f32,
}

#[test]
fn test_nan_serializes_as_null() {
    let container = Container { value: f64::NAN };
    let json = facet_json::to_string(&container).unwrap();

    assert_eq!(json, r#"{"value":null}"#, "NaN should serialize as null");
}

#[test]
fn test_positive_infinity_serializes_as_null() {
    let container = Container {
        value: f64::INFINITY,
    };
    let json = facet_json::to_string(&container).unwrap();

    assert_eq!(
        json, r#"{"value":null}"#,
        "Positive infinity should serialize as null"
    );
}

#[test]
fn test_f32_nan_serializes_as_null() {
    let container = ContainerF32 { value: f32::NAN };
    let json = facet_json::to_string(&container).unwrap();
    assert_eq!(json, r#"{"value":null}"#);
}
