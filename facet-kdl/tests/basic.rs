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

// ============================================================================
// Issue #1538: kdl::children with default regression
// ============================================================================
// This tests the scenario from the GitHub issue where kdl::children with
// default doesn't parse child nodes into vectors.

#[derive(Facet, Debug, PartialEq)]
struct SpecConfig {
    #[facet(kdl::property)]
    spec_name: String,
}

#[derive(Facet, Debug, PartialEq)]
struct ConfigWithDefaultChildren {
    #[facet(kdl::children, default)]
    specs: Vec<SpecConfig>,
}

#[test]
fn test_children_vec_with_default() {
    // Reproduce issue #1538: kdl::children with default should still parse children
    let kdl_input = r#"
        config {
            spec spec_name="test1"
            spec spec_name="test2"
        }
    "#;
    let config: ConfigWithDefaultChildren = from_str(kdl_input).unwrap();
    assert_eq!(
        config.specs.len(),
        2,
        "Expected 2 specs but got {}",
        config.specs.len()
    );
    assert_eq!(config.specs[0].spec_name, "test1");
    assert_eq!(config.specs[1].spec_name, "test2");
}

#[test]
fn test_children_vec_default_empty_children() {
    // With default, empty children block should give empty vec
    let kdl_input = r#"config"#;
    let config: ConfigWithDefaultChildren = from_str(kdl_input).unwrap();
    assert!(config.specs.is_empty());
}

// ============================================================================
// Issue #1538: Full reproduction with nested children
// ============================================================================

#[derive(Facet, Debug, PartialEq)]
struct NameNode {
    #[facet(kdl::argument)]
    value: String,
}

#[derive(Facet, Debug, PartialEq)]
struct RulesGlobNode {
    #[facet(kdl::argument)]
    pattern: String,
}

#[derive(Facet, Debug, PartialEq)]
struct IncludeNode {
    #[facet(kdl::argument)]
    pattern: String,
}

#[derive(Facet, Debug, PartialEq)]
struct FullSpecConfig {
    #[facet(kdl::child)]
    name: NameNode,
    #[facet(kdl::child)]
    rules_glob: RulesGlobNode,
    #[facet(kdl::child, default)]
    include: Option<IncludeNode>,
}

#[derive(Facet, Debug, PartialEq)]
struct FullConfig {
    #[facet(kdl::children, default)]
    specs: Vec<FullSpecConfig>,
}

#[test]
fn test_issue_1538_nested_children() {
    // This is the exact scenario from issue #1538
    let kdl_input = r#"
        config {
            spec {
                name "test-spec"
                rules_glob "docs/spec/**/*.md"
                include "**/*.rs"
            }
        }
    "#;
    let config: FullConfig = from_str(kdl_input).unwrap();
    assert_eq!(
        config.specs.len(),
        1,
        "Expected 1 spec but got {}",
        config.specs.len()
    );
    assert_eq!(config.specs[0].name.value, "test-spec");
    assert_eq!(config.specs[0].rules_glob.pattern, "docs/spec/**/*.md");
    assert_eq!(
        config.specs[0].include,
        Some(IncludeNode {
            pattern: "**/*.rs".to_string()
        })
    );
}

#[test]
fn test_issue_1538_multiple_nested_children() {
    let kdl_input = r#"
        config {
            spec {
                name "spec1"
                rules_glob "pattern1"
            }
            spec {
                name "spec2"
                rules_glob "pattern2"
                include "inc"
            }
        }
    "#;
    let config: FullConfig = from_str(kdl_input).unwrap();
    assert_eq!(
        config.specs.len(),
        2,
        "Expected 2 specs but got {}",
        config.specs.len()
    );
    assert_eq!(config.specs[0].name.value, "spec1");
    assert_eq!(config.specs[1].name.value, "spec2");
}

// Debug: Test simpler nested case without the Vec
#[test]
fn test_single_nested_spec() {
    // Test parsing a single spec with nested children first
    let kdl_input = r#"
        spec {
            name "test-spec"
            rules_glob "pattern"
        }
    "#;
    let spec: FullSpecConfig = from_str(kdl_input).unwrap();
    assert_eq!(spec.name.value, "test-spec");
    assert_eq!(spec.rules_glob.pattern, "pattern");
}

// This is the existing pattern that WORKS - see test_deeply_nested_structs
// The difference is the structure of the KDL - here children are nested one level deeper

// Let me verify the working pattern has the same structure
#[test]
fn test_verify_existing_deeply_nested() {
    // This is from the existing tests and should work
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

#[test]
fn debug_serialize() {
    let server = Server {
        host: "localhost".to_string(),
        port: 8080,
    };
    let kdl = to_string(&server).unwrap();
    println!("Serialized KDL: {}", kdl);
}

// ============================================================================
// Comprehensive KDL attribute tests
// ============================================================================
// Testing all KDL attributes:
// - kdl::argument (single positional)
// - kdl::arguments (all positional as Vec)
// - kdl::property (key=value)
// - kdl::child (single child node)
// - kdl::child = "name" (custom child name)
// - kdl::children (multiple children as Vec)
// - kdl::children = "name" (custom children name)
// - kdl::node_name (captures node name)

// --- kdl::arguments (plural) ---
#[derive(Facet, Debug, PartialEq)]
struct MultiArgNode {
    #[facet(kdl::arguments)]
    args: Vec<String>,
}

#[test]
#[ignore = "kdl::arguments (plural) not yet implemented - needs parser support"]
fn test_arguments_plural_basic() {
    let kdl_input = r#"node "first" "second" "third""#;
    let result: MultiArgNode = from_str(kdl_input).unwrap();
    assert_eq!(result.args, vec!["first", "second", "third"]);
}

#[test]
#[ignore = "kdl::arguments (plural) not yet implemented - needs parser support"]
fn test_arguments_plural_empty() {
    let kdl_input = r#"node"#;
    let result: MultiArgNode = from_str(kdl_input).unwrap();
    assert!(result.args.is_empty());
}

#[test]
#[ignore = "kdl::arguments (plural) not yet implemented - needs parser support"]
fn test_arguments_plural_single() {
    let kdl_input = r#"node "only-one""#;
    let result: MultiArgNode = from_str(kdl_input).unwrap();
    assert_eq!(result.args, vec!["only-one"]);
}

// --- kdl::arguments with property ---
#[derive(Facet, Debug, PartialEq)]
struct ArgsWithProp {
    #[facet(kdl::arguments)]
    values: Vec<i32>,
    #[facet(kdl::property)]
    name: String,
}

#[test]
fn test_arguments_with_property() {
    let kdl_input = r#"node 1 2 3 name="test""#;
    let result: ArgsWithProp = from_str(kdl_input).unwrap();
    assert_eq!(result.values, vec![1, 2, 3]);
    assert_eq!(result.name, "test");
}

// --- kdl::node_name ---
#[derive(Facet, Debug, PartialEq)]
struct CapturesNodeName {
    #[facet(kdl::node_name)]
    name: String,
    #[facet(kdl::property)]
    value: i32,
}

#[test]
fn test_node_name_capture() {
    let kdl_input = r#"my-custom-node value=42"#;
    let result: CapturesNodeName = from_str(kdl_input).unwrap();
    assert_eq!(result.name, "my-custom-node");
    assert_eq!(result.value, 42);
}

#[test]
fn test_node_name_with_different_nodes() {
    let kdl1: CapturesNodeName = from_str(r#"alpha value=1"#).unwrap();
    let kdl2: CapturesNodeName = from_str(r#"beta value=2"#).unwrap();
    assert_eq!(kdl1.name, "alpha");
    assert_eq!(kdl2.name, "beta");
}

// --- Combined attributes test ---
#[derive(Facet, Debug, PartialEq)]
struct FullKdlNode {
    #[facet(kdl::argument)]
    main_value: String,
    #[facet(kdl::property)]
    flag: bool,
    #[facet(kdl::property)]
    count: i32,
}

#[derive(Facet, Debug, PartialEq)]
struct ChildItem {
    #[facet(kdl::property)]
    id: i32,
}

#[derive(Facet, Debug, PartialEq)]
struct ComplexConfig {
    #[facet(kdl::node_name)]
    config_type: String,
    #[facet(kdl::argument)]
    name: String,
    #[facet(kdl::property)]
    enabled: bool,
    #[facet(kdl::child)]
    settings: FullKdlNode,
    #[facet(kdl::children)]
    items: Vec<ChildItem>,
}

#[test]
fn test_all_attributes_combined() {
    let kdl_input = r#"
        database "production" enabled=#true {
            settings "main" flag=#true count=5
            item id=100
            item id=200
        }
    "#;
    let result: ComplexConfig = from_str(kdl_input).unwrap();
    assert_eq!(result.config_type, "database");
    assert_eq!(result.name, "production");
    assert!(result.enabled);
    assert_eq!(result.settings.main_value, "main");
    assert!(result.settings.flag);
    assert_eq!(result.settings.count, 5);
    assert_eq!(result.items.len(), 2);
    assert_eq!(result.items[0].id, 100);
    assert_eq!(result.items[1].id, 200);
}

// --- Round-trip serialization tests for all attributes ---
#[test]
fn test_roundtrip_argument_and_property() {
    let original = Server {
        host: "example.com".to_string(),
        port: 9000,
    };
    let kdl = to_string(&original).unwrap();
    let parsed: Server = from_str(&kdl).unwrap();
    assert_eq!(parsed, original);
}

#[test]
fn test_roundtrip_nested_children() {
    let original = OuterConfig {
        middle: MiddleConfig {
            name: "roundtrip-test".to_string(),
            inner: InnerConfig {
                enabled: true,
                level: 42,
            },
        },
    };
    let kdl = to_string(&original).unwrap();
    let parsed: OuterConfig = from_str(&kdl).unwrap();
    assert_eq!(parsed, original);
}

#[test]
fn test_roundtrip_children_vec() {
    let original = ContainerWithChildrenVec {
        items: vec![
            Item {
                name: "first".to_string(),
            },
            Item {
                name: "second".to_string(),
            },
        ],
    };
    let kdl = to_string(&original).unwrap();
    println!("Serialized KDL:\n{}", kdl);
    let parsed: ContainerWithChildrenVec = from_str(&kdl).unwrap();
    assert_eq!(parsed, original);
}

// =============================================================================
// Additional coverage tests
// =============================================================================

// --- Flatten tests ---

#[derive(Facet, Debug, PartialEq)]
struct Coordinates {
    #[facet(kdl::property)]
    x: i32,
    #[facet(kdl::property)]
    y: i32,
}

#[derive(Facet, Debug, PartialEq)]
struct PointWithName {
    #[facet(kdl::property)]
    name: String,
    #[facet(flatten)]
    coords: Coordinates,
}

#[test]
fn test_flatten_struct() {
    let kdl_input = r#"point name="origin" x=0 y=0"#;
    let result: PointWithName = from_str(kdl_input).unwrap();
    assert_eq!(result.name, "origin");
    assert_eq!(result.coords.x, 0);
    assert_eq!(result.coords.y, 0);
}

#[test]
fn test_flatten_roundtrip() {
    let original = PointWithName {
        name: "center".to_string(),
        coords: Coordinates { x: 10, y: 20 },
    };
    let kdl = to_string(&original).unwrap();
    let parsed: PointWithName = from_str(&kdl).unwrap();
    assert_eq!(parsed, original);
}

// --- Enum tests ---
// NOTE: Enums with struct variants and HashMaps as children are complex
// KDL mappings that may require additional implementation work.
// Basic unit enum variants can be tested as properties:

#[derive(Facet, Debug, PartialEq)]
#[repr(u8)]
enum Status {
    Active,
    Inactive,
    Pending,
}

#[derive(Facet, Debug, PartialEq)]
struct TaskWithStatusProp {
    #[facet(kdl::property)]
    name: String,
    #[facet(kdl::property)]
    status: Status,
}

#[test]
fn test_enum_as_property() {
    // Test that unit enums work as properties (string matching)
    let kdl_input = r#"task name="build" status="Active""#;
    let result: TaskWithStatusProp = from_str(kdl_input).unwrap();
    assert_eq!(result.name, "build");
    assert_eq!(result.status, Status::Active);
}

// --- Box/Arc/Rc wrapper tests ---

use std::rc::Rc;
use std::sync::Arc;

#[derive(Facet, Debug, PartialEq)]
struct BoxWrapper {
    #[facet(kdl::property)]
    inner: Box<i32>,
}

#[test]
fn test_box_wrapper() {
    let kdl_input = r#"wrapper inner=42"#;
    let result: BoxWrapper = from_str(kdl_input).unwrap();
    assert_eq!(*result.inner, 42);
}

#[derive(Facet, Debug)]
struct ArcWrapper {
    #[facet(kdl::property)]
    inner: Arc<String>,
}

#[test]
fn test_arc_wrapper() {
    let kdl_input = r#"wrapper inner="hello""#;
    let result: ArcWrapper = from_str(kdl_input).unwrap();
    assert_eq!(*result.inner, "hello");
}

#[derive(Facet, Debug)]
struct RcWrapper {
    #[facet(kdl::property)]
    inner: Rc<String>,
}

#[test]
fn test_rc_wrapper() {
    let kdl_input = r#"wrapper inner="world""#;
    let result: RcWrapper = from_str(kdl_input).unwrap();
    assert_eq!(*result.inner, "world");
}

// --- Transparent newtype tests ---

#[derive(Facet, Debug, PartialEq)]
#[facet(transparent)]
struct UserId(i64);

#[derive(Facet, Debug, PartialEq)]
struct User {
    #[facet(kdl::property)]
    id: UserId,
    #[facet(kdl::property)]
    name: String,
}

#[test]
fn test_transparent_newtype() {
    let kdl_input = r#"user id=12345 name="alice""#;
    let result: User = from_str(kdl_input).unwrap();
    assert_eq!(result.id, UserId(12345));
    assert_eq!(result.name, "alice");
}

// --- Error case tests ---

#[derive(Facet, Debug, PartialEq)]
struct RequiredFields {
    #[facet(kdl::property)]
    required: String,
    #[facet(kdl::property)]
    also_required: i32,
}

#[test]
fn test_error_missing_required_field() {
    let kdl_input = r#"record required="present""#;
    let result: Result<RequiredFields, _> = from_str(kdl_input);
    assert!(result.is_err());
    let err = result.unwrap_err().to_string();
    assert!(err.contains("also_required") || err.contains("missing"));
}

#[derive(Facet, Debug, PartialEq)]
struct TypedValue {
    #[facet(kdl::property)]
    count: i32,
}

#[test]
fn test_error_type_mismatch() {
    let kdl_input = r#"record count="not_a_number""#;
    let result: Result<TypedValue, _> = from_str(kdl_input);
    assert!(result.is_err());
}

#[derive(Facet, Debug, PartialEq)]
#[facet(deny_unknown_fields)]
struct StrictRecord {
    #[facet(kdl::property)]
    known: String,
}

#[test]
fn test_deny_unknown_fields() {
    let kdl_input = r#"record known="value" unknown="bad""#;
    let result: Result<StrictRecord, _> = from_str(kdl_input);
    assert!(result.is_err());
    let err = result.unwrap_err().to_string();
    assert!(err.contains("unknown") || err.contains("Unknown"));
}

// --- Tuple tests ---
// NOTE: Tuples as children require specific KDL syntax that may need
// further implementation work. Skipping for now.

// --- Float edge cases ---

#[derive(Facet, Debug, PartialEq)]
struct FloatValues {
    #[facet(kdl::property)]
    positive: f64,
    #[facet(kdl::property)]
    negative: f64,
    #[facet(kdl::property)]
    small: f64,
}

#[test]
fn test_float_values() {
    let kdl_input = r#"floats positive=1.5 negative=-2.25 small=0.001"#;
    let result: FloatValues = from_str(kdl_input).unwrap();
    assert!((result.positive - 1.5).abs() < f64::EPSILON);
    assert!((result.negative - (-2.25)).abs() < f64::EPSILON);
    assert!((result.small - 0.001).abs() < f64::EPSILON);
}

#[test]
fn test_scientific_notation() {
    let kdl_input = r#"floats positive=1.5e10 negative=-2.25e-5 small=5e3"#;
    let result: FloatValues = from_str(kdl_input).unwrap();
    assert!((result.positive - 1.5e10).abs() < 1e5);
    assert!((result.negative - (-2.25e-5)).abs() < 1e-10);
    assert!((result.small - 5e3).abs() < f64::EPSILON);
}

// --- String escape tests ---

#[derive(Facet, Debug, PartialEq)]
struct EscapedString {
    #[facet(kdl::property)]
    text: String,
}

#[test]
fn test_string_escapes() {
    let kdl_input = r#"record text="line1\nline2\ttab""#;
    let result: EscapedString = from_str(kdl_input).unwrap();
    assert_eq!(result.text, "line1\nline2\ttab");
}

#[test]
fn test_string_quote_escape() {
    let kdl_input = r#"record text="say \"hello\"""#;
    let result: EscapedString = from_str(kdl_input).unwrap();
    assert_eq!(result.text, "say \"hello\"");
}

// --- Vec of scalars ---
// NOTE: kdl::children expects struct-like child nodes, not raw scalars.
// For scalar collections, use kdl::arguments (plural) instead.

// --- Alias attribute ---

#[derive(Facet, Debug, PartialEq)]
struct AliasedField {
    #[facet(kdl::property, alias = "old_name")]
    new_name: String,
}

#[test]
fn test_alias_uses_old_name() {
    let kdl_input = r#"record old_name="value""#;
    let result: AliasedField = from_str(kdl_input).unwrap();
    assert_eq!(result.new_name, "value");
}

#[test]
fn test_alias_uses_new_name() {
    let kdl_input = r#"record new_name="value""#;
    let result: AliasedField = from_str(kdl_input).unwrap();
    assert_eq!(result.new_name, "value");
}
