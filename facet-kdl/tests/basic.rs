//! Basic tests for KDL parsing.

use facet::Facet;
use facet_kdl as kdl;
use facet_kdl::from_str;

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

// ============================================================================
// Integer type tests
// ============================================================================

#[derive(Facet, Debug, PartialEq)]
struct IntegerTypes {
    #[facet(kdl::property)]
    u8_val: u8,
    #[facet(kdl::property)]
    i16_val: i16,
    #[facet(kdl::property)]
    u32_val: u32,
    #[facet(kdl::property)]
    i64_val: i64,
}

#[test]
fn test_integer_types() {
    let kdl_input = r#"ints u8_val=255 i16_val=-1000 u32_val=1000000 i64_val=-9223372036854775808"#;
    let ints: IntegerTypes = from_str(kdl_input).unwrap();
    assert_eq!(ints.u8_val, 255);
    assert_eq!(ints.i16_val, -1000);
    assert_eq!(ints.u32_val, 1000000);
    assert_eq!(ints.i64_val, i64::MIN);
}

// ============================================================================
// Nested struct tests
// ============================================================================

#[derive(Facet, Debug, PartialEq)]
struct InnerConfig {
    #[facet(kdl::property)]
    enabled: bool,
    #[facet(kdl::property)]
    level: u8,
}

#[derive(Facet, Debug, PartialEq)]
struct MiddleConfig {
    #[facet(kdl::argument)]
    name: String,
    #[facet(kdl::child)]
    inner: InnerConfig,
}

#[derive(Facet, Debug, PartialEq)]
struct OuterConfig {
    #[facet(kdl::child)]
    middle: MiddleConfig,
}

#[test]
fn test_deeply_nested_structs() {
    let kdl_input = r#"
        root {
            middle "test" {
                inner enabled=#true level=5
            }
        }
    "#;
    let config: OuterConfig = from_str(kdl_input).unwrap();
    assert_eq!(config.middle.name, "test");
    assert!(config.middle.inner.enabled);
    assert_eq!(config.middle.inner.level, 5);
}

// ============================================================================
// Optional child with default tests
// ============================================================================

#[derive(Facet, Debug, PartialEq)]
struct OptionalChildComplex {
    #[facet(kdl::property)]
    value: String,
}

#[derive(Facet, Debug, PartialEq)]
struct ParentWithOptionalChild {
    #[facet(kdl::argument)]
    id: String,
    #[facet(kdl::child)]
    #[facet(default)]
    child: Option<OptionalChildComplex>,
}

#[test]
fn test_optional_child_present() {
    // Child node with properties works
    let kdl_input = r#"
        parent "main" {
            child value="nested"
        }
    "#;
    let parent: ParentWithOptionalChild = from_str(kdl_input).unwrap();
    assert_eq!(parent.id, "main");
    assert_eq!(
        parent.child,
        Some(OptionalChildComplex {
            value: "nested".to_string()
        })
    );
}

#[test]
fn test_optional_child_absent() {
    let kdl_input = r#"parent "main""#;
    let parent: ParentWithOptionalChild = from_str(kdl_input).unwrap();
    assert_eq!(parent.id, "main");
    assert_eq!(parent.child, None);
}

// ============================================================================
// Skip field tests
// ============================================================================

#[derive(Facet, Debug, PartialEq)]
struct ConfigWithSkip {
    #[facet(kdl::property)]
    name: String,
    #[facet(skip)]
    internal_id: u64,
}

#[test]
fn test_skip_field() {
    let kdl_input = r#"config name="test""#;
    let config: ConfigWithSkip = from_str(kdl_input).unwrap();
    assert_eq!(config.name, "test");
    assert_eq!(config.internal_id, 0); // Default value
}

// ============================================================================
// Rename tests
// ============================================================================

#[derive(Facet, Debug, PartialEq)]
struct RenamedProperties {
    #[facet(kdl::property, rename = "log-level")]
    log_level: String,
    #[facet(kdl::property, rename = "max-connections")]
    max_connections: u32,
}

#[test]
fn test_renamed_properties() {
    let kdl_input = r#"config log-level="debug" max-connections=100"#;
    let config: RenamedProperties = from_str(kdl_input).unwrap();
    assert_eq!(config.log_level, "debug");
    assert_eq!(config.max_connections, 100);
}

#[derive(Facet, Debug, PartialEq)]
#[facet(rename_all = "kebab-case")]
struct RenameAllConfig {
    #[facet(kdl::property)]
    log_level: String,
    #[facet(kdl::property)]
    max_connections: u32,
}

#[test]
fn test_rename_all() {
    let kdl_input = r#"config log-level="info" max-connections=50"#;
    let config: RenameAllConfig = from_str(kdl_input).unwrap();
    assert_eq!(config.log_level, "info");
    assert_eq!(config.max_connections, 50);
}

// ============================================================================
// Multiple children tests
// ============================================================================
// NOTE: kdl::children with Vec is a feature that requires format-specific
// handling. The current implementation routes all children to matching field
// names. For full kdl::children support with singularization, use facet-kdl.

#[derive(Facet, Debug, PartialEq)]
struct ItemWithProperty {
    #[facet(kdl::property)]
    name: String,
}

#[derive(Facet, Debug, PartialEq)]
struct ContainerWithChildren {
    #[facet(kdl::child)]
    first: ItemWithProperty,
    #[facet(kdl::child)]
    second: ItemWithProperty,
}

#[test]
fn test_multiple_named_children() {
    let kdl_input = r#"
        container {
            first name="one"
            second name="two"
        }
    "#;
    let container: ContainerWithChildren = from_str(kdl_input).unwrap();
    assert_eq!(container.first.name, "one");
    assert_eq!(container.second.name, "two");
}

// ============================================================================
// Raw string tests
// ============================================================================

#[derive(Facet, Debug, PartialEq)]
struct RawContent {
    #[facet(kdl::argument)]
    content: String,
}

#[test]
fn test_raw_string() {
    // Raw strings in KDL allow quotes without escaping
    let kdl_input = r##"content #"This has "quotes" inside"#"##;
    let content: RawContent = from_str(kdl_input).unwrap();
    assert_eq!(content.content, r#"This has "quotes" inside"#);
}

#[test]
fn test_multiline_string() {
    let kdl_input = r#"content """
hello
world
""""#;
    let content: RawContent = from_str(kdl_input).unwrap();
    assert_eq!(content.content, "hello\nworld");
}

// ============================================================================
// Default value tests
// ============================================================================

#[derive(Facet, Debug, PartialEq)]
struct WithDefaults {
    #[facet(kdl::property)]
    required: String,
    #[facet(kdl::property)]
    #[facet(default)]
    optional_num: i32,
    #[facet(kdl::property)]
    #[facet(default)]
    optional_str: String,
}

#[test]
fn test_default_values() {
    let kdl_input = r#"config required="must-have""#;
    let config: WithDefaults = from_str(kdl_input).unwrap();
    assert_eq!(config.required, "must-have");
    assert_eq!(config.optional_num, 0);
    assert_eq!(config.optional_str, "");
}

#[test]
fn test_override_defaults() {
    let kdl_input = r#"config required="yes" optional_num=42 optional_str="custom""#;
    let config: WithDefaults = from_str(kdl_input).unwrap();
    assert_eq!(config.required, "yes");
    assert_eq!(config.optional_num, 42);
    assert_eq!(config.optional_str, "custom");
}

// ============================================================================
// Char type tests
// ============================================================================

#[derive(Facet, Debug, PartialEq)]
struct CharValue {
    #[facet(kdl::property)]
    ch: char,
}

#[test]
fn test_char_value() {
    let kdl_input = r#"char ch="X""#;
    let result: CharValue = from_str(kdl_input).unwrap();
    assert_eq!(result.ch, 'X');
}

#[test]
fn test_unicode_char() {
    let kdl_input = r#"char ch="€""#;
    let result: CharValue = from_str(kdl_input).unwrap();
    assert_eq!(result.ch, '€');
}

// ============================================================================
// kdl::children Vec tests
// ============================================================================

#[derive(Facet, Debug, PartialEq)]
struct Item {
    #[facet(kdl::property)]
    name: String,
}

#[derive(Facet, Debug, PartialEq)]
struct ContainerWithChildrenVec {
    #[facet(kdl::children)]
    items: Vec<Item>,
}

#[test]
fn test_children_vec_basic() {
    // Test that kdl::children with Vec collects multiple child nodes
    let kdl_input = r#"
        container {
            item name="one"
            item name="two"
            item name="three"
        }
    "#;
    let container: ContainerWithChildrenVec = from_str(kdl_input).unwrap();
    assert_eq!(container.items.len(), 3);
    assert_eq!(container.items[0].name, "one");
    assert_eq!(container.items[1].name, "two");
    assert_eq!(container.items[2].name, "three");
}

#[test]
fn test_children_vec_empty() {
    let kdl_input = r#"container"#;
    let container: ContainerWithChildrenVec = from_str(kdl_input).unwrap();
    assert!(container.items.is_empty());
}

// ============================================================================
// Serialization tests
// ============================================================================

use facet_kdl::to_string;

#[test]
fn test_serialize_simple() {
    let server = Server {
        host: "localhost".to_string(),
        port: 8080,
    };
    let kdl = to_string(&server).unwrap();
    // Verify it round-trips
    let parsed: Server = from_str(&kdl).unwrap();
    assert_eq!(parsed, server);
}

#[test]
fn test_serialize_with_child() {
    let person = Person {
        name: "Bob".to_string(),
        address: Address {
            street: "456 Oak Ave".to_string(),
            city: "Portland".to_string(),
        },
    };
    let kdl = to_string(&person).unwrap();
    let parsed: Person = from_str(&kdl).unwrap();
    assert_eq!(parsed, person);
}

#[test]
fn test_serialize_nested() {
    let config = OuterConfig {
        middle: MiddleConfig {
            name: "nested-test".to_string(),
            inner: InnerConfig {
                enabled: false,
                level: 10,
            },
        },
    };
    let kdl = to_string(&config).unwrap();
    let parsed: OuterConfig = from_str(&kdl).unwrap();
    assert_eq!(parsed, config);
}
