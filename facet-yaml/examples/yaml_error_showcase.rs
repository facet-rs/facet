//! Error Showcase: Demonstrating facet-yaml error diagnostics
//!
//! This example showcases the rich error reporting capabilities of facet-yaml
//! with miette's beautiful diagnostic output.
//!
//! Run with: cargo run --example yaml_error_showcase

use facet::Facet;
use facet_showcase::{Language, ShowcaseRunner};
use facet_yaml::from_str;

fn main() {
    let mut runner = ShowcaseRunner::new("facet-yaml Error Showcase").language(Language::Yaml);
    runner.header();

    // =========================================================================
    // Syntax Errors
    // =========================================================================

    scenario_syntax_error_bad_indentation(&mut runner);
    scenario_syntax_error_invalid_character(&mut runner);
    scenario_syntax_error_unclosed_quote(&mut runner);

    // =========================================================================
    // Semantic Errors
    // =========================================================================

    scenario_unknown_field(&mut runner);
    scenario_type_mismatch_string_for_int(&mut runner);
    scenario_type_mismatch_int_for_string(&mut runner);
    scenario_missing_field(&mut runner);
    scenario_number_overflow(&mut runner);
    scenario_wrong_type_for_sequence(&mut runner);
    scenario_wrong_type_for_mapping(&mut runner);

    // =========================================================================
    // Enum Errors
    // =========================================================================

    scenario_unknown_enum_variant(&mut runner);
    scenario_enum_wrong_format(&mut runner);
    scenario_internally_tagged_missing_tag(&mut runner);

    // =========================================================================
    // YAML-Specific Features
    // =========================================================================

    scenario_duplicate_key(&mut runner);
    scenario_anchor_reference(&mut runner);
    scenario_multiline_string(&mut runner);

    // =========================================================================
    // Edge Cases
    // =========================================================================

    scenario_empty_input(&mut runner);
    scenario_null_for_required(&mut runner);
    scenario_unicode_content(&mut runner);
    scenario_nested_error(&mut runner);
    scenario_sequence_item_error(&mut runner);

    runner.footer();
}

// ============================================================================
// Syntax Errors
// ============================================================================

fn scenario_syntax_error_bad_indentation(runner: &mut ShowcaseRunner) {
    #[derive(Facet, Debug)]
    struct Config {
        name: String,
    }

    let yaml = r#"name: test
  nested: value
 wrong: indent"#;

    let result: Result<Config, _> = from_str(yaml);

    runner
        .scenario("Syntax Error: Bad Indentation")
        .description("YAML indentation is inconsistent or invalid.")
        .input(Language::Yaml, yaml)
        .target_type::<Config>()
        .result(&result)
        .finish();
}

fn scenario_syntax_error_invalid_character(runner: &mut ShowcaseRunner) {
    #[derive(Facet, Debug)]
    struct Config {
        name: String,
    }

    let yaml = r#"name: @invalid"#;
    let result: Result<Config, _> = from_str(yaml);

    runner
        .scenario("Syntax Error: Invalid Character")
        .description("YAML contains an invalid character in an unexpected location.")
        .input(Language::Yaml, yaml)
        .target_type::<Config>()
        .result(&result)
        .finish();
}

fn scenario_syntax_error_unclosed_quote(runner: &mut ShowcaseRunner) {
    #[derive(Facet, Debug)]
    struct Config {
        message: String,
        name: String,
    }

    let yaml = r#"message: "hello world
name: test"#;
    let result: Result<Config, _> = from_str(yaml);

    runner
        .scenario("Syntax Error: Unclosed Quote")
        .description("String value has an opening quote but no closing quote.")
        .input(Language::Yaml, yaml)
        .target_type::<Config>()
        .result(&result)
        .finish();
}

// ============================================================================
// Semantic Errors
// ============================================================================

fn scenario_unknown_field(runner: &mut ShowcaseRunner) {
    #[derive(Facet, Debug)]
    #[facet(deny_unknown_fields)]
    struct User {
        username: String,
        email: String,
    }

    let yaml = r#"username: alice
emial: alice@example.com"#;
    let result: Result<User, _> = from_str(yaml);

    runner
        .scenario("Unknown Field")
        .description(
            "YAML contains a field that doesn't exist in the target struct.\n\
             The error shows the unknown field and lists valid alternatives.",
        )
        .input(Language::Yaml, yaml)
        .target_type::<User>()
        .result(&result)
        .finish();
}

fn scenario_type_mismatch_string_for_int(runner: &mut ShowcaseRunner) {
    #[derive(Facet, Debug)]
    struct Item {
        id: u64,
        count: i32,
    }

    let yaml = r#"id: 42
count: "not a number""#;
    let result: Result<Item, _> = from_str(yaml);

    runner
        .scenario("Type Mismatch: String for Integer")
        .description("YAML value is a string where an integer was expected.")
        .input(Language::Yaml, yaml)
        .target_type::<Item>()
        .result(&result)
        .finish();
}

fn scenario_type_mismatch_int_for_string(runner: &mut ShowcaseRunner) {
    #[derive(Facet, Debug)]
    struct Item {
        id: u64,
        name: String,
    }

    let yaml = r#"id: 42
name: 123"#;
    let result: Result<Item, _> = from_str(yaml);

    runner
        .scenario("Type Mismatch: Integer for String")
        .description(
            "YAML value is an integer where a string was expected (may succeed with coercion).",
        )
        .input(Language::Yaml, yaml)
        .target_type::<Item>()
        .result(&result)
        .finish();
}

fn scenario_missing_field(runner: &mut ShowcaseRunner) {
    #[derive(Facet, Debug)]
    struct ServerConfig {
        host: String,
        port: u16,
    }

    let yaml = r#"host: localhost"#;
    let result: Result<ServerConfig, _> = from_str(yaml);

    runner
        .scenario("Missing Required Field")
        .description("YAML is missing a required field that has no default.")
        .input(Language::Yaml, yaml)
        .target_type_str(
            r#"#[derive(Facet)]
struct ServerConfig {
    host: String,
    port: u16,  // Required but missing from YAML
}"#,
        )
        .result(&result)
        .finish();
}

fn scenario_number_overflow(runner: &mut ShowcaseRunner) {
    #[derive(Facet, Debug)]
    struct Counter {
        count: u32,
    }

    let yaml = r#"count: 999999999999"#;
    let result: Result<Counter, _> = from_str(yaml);

    runner
        .scenario("Number Out of Range")
        .description("YAML number is too large for the target integer type.")
        .input(Language::Yaml, yaml)
        .target_type_str(
            r#"#[derive(Facet)]
struct Counter {
    count: u32,  // Max value is 4,294,967,295
}"#,
        )
        .result(&result)
        .finish();
}

fn scenario_wrong_type_for_sequence(runner: &mut ShowcaseRunner) {
    #[derive(Facet, Debug)]
    struct Container {
        items: Vec<i32>,
    }

    let yaml = r#"items: "not a sequence""#;
    let result: Result<Container, _> = from_str(yaml);

    runner
        .scenario("Expected Sequence, Got Scalar")
        .description("YAML has a scalar where a sequence was expected.")
        .input(Language::Yaml, yaml)
        .target_type_str(
            r#"#[derive(Facet)]
struct Container {
    items: Vec<i32>,  // Expected sequence, got string
}"#,
        )
        .result(&result)
        .finish();
}

fn scenario_wrong_type_for_mapping(runner: &mut ShowcaseRunner) {
    #[derive(Facet, Debug)]
    struct Nested {
        value: i32,
    }

    #[derive(Facet, Debug)]
    struct Outer {
        config: Nested,
    }

    let yaml = r#"config: "not a mapping""#;
    let result: Result<Outer, _> = from_str(yaml);

    runner
        .scenario("Expected Mapping, Got Scalar")
        .description("YAML has a scalar where a mapping was expected.")
        .input(Language::Yaml, yaml)
        .target_type_str(
            r#"#[derive(Facet)]
struct Nested {
    value: i32,
}

#[derive(Facet)]
struct Outer {
    config: Nested,  // Expected mapping, got string
}"#,
        )
        .result(&result)
        .finish();
}

// ============================================================================
// Enum Errors
// ============================================================================

fn scenario_unknown_enum_variant(runner: &mut ShowcaseRunner) {
    #[derive(Facet, Debug)]
    #[repr(u8)]
    #[allow(dead_code)]
    enum Status {
        Active,
        Inactive,
        Pending,
    }

    let yaml = r#"Unknown"#;
    let result: Result<Status, _> = from_str(yaml);

    runner
        .scenario("Unknown Enum Variant")
        .description("YAML specifies a variant name that doesn't exist.")
        .input(Language::Yaml, yaml)
        .target_type_str(
            r#"#[derive(Facet)]
#[repr(u8)]
enum Status {
    Active,
    Inactive,
    Pending,
}
// YAML has "Unknown" which is not a valid variant"#,
        )
        .result(&result)
        .finish();
}

fn scenario_enum_wrong_format(runner: &mut ShowcaseRunner) {
    #[derive(Facet, Debug)]
    #[repr(u8)]
    #[allow(dead_code)]
    enum Message {
        Text(String),
        Number(i32),
    }

    let yaml = r#"type: Text
content: hello"#;
    let result: Result<Message, _> = from_str(yaml);

    runner
        .scenario("Enum Wrong Format")
        .description("Externally tagged enum expects {Variant: content} but got wrong format.")
        .input(Language::Yaml, yaml)
        .target_type_str(
            r#"#[derive(Facet)]
#[repr(u8)]
enum Message {
    Text(String),
    Number(i32),
}
// Externally tagged expects:
//   Text: "hello"
// But YAML has:
//   type: Text
//   content: hello"#,
        )
        .result(&result)
        .finish();
}

fn scenario_internally_tagged_missing_tag(runner: &mut ShowcaseRunner) {
    #[derive(Facet, Debug)]
    #[repr(C)]
    #[facet(tag = "type")]
    #[allow(dead_code)]
    enum Request {
        Ping { id: String },
        Echo { id: String, message: String },
    }

    let yaml = r#"id: "123"
method: ping"#;
    let result: Result<Request, _> = from_str(yaml);

    runner
        .scenario("Internally Tagged Enum: Missing Tag Field")
        .description("Internally tagged enum requires the tag field to be present.")
        .input(Language::Yaml, yaml)
        .target_type_str(
            r#"#[derive(Facet)]
#[repr(C)]
#[facet(tag = "type")]
enum Request {
    Ping { id: String },
    Echo { id: String, message: String },
}
// YAML is missing the "type" tag field"#,
        )
        .result(&result)
        .finish();
}

// ============================================================================
// YAML-Specific Features
// ============================================================================

fn scenario_duplicate_key(runner: &mut ShowcaseRunner) {
    #[derive(Facet, Debug)]
    struct Config {
        name: String,
        value: i32,
    }

    let yaml = r#"name: first
value: 42
name: second"#;
    let result: Result<Config, _> = from_str(yaml);

    runner
        .scenario("Duplicate Key")
        .description("YAML mapping contains the same key more than once.")
        .input(Language::Yaml, yaml)
        .target_type::<Config>()
        .result(&result)
        .finish();
}

fn scenario_anchor_reference(runner: &mut ShowcaseRunner) {
    #[derive(Facet, Debug)]
    struct ServerConfig {
        timeout: u32,
        retries: u32,
        host: String,
    }

    #[derive(Facet, Debug)]
    struct AllConfigs {
        defaults: ServerConfig,
        production: ServerConfig,
        staging: ServerConfig,
    }

    let yaml = r#"defaults: &defaults
  timeout: 30
  retries: 3

production:
  <<: *defaults
  host: prod.example.com

staging:
  <<: *defaults
  host: staging.example.com"#;
    let result: Result<AllConfigs, _> = from_str(yaml);

    runner
        .scenario("Anchors and Aliases")
        .description("YAML anchors and aliases for value reuse.")
        .input(Language::Yaml, yaml)
        .target_type::<AllConfigs>()
        .result(&result)
        .finish();
}

fn scenario_multiline_string(runner: &mut ShowcaseRunner) {
    #[derive(Facet, Debug)]
    struct TextContent {
        literal: String,
        folded: String,
    }

    let yaml = r#"literal: |
  This is a literal block.
  Newlines are preserved.

folded: >
  This is a folded block.
  Lines get folded into
  a single paragraph."#;
    let result: Result<TextContent, _> = from_str(yaml);

    runner
        .scenario("Multiline String Styles")
        .description("YAML supports various multiline string styles.")
        .input(Language::Yaml, yaml)
        .target_type::<TextContent>()
        .result(&result)
        .finish();
}

// ============================================================================
// Edge Cases
// ============================================================================

fn scenario_empty_input(runner: &mut ShowcaseRunner) {
    let yaml = r#""#;
    let result: Result<i32, _> = from_str(yaml);

    runner
        .scenario("Empty Input")
        .description("No YAML content at all.")
        .input(Language::Yaml, yaml)
        .target_type::<i32>()
        .result(&result)
        .finish();
}

fn scenario_null_for_required(runner: &mut ShowcaseRunner) {
    #[derive(Facet, Debug)]
    struct Item {
        name: String,
        count: i32,
    }

    let yaml = r#"name: ~
count: 42"#;
    let result: Result<Item, _> = from_str(yaml);

    runner
        .scenario("Null for Required Field")
        .description("YAML has explicit null where a value is required.")
        .input(Language::Yaml, yaml)
        .target_type::<Item>()
        .result(&result)
        .finish();
}

fn scenario_unicode_content(runner: &mut ShowcaseRunner) {
    #[derive(Facet, Debug)]
    struct EmojiData {
        emoji: String,
        count: i32,
    }

    let yaml = r#"emoji: "🎉🚀"
count: nope"#;
    let result: Result<EmojiData, _> = from_str(yaml);

    runner
        .scenario("Error with Unicode Content")
        .description("Error reporting handles unicode correctly.")
        .input(Language::Yaml, yaml)
        .target_type::<EmojiData>()
        .result(&result)
        .finish();
}

fn scenario_nested_error(runner: &mut ShowcaseRunner) {
    #[derive(Facet, Debug)]
    struct Ports {
        http: u16,
        https: u16,
    }

    #[derive(Facet, Debug)]
    struct Database {
        url: String,
    }

    #[derive(Facet, Debug)]
    struct Server {
        host: String,
        ports: Ports,
        database: Database,
    }

    #[derive(Facet, Debug)]
    struct AppConfig {
        server: Server,
    }

    let yaml = r#"server:
  host: localhost
  ports:
    http: 8080
    https: "not a number"
  database:
    url: postgres://localhost/db"#;
    let result: Result<AppConfig, _> = from_str(yaml);

    runner
        .scenario("Error in Nested Structure")
        .description("Error location is correctly identified in deeply nested YAML.")
        .input(Language::Yaml, yaml)
        .target_type::<AppConfig>()
        .result(&result)
        .finish();
}

fn scenario_sequence_item_error(runner: &mut ShowcaseRunner) {
    #[derive(Facet, Debug)]
    struct User {
        name: String,
        age: u32,
    }

    #[derive(Facet, Debug)]
    struct UserList {
        users: Vec<User>,
    }

    let yaml = r#"users:
  - name: Alice
    age: 30
  - name: Bob
    age: "twenty-five"
  - name: Charlie
    age: 35"#;
    let result: Result<UserList, _> = from_str(yaml);

    runner
        .scenario("Error in Sequence Item")
        .description("Error in one item of a sequence is reported with context.")
        .input(Language::Yaml, yaml)
        .target_type::<UserList>()
        .result(&result)
        .finish();
}
