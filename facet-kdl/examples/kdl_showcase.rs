//! facet-kdl Showcase
//!
//! This example demonstrates facet-kdl's capabilities for KDL document parsing,
//! including rich error diagnostics for both syntax errors and schema mismatches.
//!
//! Run with: cargo run -p facet-kdl --example kdl_showcase

use facet::Facet;
use facet_kdl as kdl;
use facet_showcase::{Language, ShowcaseRunner};

// =============================================================================
// Type Definitions
// =============================================================================

/// A simple server configuration.
#[derive(Facet, Debug)]
struct ServerConfig {
    #[facet(kdl::child)]
    server: Server,
}

#[derive(Facet, Debug)]
struct Server {
    #[facet(kdl::argument)]
    host: String,
    #[facet(kdl::property)]
    port: u16,
}

/// Configuration with child nodes.
#[derive(Facet, Debug)]
struct DatabaseConfig {
    #[facet(kdl::child)]
    database: Database,
}

#[derive(Facet, Debug)]
struct Database {
    #[facet(kdl::property)]
    url: String,
    #[facet(kdl::property, default)]
    pool_size: Option<u32>,
}

/// Configuration expecting multiple children.
#[derive(Facet, Debug)]
struct UsersConfig {
    #[facet(kdl::children)]
    users: Vec<User>,
}

#[derive(Facet, Debug)]
struct User {
    #[facet(kdl::argument)]
    name: String,
    #[facet(kdl::property, default)]
    admin: Option<bool>,
}

/// Wrapper for args (like in dodeca/rapace config).
#[derive(Facet, Debug, Default)]
#[facet(traits(Default))]
struct ArgsNode {
    #[facet(kdl::arguments, default)]
    values: Vec<String>,
}

/// Rust config similar to dodeca's RustConfig.
#[derive(Facet, Debug)]
struct RustConfigWrapper {
    #[facet(kdl::child)]
    rust: RustConfig,
}

#[derive(Facet, Debug, Default)]
#[facet(traits(Default))]
struct RustConfig {
    #[facet(kdl::property, default)]
    command: Option<String>,
    // This is the problematic one - using property for what should be a child node
    #[facet(kdl::property, default)]
    args: Option<Vec<String>>,
}

/// Fixed version using child node pattern.
#[derive(Facet, Debug)]
struct RustConfigFixedWrapper {
    #[facet(kdl::child)]
    rust: RustConfigFixed,
}

#[derive(Facet, Debug, Default)]
#[facet(traits(Default))]
struct RustConfigFixed {
    #[facet(kdl::child, default)]
    command: Option<CommandNode>,
    #[facet(kdl::child, default)]
    args: Option<ArgsNode>,
}

#[derive(Facet, Debug)]
struct CommandNode {
    #[facet(kdl::argument)]
    value: String,
}

// =============================================================================
// Types for Document Model examples
// =============================================================================

/// A simple configuration that can be roundtripped.
/// This struct is the "document struct" - its fields become root-level nodes.
#[derive(Facet, Debug, PartialEq)]
struct AppConfig {
    host: String,
    port: u16,
}

/// A more complex config showing nested structures.
#[derive(Facet, Debug, PartialEq)]
struct ComplexConfig {
    #[facet(kdl::child)]
    server: ServerNode,
    #[facet(kdl::child)]
    database: DatabaseNode,
}

#[derive(Facet, Debug, PartialEq)]
struct ServerNode {
    #[facet(kdl::argument)]
    host: String,
    #[facet(kdl::property)]
    port: u16,
}

#[derive(Facet, Debug, PartialEq)]
struct DatabaseNode {
    #[facet(kdl::property)]
    url: String,
}

/// Document with multiple root nodes of the same type.
#[derive(Facet, Debug, PartialEq)]
struct RoutesConfig {
    #[facet(kdl::children)]
    routes: Vec<Route>,
}

#[derive(Facet, Debug, PartialEq)]
struct Route {
    #[facet(kdl::argument)]
    path: String,
    #[facet(kdl::property)]
    handler: String,
}

fn main() {
    let mut runner = ShowcaseRunner::new("KDL").language(Language::Kdl);

    runner.header();
    runner.intro(r#"[`facet-kdl`](https://docs.rs/facet-kdl) parses KDL documents into Rust types using `Facet` attributes. Map KDL arguments with `kdl::argument`, properties with `kdl::property`, and child nodes with `kdl::child` or `kdl::children`.

## KDL Data Model

**KDL is not XML.** Unlike XML which has exactly one root element, KDL documents can have **multiple root nodes**. This is a fundamental difference that affects how you design your Rust types.

### The Transparent Document Model

When you serialize or deserialize with facet-kdl, the outermost Rust struct is the **document struct**. It's transparent - it doesn't appear in the KDL output. Instead, its fields become root-level nodes.

For example, if you have:
```rust
struct Config {
    host: String,
    port: u16,
}
```

Serializing it produces:
```kdl
host "localhost"
port 8080
```

Each field becomes its own root node. The struct name `Config` doesn't appear anywhere.

### Why Fields Default to Child Nodes

At the document level, there's no node to attach properties to. You can't write:
```kdl
host="localhost" port=8080  // Invalid! Properties need a node name
```

So fields without explicit `kdl::*` attributes default to being child nodes. If you want properties, you need a wrapper node."#);

    // =========================================================================
    // PART 1: Document Model
    // =========================================================================
    runner.section("Document Model");

    showcase_transparent_document(&mut runner);
    showcase_multiple_roots(&mut runner);
    showcase_roundtrip(&mut runner);

    // =========================================================================
    // PART 2: Successful Parsing
    // =========================================================================
    runner.section("Successful Parsing");

    showcase_simple_node(&mut runner);
    showcase_properties(&mut runner);
    showcase_children(&mut runner);

    // =========================================================================
    // PART 3: KDL Syntax Errors
    // =========================================================================
    runner.section("KDL Syntax Errors");

    error_invalid_syntax(&mut runner);
    error_unclosed_brace(&mut runner);
    error_invalid_number(&mut runner);

    // =========================================================================
    // PART 4: Schema Mismatch Errors
    // =========================================================================
    runner.section("Schema Mismatch Errors");

    error_expected_scalar_got_struct(&mut runner);
    error_missing_required_field(&mut runner);
    error_wrong_type(&mut runner);

    runner.footer();
}

// =============================================================================
// Document Model Scenarios
// =============================================================================

fn showcase_transparent_document(runner: &mut ShowcaseRunner) {
    // Deserialize a document where each root node is a field
    let input = r#"host "https://example.com"
port 443"#;
    let result = kdl::from_str::<AppConfig>(input);

    runner
        .scenario("Transparent Document Struct")
        .description("The outermost struct is the 'document struct'. It's transparent - its fields become root-level KDL nodes. Each field without explicit `kdl::*` attributes defaults to a child node.")
        .target_type::<AppConfig>()
        .input(Language::Kdl, input)
        .result(&result)
        .finish();
}

fn showcase_multiple_roots(runner: &mut ShowcaseRunner) {
    let input = r#"route "/api/users" handler="users_handler"
route "/api/posts" handler="posts_handler"
route "/health" handler="health_check""#;
    let result = kdl::from_str::<RoutesConfig>(input);

    runner
        .scenario("Multiple Root Nodes")
        .description("Unlike XML, KDL allows multiple root nodes. Use `kdl::children` to collect them into a Vec. The field name's singular form (`route` for `routes`) matches the node names.")
        .target_type::<RoutesConfig>()
        .input(Language::Kdl, input)
        .result(&result)
        .finish();
}

fn showcase_roundtrip(runner: &mut ShowcaseRunner) {
    let original = AppConfig {
        host: "https://api.example.com".to_string(),
        port: 8080,
    };

    let serialized = kdl::to_string(&original).unwrap();
    let deserialized: AppConfig = kdl::from_str(&serialized).unwrap();

    // Show the serialization
    runner
        .scenario("Roundtrip Serialization")
        .description("Serialization and deserialization roundtrip correctly. The document struct is transparent - `AppConfig` doesn't appear in the output, only its fields as root nodes.")
        .target_type::<AppConfig>()
        .result(&Ok::<_, kdl::KdlDeserializeError>(deserialized))
        .serialized_output(Language::Kdl, &serialized)
        .finish();
}

// =============================================================================
// Successful Parsing Scenarios
// =============================================================================

fn showcase_simple_node(runner: &mut ShowcaseRunner) {
    let input = r#"server "localhost" port=8080"#;
    let result = kdl::from_str::<ServerConfig>(input);

    runner
        .scenario("Simple Node with Argument and Property")
        .description("Parse a node with a positional argument and a property.")
        .target_type::<ServerConfig>()
        .input(Language::Kdl, input)
        .result(&result)
        .finish();
}

fn showcase_properties(runner: &mut ShowcaseRunner) {
    let input = r#"database url="postgres://localhost/mydb" pool_size=10"#;
    let result = kdl::from_str::<DatabaseConfig>(input);

    runner
        .scenario("Node with Properties")
        .description("Parse a node with multiple key=value properties.")
        .target_type::<DatabaseConfig>()
        .input(Language::Kdl, input)
        .result(&result)
        .finish();
}

fn showcase_children(runner: &mut ShowcaseRunner) {
    let input = r#"
user "alice" admin=#true
user "bob"
user "charlie" admin=#false
"#;
    let result = kdl::from_str::<UsersConfig>(input);

    runner
        .scenario("Multiple Child Nodes")
        .description("Parse multiple nodes of the same type into a Vec.")
        .target_type::<UsersConfig>()
        .input(Language::Kdl, input)
        .result(&result)
        .finish();
}

// =============================================================================
// KDL Syntax Error Scenarios
// =============================================================================

fn error_invalid_syntax(runner: &mut ShowcaseRunner) {
    // Unclosed string literal
    let input = r#"server "localhost port=8080"#;
    let result = kdl::from_str::<ServerConfig>(input);

    runner
        .scenario("Unclosed String")
        .description("KDL syntax error when a string literal is not closed.")
        .target_type::<ServerConfig>()
        .input(Language::Kdl, input)
        .result(&result)
        .finish();
}

fn error_unclosed_brace(runner: &mut ShowcaseRunner) {
    let input = r#"
parent {
    child "value"
"#;
    let result = kdl::from_str::<ServerConfig>(input);

    runner
        .scenario("Unclosed Brace")
        .description("KDL syntax error when a children block is not closed.")
        .target_type::<ServerConfig>()
        .input(Language::Kdl, input)
        .result(&result)
        .finish();
}

fn error_invalid_number(runner: &mut ShowcaseRunner) {
    let input = r#"server "localhost" port=808O"#; // Note: letter O instead of zero
    let result = kdl::from_str::<ServerConfig>(input);

    runner
        .scenario("Invalid Number")
        .description("Error when a property value looks like a number but isn't valid.")
        .target_type::<ServerConfig>()
        .input(Language::Kdl, input)
        .result(&result)
        .finish();
}

// =============================================================================
// Schema Mismatch Error Scenarios
// =============================================================================

fn error_expected_scalar_got_struct(runner: &mut ShowcaseRunner) {
    // This is the exact error from dodeca/rapace:
    // The config uses `args "run" "--quiet"` (a child node with arguments)
    // but RustConfig expects `args=["run", "--quiet"]` (a property - which isn't valid KDL anyway!)
    let input = r#"
rust {
    command "cargo"
    args "run" "--quiet" "--release"
}
"#;
    let result = kdl::from_str::<RustConfigWrapper>(input);

    runner
        .scenario("Expected Scalar, Got Struct")
        .description("Error when a field expects a scalar value but receives a child node. This happens when using `kdl::property` for what should be `kdl::child`.")
        .target_type::<RustConfigWrapper>()
        .input(Language::Kdl, input)
        .result(&result)
        .finish();
}

fn error_missing_required_field(runner: &mut ShowcaseRunner) {
    // Server expects 'host' argument but we don't provide it
    let input = r#"server port=8080"#;
    let result = kdl::from_str::<ServerConfig>(input);

    runner
        .scenario("Missing Required Field")
        .description("Error when a required field (without `default`) is not provided.")
        .target_type::<ServerConfig>()
        .input(Language::Kdl, input)
        .result(&result)
        .finish();
}

fn error_wrong_type(runner: &mut ShowcaseRunner) {
    // port expects u16 but we give a string
    let input = r#"server "localhost" port="not-a-number""#;
    let result = kdl::from_str::<ServerConfig>(input);

    runner
        .scenario("Wrong Value Type")
        .description("Error when a property value cannot be parsed as the expected type.")
        .target_type::<ServerConfig>()
        .input(Language::Kdl, input)
        .result(&result)
        .finish();
}
