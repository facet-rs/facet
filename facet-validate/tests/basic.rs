//! Basic tests for validation during deserialization.

use facet::Facet;
use facet_validate as validate;

// ============================================================================
// Custom validators
// ============================================================================

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
struct ProductCustom {
    #[facet(validate::custom = validate_non_empty)]
    name: String,

    #[facet(validate::custom = validate_positive)]
    price: i64,
}

#[test]
fn test_custom_valid_product() {
    let json = r#"{"name": "Widget", "price": 100}"#;
    let result: Result<ProductCustom, _> = facet_json::from_str(json);
    assert!(result.is_ok());
    let product = result.unwrap();
    assert_eq!(product.name, "Widget");
    assert_eq!(product.price, 100);
}

#[test]
fn test_custom_invalid_empty_name() {
    let json = r#"{"name": "", "price": 100}"#;
    let result: Result<ProductCustom, _> = facet_json::from_str(json);
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
fn test_custom_invalid_negative_price() {
    let json = r#"{"name": "Widget", "price": -5}"#;
    let result: Result<ProductCustom, _> = facet_json::from_str(json);
    assert!(result.is_err());
    let err = result.unwrap_err();
    let err_str = err.to_string();
    assert!(
        err_str.contains("must be positive"),
        "Expected error about positive number, got: {}",
        err_str
    );
}

// ============================================================================
// Min/Max validators
// ============================================================================

#[derive(Debug, Facet)]
struct Bounded {
    #[facet(validate::min = 0, validate::max = 100)]
    value: i64,
}

#[test]
fn test_min_max_valid() {
    let json = r#"{"value": 50}"#;
    let result: Result<Bounded, _> = facet_json::from_str(json);
    assert!(result.is_ok());
    assert_eq!(result.unwrap().value, 50);
}

#[test]
fn test_min_max_at_boundaries() {
    let json = r#"{"value": 0}"#;
    let result: Result<Bounded, _> = facet_json::from_str(json);
    assert!(result.is_ok());

    let json = r#"{"value": 100}"#;
    let result: Result<Bounded, _> = facet_json::from_str(json);
    assert!(result.is_ok());
}

#[test]
fn test_min_violation() {
    let json = r#"{"value": -1}"#;
    let result: Result<Bounded, _> = facet_json::from_str(json);
    assert!(result.is_err());
    let err_str = result.unwrap_err().to_string();
    assert!(
        err_str.contains("must be >= 0"),
        "Expected min error, got: {}",
        err_str
    );
}

#[test]
fn test_max_violation() {
    let json = r#"{"value": 101}"#;
    let result: Result<Bounded, _> = facet_json::from_str(json);
    assert!(result.is_err());
    let err_str = result.unwrap_err().to_string();
    assert!(
        err_str.contains("must be <= 100"),
        "Expected max error, got: {}",
        err_str
    );
}

// ============================================================================
// Length validators
// ============================================================================

#[derive(Debug, Facet)]
struct LengthBounded {
    #[facet(validate::min_length = 3, validate::max_length = 10)]
    name: String,
}

#[test]
fn test_length_valid() {
    let json = r#"{"name": "hello"}"#;
    let result: Result<LengthBounded, _> = facet_json::from_str(json);
    assert!(result.is_ok());
}

#[test]
fn test_length_at_boundaries() {
    let json = r#"{"name": "abc"}"#; // exactly 3
    let result: Result<LengthBounded, _> = facet_json::from_str(json);
    assert!(result.is_ok());

    let json = r#"{"name": "abcdefghij"}"#; // exactly 10
    let result: Result<LengthBounded, _> = facet_json::from_str(json);
    assert!(result.is_ok());
}

#[test]
fn test_min_length_violation() {
    let json = r#"{"name": "ab"}"#; // only 2
    let result: Result<LengthBounded, _> = facet_json::from_str(json);
    assert!(result.is_err());
    let err_str = result.unwrap_err().to_string();
    assert!(
        err_str.contains("length must be >= 3"),
        "Expected min_length error, got: {}",
        err_str
    );
}

#[test]
fn test_max_length_violation() {
    let json = r#"{"name": "abcdefghijk"}"#; // 11 chars
    let result: Result<LengthBounded, _> = facet_json::from_str(json);
    assert!(result.is_err());
    let err_str = result.unwrap_err().to_string();
    assert!(
        err_str.contains("length must be <= 10"),
        "Expected max_length error, got: {}",
        err_str
    );
}

// ============================================================================
// Email validator
// ============================================================================

#[derive(Debug, Facet)]
struct Contact {
    #[facet(validate::email)]
    email: String,
}

#[test]
fn test_email_valid() {
    let json = r#"{"email": "user@example.com"}"#;
    let result: Result<Contact, _> = facet_json::from_str(json);
    assert!(result.is_ok());
}

#[test]
fn test_email_invalid() {
    let json = r#"{"email": "not-an-email"}"#;
    let result: Result<Contact, _> = facet_json::from_str(json);
    assert!(result.is_err());
    let err_str = result.unwrap_err().to_string();
    assert!(
        err_str.contains("not a valid email"),
        "Expected email error, got: {}",
        err_str
    );
}

// ============================================================================
// URL validator
// ============================================================================

#[derive(Debug, Facet)]
struct Website {
    #[facet(validate::url)]
    url: String,
}

#[test]
fn test_url_valid() {
    let json = r#"{"url": "https://example.com/path"}"#;
    let result: Result<Website, _> = facet_json::from_str(json);
    assert!(result.is_ok());
}

#[test]
fn test_url_invalid() {
    let json = r#"{"url": "not-a-url"}"#;
    let result: Result<Website, _> = facet_json::from_str(json);
    assert!(result.is_err());
    let err_str = result.unwrap_err().to_string();
    assert!(
        err_str.contains("not a valid URL"),
        "Expected URL error, got: {}",
        err_str
    );
}

// ============================================================================
// Regex validator
// ============================================================================

#[derive(Debug, Facet)]
struct CountryCode {
    #[facet(validate::regex = r"^[A-Z]{2}$")]
    code: String,
}

#[test]
fn test_regex_valid() {
    let json = r#"{"code": "US"}"#;
    let result: Result<CountryCode, _> = facet_json::from_str(json);
    assert!(result.is_ok());
}

#[test]
fn test_regex_invalid() {
    let json = r#"{"code": "USA"}"#; // 3 chars, not 2
    let result: Result<CountryCode, _> = facet_json::from_str(json);
    assert!(result.is_err());
    let err_str = result.unwrap_err().to_string();
    assert!(
        err_str.contains("does not match pattern"),
        "Expected regex error, got: {}",
        err_str
    );
}

// ============================================================================
// Contains validator
// ============================================================================

#[derive(Debug, Facet)]
struct Message {
    #[facet(validate::contains = "hello")]
    text: String,
}

#[test]
fn test_contains_valid() {
    let json = r#"{"text": "say hello world"}"#;
    let result: Result<Message, _> = facet_json::from_str(json);
    assert!(result.is_ok());
}

#[test]
fn test_contains_invalid() {
    let json = r#"{"text": "goodbye world"}"#;
    let result: Result<Message, _> = facet_json::from_str(json);
    assert!(result.is_err());
    let err_str = result.unwrap_err().to_string();
    assert!(
        err_str.contains("does not contain 'hello'"),
        "Expected contains error, got: {}",
        err_str
    );
}

// ============================================================================
// Combined validators
// ============================================================================

#[derive(Debug, Facet)]
struct User {
    #[facet(validate::min_length = 1, validate::max_length = 50)]
    name: String,

    #[facet(validate::email)]
    email: String,

    #[facet(validate::min = 0, validate::max = 150)]
    age: i32,
}

#[test]
fn test_combined_valid() {
    let json = r#"{"name": "Alice", "email": "alice@example.com", "age": 30}"#;
    let result: Result<User, _> = facet_json::from_str(json);
    assert!(result.is_ok());
}

#[test]
fn test_combined_first_field_fails() {
    let json = r#"{"name": "", "email": "alice@example.com", "age": 30}"#;
    let result: Result<User, _> = facet_json::from_str(json);
    assert!(result.is_err());
    assert!(
        result
            .unwrap_err()
            .to_string()
            .contains("length must be >= 1")
    );
}
