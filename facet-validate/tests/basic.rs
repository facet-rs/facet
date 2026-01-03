//! Basic tests for validation during deserialization.

use facet::Facet;
use facet_validate as validate;

fn validate_non_empty(s: &str) -> Result<(), String> {
    if s.is_empty() {
        Err("string must not be empty".to_string())
    } else {
        Ok(())
    }
}

fn validate_positive(n: &i64) -> Result<(), String> {
    if *n <= 0 {
        Err(format!("number must be positive, got {}", n))
    } else {
        Ok(())
    }
}

#[derive(Debug, Facet)]
struct Product {
    #[facet(validate::custom = validate_non_empty)]
    name: String,

    #[facet(validate::custom = validate_positive)]
    price: i64,
}

#[test]
fn test_valid_product() {
    let json = r#"{"name": "Widget", "price": 100}"#;
    let result: Result<Product, _> = facet_json::from_str(json);
    assert!(result.is_ok());
    let product = result.unwrap();
    assert_eq!(product.name, "Widget");
    assert_eq!(product.price, 100);
}

#[test]
fn test_invalid_empty_name() {
    let json = r#"{"name": "", "price": 100}"#;
    let result: Result<Product, _> = facet_json::from_str(json);
    assert!(result.is_err());
    let err = result.unwrap_err();
    let err_str = err.to_string();
    assert!(
        err_str.contains("string must not be empty"),
        "Expected error about empty string, got: {}",
        err_str
    );
}

#[test]
fn test_invalid_negative_price() {
    let json = r#"{"name": "Widget", "price": -5}"#;
    let result: Result<Product, _> = facet_json::from_str(json);
    assert!(result.is_err());
    let err = result.unwrap_err();
    let err_str = err.to_string();
    assert!(
        err_str.contains("must be positive"),
        "Expected error about positive number, got: {}",
        err_str
    );
}
