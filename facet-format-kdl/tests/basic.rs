//! Basic tests for KDL parsing.

use facet::Facet;
use facet_format_kdl as kdl;
use facet_format_kdl::from_str;

#[derive(Facet, Debug, PartialEq)]
struct SimpleValue {
    #[facet(kdl::argument)]
    value: String,
}

#[test]
fn test_single_argument() {
    let kdl_input = r#"node "hello""#;
    let result: SimpleValue = from_str(kdl_input).unwrap();
    assert_eq!(result.value, "hello");
}

#[derive(Facet, Debug, PartialEq)]
struct Server {
    #[facet(kdl::argument)]
    host: String,
    #[facet(kdl::property)]
    port: u16,
}

#[test]
fn test_argument_and_property() {
    let kdl_input = r#"server "localhost" port=8080"#;
    let server: Server = from_str(kdl_input).unwrap();
    assert_eq!(server.host, "localhost");
    assert_eq!(server.port, 8080);
}

#[derive(Facet, Debug, PartialEq)]
struct Numbers {
    #[facet(kdl::property)]
    a: i32,
    #[facet(kdl::property)]
    b: f64,
    #[facet(kdl::property)]
    c: bool,
}

#[test]
fn test_multiple_properties() {
    let kdl_input = r#"numbers a=-42 b=3.125 c=#true"#;
    let nums: Numbers = from_str(kdl_input).unwrap();
    assert_eq!(nums.a, -42);
    assert!((nums.b - 3.125).abs() < 0.001);
    assert!(nums.c);
}

#[test]
fn test_false_bool() {
    let kdl_input = r#"numbers a=0 b=0.0 c=#false"#;
    let nums: Numbers = from_str(kdl_input).unwrap();
    assert_eq!(nums.a, 0);
    assert!((nums.b - 0.0).abs() < 0.001);
    assert!(!nums.c);
}

// Test child nodes
#[derive(Facet, Debug, PartialEq)]
struct Address {
    #[facet(kdl::property)]
    street: String,
    #[facet(kdl::property)]
    city: String,
}

#[derive(Facet, Debug, PartialEq)]
struct Person {
    #[facet(kdl::argument)]
    name: String,
    #[facet(kdl::child)]
    address: Address,
}

#[test]
fn test_child_node() {
    let kdl_input = r#"
        person "Alice" {
            address street="123 Main St" city="Springfield"
        }
    "#;
    let person: Person = from_str(kdl_input).unwrap();
    assert_eq!(person.name, "Alice");
    assert_eq!(person.address.street, "123 Main St");
    assert_eq!(person.address.city, "Springfield");
}

// Test null values
#[derive(Facet, Debug, PartialEq)]
struct MaybeValue {
    #[facet(kdl::property)]
    value: Option<String>,
}

#[test]
fn test_null_value() {
    let kdl_input = r#"config value=#null"#;
    let config: MaybeValue = from_str(kdl_input).unwrap();
    assert_eq!(config.value, None);
}

#[test]
fn test_some_value() {
    let kdl_input = r#"config value="hello""#;
    let config: MaybeValue = from_str(kdl_input).unwrap();
    assert_eq!(config.value, Some("hello".to_string()));
}
