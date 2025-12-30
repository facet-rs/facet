//! Basic tests for KDL parsing.
//!
//! In facet-kdl, the Rust type (schema) determines how KDL is interpreted.
//! Root-level KDL nodes are treated as children of an implicit document struct.
//! To deserialize a single node, use a wrapper struct with `kdl::child`.

use facet::Facet;
use facet_kdl as kdl;
use facet_kdl::from_str;

// ============================================================================
// Basic node structures
// ============================================================================

#[derive(Facet, Debug, PartialEq)]
struct SimpleValue {
    #[facet(kdl::argument)]
    value: String,
}

/// Wrapper to receive a single `node` child
#[derive(Facet, Debug, PartialEq)]
struct SimpleValueDoc {
    #[facet(kdl::child)]
    node: SimpleValue,
}

#[test]
fn test_single_argument() {
    let kdl_input = r#"node "hello""#;
    let result: SimpleValueDoc = from_str(kdl_input).unwrap();
    assert_eq!(result.node.value, "hello");
}

#[derive(Facet, Debug, PartialEq)]
struct Server {
    #[facet(kdl::argument)]
    host: String,
    #[facet(kdl::property)]
    port: u16,
}

/// Wrapper to receive a single `server` child
#[derive(Facet, Debug, PartialEq)]
struct ServerDoc {
    #[facet(kdl::child)]
    server: Server,
}

#[test]
fn test_argument_and_property() {
    let kdl_input = r#"server "localhost" port=8080"#;
    let doc: ServerDoc = from_str(kdl_input).unwrap();
    assert_eq!(doc.server.host, "localhost");
    assert_eq!(doc.server.port, 8080);
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

#[derive(Facet, Debug, PartialEq)]
struct NumbersDoc {
    #[facet(kdl::child)]
    numbers: Numbers,
}

#[test]
fn test_multiple_properties() {
    let kdl_input = r#"numbers a=-42 b=3.125 c=#true"#;
    let doc: NumbersDoc = from_str(kdl_input).unwrap();
    assert_eq!(doc.numbers.a, -42);
    assert!((doc.numbers.b - 3.125).abs() < 0.001);
    assert!(doc.numbers.c);
}

#[test]
fn test_false_bool() {
    let kdl_input = r#"numbers a=0 b=0.0 c=#false"#;
    let doc: NumbersDoc = from_str(kdl_input).unwrap();
    assert_eq!(doc.numbers.a, 0);
    assert!((doc.numbers.b - 0.0).abs() < 0.001);
    assert!(!doc.numbers.c);
}

// ============================================================================
// Child nodes
// ============================================================================

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

#[derive(Facet, Debug, PartialEq)]
struct PersonDoc {
    #[facet(kdl::child)]
    person: Person,
}

#[test]
fn test_child_node() {
    let kdl_input = r#"
        person "Alice" {
            address street="123 Main St" city="Springfield"
        }
    "#;
    let doc: PersonDoc = from_str(kdl_input).unwrap();
    assert_eq!(doc.person.name, "Alice");
    assert_eq!(doc.person.address.street, "123 Main St");
    assert_eq!(doc.person.address.city, "Springfield");
}

// ============================================================================
// Null and Option values
// ============================================================================

#[derive(Facet, Debug, PartialEq)]
struct MaybeValue {
    #[facet(kdl::property)]
    value: Option<String>,
}

#[derive(Facet, Debug, PartialEq)]
struct MaybeValueDoc {
    #[facet(kdl::child)]
    config: MaybeValue,
}

#[test]
fn test_null_value() {
    let kdl_input = r#"config value=#null"#;
    let doc: MaybeValueDoc = from_str(kdl_input).unwrap();
    assert_eq!(doc.config.value, None);
}

#[test]
fn test_some_value() {
    let kdl_input = r#"config value="hello""#;
    let doc: MaybeValueDoc = from_str(kdl_input).unwrap();
    assert_eq!(doc.config.value, Some("hello".to_string()));
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

#[derive(Facet, Debug, PartialEq)]
struct IntegerTypesDoc {
    #[facet(kdl::child)]
    ints: IntegerTypes,
}

#[test]
fn test_integer_types() {
    let kdl_input = r#"ints u8_val=255 i16_val=-1000 u32_val=1000000 i64_val=-9223372036854775808"#;
    let doc: IntegerTypesDoc = from_str(kdl_input).unwrap();
    assert_eq!(doc.ints.u8_val, 255);
    assert_eq!(doc.ints.i16_val, -1000);
    assert_eq!(doc.ints.u32_val, 1000000);
    assert_eq!(doc.ints.i64_val, i64::MIN);
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

#[derive(Facet, Debug, PartialEq)]
struct OuterConfigDoc {
    #[facet(kdl::child)]
    root: OuterConfig,
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
    let doc: OuterConfigDoc = from_str(kdl_input).unwrap();
    assert_eq!(doc.root.middle.name, "test");
    assert!(doc.root.middle.inner.enabled);
    assert_eq!(doc.root.middle.inner.level, 5);
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

#[derive(Facet, Debug, PartialEq)]
struct ParentDoc {
    #[facet(kdl::child)]
    parent: ParentWithOptionalChild,
}

#[test]
fn test_optional_child_present() {
    let kdl_input = r#"
        parent "main" {
            child value="nested"
        }
    "#;
    let doc: ParentDoc = from_str(kdl_input).unwrap();
    assert_eq!(doc.parent.id, "main");
    assert_eq!(
        doc.parent.child,
        Some(OptionalChildComplex {
            value: "nested".to_string()
        })
    );
}

#[test]
fn test_optional_child_absent() {
    let kdl_input = r#"parent "main""#;
    let doc: ParentDoc = from_str(kdl_input).unwrap();
    assert_eq!(doc.parent.id, "main");
    assert_eq!(doc.parent.child, None);
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

#[derive(Facet, Debug, PartialEq)]
struct ConfigWithSkipDoc {
    #[facet(kdl::child)]
    config: ConfigWithSkip,
}

#[test]
fn test_skip_field() {
    let kdl_input = r#"config name="test""#;
    let doc: ConfigWithSkipDoc = from_str(kdl_input).unwrap();
    assert_eq!(doc.config.name, "test");
    assert_eq!(doc.config.internal_id, 0); // Default value
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

#[derive(Facet, Debug, PartialEq)]
struct RenamedPropertiesDoc {
    #[facet(kdl::child)]
    config: RenamedProperties,
}

#[test]
fn test_renamed_properties() {
    let kdl_input = r#"config log-level="debug" max-connections=100"#;
    let doc: RenamedPropertiesDoc = from_str(kdl_input).unwrap();
    assert_eq!(doc.config.log_level, "debug");
    assert_eq!(doc.config.max_connections, 100);
}

#[derive(Facet, Debug, PartialEq)]
#[facet(rename_all = "kebab-case")]
struct RenameAllConfig {
    #[facet(kdl::property)]
    log_level: String,
    #[facet(kdl::property)]
    max_connections: u32,
}

#[derive(Facet, Debug, PartialEq)]
struct RenameAllConfigDoc {
    #[facet(kdl::child)]
    config: RenameAllConfig,
}

#[test]
fn test_rename_all() {
    let kdl_input = r#"config log-level="info" max-connections=50"#;
    let doc: RenameAllConfigDoc = from_str(kdl_input).unwrap();
    assert_eq!(doc.config.log_level, "info");
    assert_eq!(doc.config.max_connections, 50);
}

// ============================================================================
// Multiple children tests
// ============================================================================

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

#[derive(Facet, Debug, PartialEq)]
struct ContainerDoc {
    #[facet(kdl::child)]
    container: ContainerWithChildren,
}

#[test]
fn test_multiple_named_children() {
    let kdl_input = r#"
        container {
            first name="one"
            second name="two"
        }
    "#;
    let doc: ContainerDoc = from_str(kdl_input).unwrap();
    assert_eq!(doc.container.first.name, "one");
    assert_eq!(doc.container.second.name, "two");
}

// ============================================================================
// Raw string tests
// ============================================================================

#[derive(Facet, Debug, PartialEq)]
struct RawContent {
    #[facet(kdl::argument)]
    content: String,
}

#[derive(Facet, Debug, PartialEq)]
struct RawContentDoc {
    #[facet(kdl::child)]
    content: RawContent,
}

#[test]
fn test_raw_string() {
    // Raw strings in KDL allow quotes without escaping
    let kdl_input = r##"content #"This has "quotes" inside"#"##;
    let doc: RawContentDoc = from_str(kdl_input).unwrap();
    assert_eq!(doc.content.content, r#"This has "quotes" inside"#);
}

#[test]
fn test_multiline_string() {
    let kdl_input = r#"content """
hello
world
""""#;
    let doc: RawContentDoc = from_str(kdl_input).unwrap();
    assert_eq!(doc.content.content, "hello\nworld");
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

#[derive(Facet, Debug, PartialEq)]
struct WithDefaultsDoc {
    #[facet(kdl::child)]
    config: WithDefaults,
}

#[test]
fn test_default_values() {
    let kdl_input = r#"config required="must-have""#;
    let doc: WithDefaultsDoc = from_str(kdl_input).unwrap();
    assert_eq!(doc.config.required, "must-have");
    assert_eq!(doc.config.optional_num, 0);
    assert_eq!(doc.config.optional_str, "");
}

#[test]
fn test_override_defaults() {
    let kdl_input = r#"config required="yes" optional_num=42 optional_str="custom""#;
    let doc: WithDefaultsDoc = from_str(kdl_input).unwrap();
    assert_eq!(doc.config.required, "yes");
    assert_eq!(doc.config.optional_num, 42);
    assert_eq!(doc.config.optional_str, "custom");
}

// ============================================================================
// Char type tests
// ============================================================================

#[derive(Facet, Debug, PartialEq)]
struct CharValue {
    #[facet(kdl::property)]
    ch: char,
}

#[derive(Facet, Debug, PartialEq)]
struct CharValueDoc {
    #[facet(kdl::child)]
    char: CharValue,
}

#[test]
fn test_char_value() {
    let kdl_input = r#"char ch="X""#;
    let doc: CharValueDoc = from_str(kdl_input).unwrap();
    assert_eq!(doc.char.ch, 'X');
}

#[test]
fn test_unicode_char() {
    let kdl_input = r#"char ch="€""#;
    let doc: CharValueDoc = from_str(kdl_input).unwrap();
    assert_eq!(doc.char.ch, '€');
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

#[derive(Facet, Debug, PartialEq)]
struct ContainerWithChildrenVecDoc {
    #[facet(kdl::child)]
    container: ContainerWithChildrenVec,
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
    let doc: ContainerWithChildrenVecDoc = from_str(kdl_input).unwrap();
    assert_eq!(doc.container.items.len(), 3);
    assert_eq!(doc.container.items[0].name, "one");
    assert_eq!(doc.container.items[1].name, "two");
    assert_eq!(doc.container.items[2].name, "three");
}

#[test]
fn test_children_vec_empty() {
    let kdl_input = r#"container"#;
    let doc: ContainerWithChildrenVecDoc = from_str(kdl_input).unwrap();
    assert!(doc.container.items.is_empty());
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
    // The serializer outputs a single node, wrap in doc to parse
    let doc: ServerDoc = from_str(&kdl).unwrap();
    assert_eq!(doc.server, server);
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
    let doc: PersonDoc = from_str(&kdl).unwrap();
    assert_eq!(doc.person, person);
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
    println!("Serialized KDL:\n{}", kdl);
    // The serializer uses type name as node name (OuterConfig → OuterConfig node)
    // Use kdl::children to capture any node
    #[derive(Facet, Debug, PartialEq)]
    struct AnyDoc {
        #[facet(kdl::children)]
        items: Vec<OuterConfig>,
    }
    let doc: AnyDoc = from_str(&kdl).unwrap();
    assert_eq!(doc.items.len(), 1);
    assert_eq!(doc.items[0], config);
}

// ============================================================================
// Issue #1538 / #1540: kdl::children with default - single root node
// ============================================================================
// This tests the scenario where a single root node should be collected
// into a kdl::children Vec via singularization.

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
fn test_children_vec_with_default_single_node() {
    // Issue #1540: A single root node should be collected into kdl::children
    // via singularization (spec → specs)
    let kdl_input = r#"spec spec_name="test1""#;
    let config: ConfigWithDefaultChildren = from_str(kdl_input).unwrap();
    assert_eq!(
        config.specs.len(),
        1,
        "Expected 1 spec but got {}",
        config.specs.len()
    );
    assert_eq!(config.specs[0].spec_name, "test1");
}

#[test]
fn test_children_vec_with_default_multiple_nodes() {
    // Multiple root nodes should all be collected
    let kdl_input = r#"
        spec spec_name="test1"
        spec spec_name="test2"
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
    // Empty document should give empty vec with default
    let kdl_input = r#""#;
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
fn test_issue_1538_single_spec() {
    // Single spec node should be collected via singularization
    let kdl_input = r#"
        spec {
            name "test-spec"
            rules_glob "docs/spec/**/*.md"
            include "**/*.rs"
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
        spec {
            name "spec1"
            rules_glob "pattern1"
        }
        spec {
            name "spec2"
            rules_glob "pattern2"
            include "inc"
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

// Test parsing a single spec directly (with wrapper)
#[derive(Facet, Debug, PartialEq)]
struct FullSpecConfigDoc {
    #[facet(kdl::child)]
    spec: FullSpecConfig,
}

#[test]
fn test_single_nested_spec() {
    let kdl_input = r#"
        spec {
            name "test-spec"
            rules_glob "pattern"
        }
    "#;
    let doc: FullSpecConfigDoc = from_str(kdl_input).unwrap();
    assert_eq!(doc.spec.name.value, "test-spec");
    assert_eq!(doc.spec.rules_glob.pattern, "pattern");
}

#[test]
fn test_verify_existing_deeply_nested() {
    let kdl_input = r#"
        root {
            middle "test" {
                inner enabled=#true level=5
            }
        }
    "#;
    let doc: OuterConfigDoc = from_str(kdl_input).unwrap();
    assert_eq!(doc.root.middle.name, "test");
    assert!(doc.root.middle.inner.enabled);
    assert_eq!(doc.root.middle.inner.level, 5);
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

// --- kdl::arguments (plural) ---
#[derive(Facet, Debug, PartialEq)]
struct MultiArgNode {
    #[facet(kdl::arguments)]
    args: Vec<String>,
}

#[derive(Facet, Debug, PartialEq)]
struct MultiArgNodeDoc {
    #[facet(kdl::child)]
    node: MultiArgNode,
}

#[test]
#[ignore = "kdl::arguments (plural) not yet implemented - needs parser support"]
fn test_arguments_plural_basic() {
    let kdl_input = r#"node "first" "second" "third""#;
    let doc: MultiArgNodeDoc = from_str(kdl_input).unwrap();
    assert_eq!(doc.node.args, vec!["first", "second", "third"]);
}

#[test]
#[ignore = "kdl::arguments (plural) not yet implemented - needs parser support"]
fn test_arguments_plural_empty() {
    let kdl_input = r#"node"#;
    let doc: MultiArgNodeDoc = from_str(kdl_input).unwrap();
    assert!(doc.node.args.is_empty());
}

#[test]
#[ignore = "kdl::arguments (plural) not yet implemented - needs parser support"]
fn test_arguments_plural_single() {
    let kdl_input = r#"node "only-one""#;
    let doc: MultiArgNodeDoc = from_str(kdl_input).unwrap();
    assert_eq!(doc.node.args, vec!["only-one"]);
}

// --- kdl::arguments with property ---
#[derive(Facet, Debug, PartialEq)]
struct ArgsWithProp {
    #[facet(kdl::arguments)]
    values: Vec<i32>,
    #[facet(kdl::property)]
    name: String,
}

#[derive(Facet, Debug, PartialEq)]
struct ArgsWithPropDoc {
    #[facet(kdl::child)]
    node: ArgsWithProp,
}

#[test]
fn test_arguments_with_property() {
    let kdl_input = r#"node 1 2 3 name="test""#;
    let doc: ArgsWithPropDoc = from_str(kdl_input).unwrap();
    assert_eq!(doc.node.values, vec![1, 2, 3]);
    assert_eq!(doc.node.name, "test");
}

// --- kdl::node_name ---
#[derive(Facet, Debug, PartialEq)]
struct CapturesNodeName {
    #[facet(kdl::node_name)]
    name: String,
    #[facet(kdl::property)]
    value: i32,
}

/// To capture an arbitrary node name, use kdl::children since we don't know the name ahead of time
#[derive(Facet, Debug, PartialEq)]
struct CapturesNodeNameDoc {
    #[facet(kdl::children)]
    nodes: Vec<CapturesNodeName>,
}

#[test]
fn test_node_name_capture() {
    let kdl_input = r#"my-custom-node value=42"#;
    let doc: CapturesNodeNameDoc = from_str(kdl_input).unwrap();
    assert_eq!(doc.nodes.len(), 1);
    assert_eq!(doc.nodes[0].name, "my-custom-node");
    assert_eq!(doc.nodes[0].value, 42);
}

#[test]
fn test_node_name_with_different_nodes() {
    let kdl_input = r#"
        alpha value=1
        beta value=2
    "#;
    let doc: CapturesNodeNameDoc = from_str(kdl_input).unwrap();
    assert_eq!(doc.nodes.len(), 2);
    assert_eq!(doc.nodes[0].name, "alpha");
    assert_eq!(doc.nodes[1].name, "beta");
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

#[derive(Facet, Debug, PartialEq)]
struct ComplexConfigDoc {
    #[facet(kdl::child)]
    database: ComplexConfig,
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
    let doc: ComplexConfigDoc = from_str(kdl_input).unwrap();
    assert_eq!(doc.database.config_type, "database");
    assert_eq!(doc.database.name, "production");
    assert!(doc.database.enabled);
    assert_eq!(doc.database.settings.main_value, "main");
    assert!(doc.database.settings.flag);
    assert_eq!(doc.database.settings.count, 5);
    assert_eq!(doc.database.items.len(), 2);
    assert_eq!(doc.database.items[0].id, 100);
    assert_eq!(doc.database.items[1].id, 200);
}

// --- Round-trip serialization tests ---
// Note: The serializer uses type names as node names. We use kdl::children to
// capture any node type for round-tripping.

#[test]
fn test_roundtrip_argument_and_property() {
    let original = Server {
        host: "example.com".to_string(),
        port: 9000,
    };
    let kdl = to_string(&original).unwrap();
    #[derive(Facet, Debug, PartialEq)]
    struct Doc {
        #[facet(kdl::children)]
        items: Vec<Server>,
    }
    let doc: Doc = from_str(&kdl).unwrap();
    assert_eq!(doc.items.len(), 1);
    assert_eq!(doc.items[0], original);
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
    #[derive(Facet, Debug, PartialEq)]
    struct Doc {
        #[facet(kdl::children)]
        items: Vec<OuterConfig>,
    }
    let doc: Doc = from_str(&kdl).unwrap();
    assert_eq!(doc.items.len(), 1);
    assert_eq!(doc.items[0], original);
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
    #[derive(Facet, Debug, PartialEq)]
    struct Doc {
        #[facet(kdl::children)]
        items: Vec<ContainerWithChildrenVec>,
    }
    let doc: Doc = from_str(&kdl).unwrap();
    assert_eq!(doc.items.len(), 1);
    assert_eq!(doc.items[0], original);
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

#[derive(Facet, Debug, PartialEq)]
struct PointDoc {
    #[facet(kdl::child)]
    point: PointWithName,
}

#[test]
fn test_flatten_struct() {
    let kdl_input = r#"point name="origin" x=0 y=0"#;
    let doc: PointDoc = from_str(kdl_input).unwrap();
    assert_eq!(doc.point.name, "origin");
    assert_eq!(doc.point.coords.x, 0);
    assert_eq!(doc.point.coords.y, 0);
}

#[test]
fn test_flatten_roundtrip() {
    let original = PointWithName {
        name: "center".to_string(),
        coords: Coordinates { x: 10, y: 20 },
    };
    let kdl = to_string(&original).unwrap();
    #[derive(Facet, Debug, PartialEq)]
    struct Doc {
        #[facet(kdl::children)]
        items: Vec<PointWithName>,
    }
    let doc: Doc = from_str(&kdl).unwrap();
    assert_eq!(doc.items.len(), 1);
    assert_eq!(doc.items[0], original);
}

// --- Enum tests ---

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

#[derive(Facet, Debug, PartialEq)]
struct TaskDoc {
    #[facet(kdl::child)]
    task: TaskWithStatusProp,
}

#[test]
fn test_enum_as_property() {
    let kdl_input = r#"task name="build" status="Active""#;
    let doc: TaskDoc = from_str(kdl_input).unwrap();
    assert_eq!(doc.task.name, "build");
    assert_eq!(doc.task.status, Status::Active);
}

// --- Box/Arc/Rc wrapper tests ---

use std::rc::Rc;
use std::sync::Arc;

#[derive(Facet, Debug, PartialEq)]
struct BoxWrapper {
    #[facet(kdl::property)]
    inner: Box<i32>,
}

#[derive(Facet, Debug, PartialEq)]
struct BoxWrapperDoc {
    #[facet(kdl::child)]
    wrapper: BoxWrapper,
}

#[test]
fn test_box_wrapper() {
    let kdl_input = r#"wrapper inner=42"#;
    let doc: BoxWrapperDoc = from_str(kdl_input).unwrap();
    assert_eq!(*doc.wrapper.inner, 42);
}

#[derive(Facet, Debug)]
struct ArcWrapper {
    #[facet(kdl::property)]
    inner: Arc<String>,
}

#[derive(Facet, Debug)]
struct ArcWrapperDoc {
    #[facet(kdl::child)]
    wrapper: ArcWrapper,
}

#[test]
fn test_arc_wrapper() {
    let kdl_input = r#"wrapper inner="hello""#;
    let doc: ArcWrapperDoc = from_str(kdl_input).unwrap();
    assert_eq!(*doc.wrapper.inner, "hello");
}

#[derive(Facet, Debug)]
struct RcWrapper {
    #[facet(kdl::property)]
    inner: Rc<String>,
}

#[derive(Facet, Debug)]
struct RcWrapperDoc {
    #[facet(kdl::child)]
    wrapper: RcWrapper,
}

#[test]
fn test_rc_wrapper() {
    let kdl_input = r#"wrapper inner="world""#;
    let doc: RcWrapperDoc = from_str(kdl_input).unwrap();
    assert_eq!(*doc.wrapper.inner, "world");
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

#[derive(Facet, Debug, PartialEq)]
struct UserDoc {
    #[facet(kdl::child)]
    user: User,
}

#[test]
fn test_transparent_newtype() {
    let kdl_input = r#"user id=12345 name="alice""#;
    let doc: UserDoc = from_str(kdl_input).unwrap();
    assert_eq!(doc.user.id, UserId(12345));
    assert_eq!(doc.user.name, "alice");
}

// --- Error case tests ---

#[derive(Facet, Debug, PartialEq)]
struct RequiredFields {
    #[facet(kdl::property)]
    required: String,
    #[facet(kdl::property)]
    also_required: i32,
}

#[derive(Facet, Debug, PartialEq)]
struct RequiredFieldsDoc {
    #[facet(kdl::child)]
    record: RequiredFields,
}

#[test]
fn test_error_missing_required_field() {
    let kdl_input = r#"record required="present""#;
    let result: Result<RequiredFieldsDoc, _> = from_str(kdl_input);
    assert!(result.is_err());
    let err = result.unwrap_err().to_string();
    assert!(err.contains("also_required") || err.contains("missing"));
}

#[derive(Facet, Debug, PartialEq)]
struct TypedValue {
    #[facet(kdl::property)]
    count: i32,
}

#[derive(Facet, Debug, PartialEq)]
struct TypedValueDoc {
    #[facet(kdl::child)]
    record: TypedValue,
}

#[test]
fn test_error_type_mismatch() {
    let kdl_input = r#"record count="not_a_number""#;
    let result: Result<TypedValueDoc, _> = from_str(kdl_input);
    assert!(result.is_err());
}

#[derive(Facet, Debug, PartialEq)]
#[facet(deny_unknown_fields)]
struct StrictRecord {
    #[facet(kdl::property)]
    known: String,
}

#[derive(Facet, Debug, PartialEq)]
struct StrictRecordDoc {
    #[facet(kdl::child)]
    record: StrictRecord,
}

#[test]
fn test_deny_unknown_fields() {
    let kdl_input = r#"record known="value" unknown="bad""#;
    let result: Result<StrictRecordDoc, _> = from_str(kdl_input);
    assert!(result.is_err());
    let err = result.unwrap_err().to_string();
    assert!(err.contains("unknown") || err.contains("Unknown"));
}

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

#[derive(Facet, Debug, PartialEq)]
struct FloatValuesDoc {
    #[facet(kdl::child)]
    floats: FloatValues,
}

#[test]
fn test_float_values() {
    let kdl_input = r#"floats positive=1.5 negative=-2.25 small=0.001"#;
    let doc: FloatValuesDoc = from_str(kdl_input).unwrap();
    assert!((doc.floats.positive - 1.5).abs() < f64::EPSILON);
    assert!((doc.floats.negative - (-2.25)).abs() < f64::EPSILON);
    assert!((doc.floats.small - 0.001).abs() < f64::EPSILON);
}

#[test]
fn test_scientific_notation() {
    let kdl_input = r#"floats positive=1.5e10 negative=-2.25e-5 small=5e3"#;
    let doc: FloatValuesDoc = from_str(kdl_input).unwrap();
    assert!((doc.floats.positive - 1.5e10).abs() < 1e5);
    assert!((doc.floats.negative - (-2.25e-5)).abs() < 1e-10);
    assert!((doc.floats.small - 5e3).abs() < f64::EPSILON);
}

// --- String escape tests ---

#[derive(Facet, Debug, PartialEq)]
struct EscapedString {
    #[facet(kdl::property)]
    text: String,
}

#[derive(Facet, Debug, PartialEq)]
struct EscapedStringDoc {
    #[facet(kdl::child)]
    record: EscapedString,
}

#[test]
fn test_string_escapes() {
    let kdl_input = r#"record text="line1\nline2\ttab""#;
    let doc: EscapedStringDoc = from_str(kdl_input).unwrap();
    assert_eq!(doc.record.text, "line1\nline2\ttab");
}

#[test]
fn test_string_quote_escape() {
    let kdl_input = r#"record text="say \"hello\"""#;
    let doc: EscapedStringDoc = from_str(kdl_input).unwrap();
    assert_eq!(doc.record.text, "say \"hello\"");
}

// --- Alias attribute ---

#[derive(Facet, Debug, PartialEq)]
struct AliasedField {
    #[facet(kdl::property, alias = "old_name")]
    new_name: String,
}

#[derive(Facet, Debug, PartialEq)]
struct AliasedFieldDoc {
    #[facet(kdl::child)]
    record: AliasedField,
}

#[test]
fn test_alias_uses_old_name() {
    let kdl_input = r#"record old_name="value""#;
    let doc: AliasedFieldDoc = from_str(kdl_input).unwrap();
    assert_eq!(doc.record.new_name, "value");
}

#[test]
fn test_alias_uses_new_name() {
    let kdl_input = r#"record new_name="value""#;
    let doc: AliasedFieldDoc = from_str(kdl_input).unwrap();
    assert_eq!(doc.record.new_name, "value");
}
