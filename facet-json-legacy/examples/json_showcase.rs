//! Comprehensive facet-json Showcase
//!
//! This example demonstrates both serialization and error handling capabilities
//! of facet-json using the modern facet-showcase infrastructure with
//! syntax-highlighted output and rich error diagnostics.
//!
//! Run with: cargo run --example json_showcase

use facet::Facet;
use facet_json_legacy::from_str;
use facet_showcase::{Language, ShowcaseRunner};
use std::collections::HashMap;

// Type definitions for the showcase

#[derive(Facet)]
struct Person {
    name: String,
    age: u32,
    email: Option<String>,
}

#[derive(Facet)]
struct Address {
    street: String,
    city: String,
}

#[derive(Facet)]
struct Company {
    name: String,
    address: Address,
    employees: Vec<String>,
}

#[derive(Facet)]
#[repr(u8)]
#[allow(dead_code)]
enum Message {
    Text(String),
    Image { url: String, width: u32 },
    Ping,
}

#[derive(Facet)]
#[repr(C)]
#[facet(tag = "type")]
#[allow(dead_code)]
enum ApiResponse {
    Success { data: String },
    Error { code: i32, message: String },
}

#[derive(Facet)]
#[repr(C)]
#[facet(tag = "t", content = "c")]
#[allow(dead_code)]
enum Event {
    Click { x: i32, y: i32 },
    KeyPress(char),
    Resize,
}

#[derive(Facet)]
#[repr(u8)]
#[facet(untagged)]
#[allow(dead_code)]
enum StringOrNumber {
    Str(String),
    Num(i64),
}

#[derive(Facet)]
struct Point(i32, i32, i32);

#[derive(Facet)]
struct Config {
    debug: bool,
    max_connections: u32,
    endpoints: Vec<String>,
}

fn main() {
    let mut runner = ShowcaseRunner::new("JSON").language(Language::Json);

    runner.header();
    runner.intro("[`facet-json`](https://docs.rs/facet-json) provides JSON serialization and deserialization for any type that implements `Facet`. It includes rich error diagnostics with source locations and typo suggestions.");

    // =========================================================================
    // PART 1: Serialization Examples
    // =========================================================================
    //
    // This section demonstrates facet-json's serialization capabilities,
    // showing how various Rust types are converted to JSON format.
    // =========================================================================

    showcase_basic_struct(&mut runner);
    showcase_nested_structs(&mut runner);
    showcase_externally_tagged_enum(&mut runner);
    showcase_internally_tagged_enum(&mut runner);
    showcase_adjacently_tagged_enum(&mut runner);
    showcase_untagged_enum(&mut runner);
    showcase_maps_string_keys(&mut runner);
    showcase_maps_integer_keys(&mut runner);
    showcase_tuple_struct(&mut runner);
    showcase_pretty_vs_compact(&mut runner);

    // =========================================================================
    // PART 2: Error Handling Examples
    // =========================================================================
    //
    // This section demonstrates facet-json's rich error diagnostics,
    // showcasing how various parsing errors are reported with helpful
    // context and suggestions.
    // =========================================================================

    // -------------------------------------------------------------------------
    // Syntax Errors
    // -------------------------------------------------------------------------
    scenario_syntax_error_unexpected_char(&mut runner);
    scenario_syntax_error_in_context(&mut runner);
    scenario_syntax_error_multiline(&mut runner);

    // -------------------------------------------------------------------------
    // Semantic Errors
    // -------------------------------------------------------------------------
    scenario_unknown_field(&mut runner);
    scenario_type_mismatch(&mut runner);
    scenario_missing_field(&mut runner);
    scenario_number_overflow(&mut runner);
    scenario_wrong_type_for_array(&mut runner);
    scenario_tuple_wrong_size(&mut runner);

    // -------------------------------------------------------------------------
    // Enum Errors
    // -------------------------------------------------------------------------
    scenario_unknown_enum_variant(&mut runner);
    scenario_wrong_variant_format(&mut runner);
    scenario_internally_tagged_missing_tag(&mut runner);

    // -------------------------------------------------------------------------
    // Edge Cases
    // -------------------------------------------------------------------------
    scenario_trailing_data(&mut runner);
    scenario_empty_input(&mut runner);
    scenario_unicode_content(&mut runner);

    runner.footer();
}

fn showcase_basic_struct(runner: &mut ShowcaseRunner) {
    let person = Person {
        name: "Alice".to_string(),
        age: 30,
        email: Some("alice@example.com".to_string()),
    };

    let json_output = facet_json_legacy::to_string_pretty(&person);

    runner
        .scenario("Basic Struct")
        .description("Simple struct with optional field serialized to JSON.")
        .target_type::<Person>()
        .success(&person)
        .serialized_output(Language::Json, &json_output)
        .finish();
}

fn showcase_nested_structs(runner: &mut ShowcaseRunner) {
    let company = Company {
        name: "Acme Corp".to_string(),
        address: Address {
            street: "123 Main St".to_string(),
            city: "Springfield".to_string(),
        },
        employees: vec!["Bob".to_string(), "Carol".to_string(), "Dave".to_string()],
    };

    let json_output = facet_json_legacy::to_string_pretty(&company);

    runner
        .scenario("Nested Structs")
        .description("Struct containing nested struct and vector.")
        .target_type::<Company>()
        .success(&company)
        .serialized_output(Language::Json, &json_output)
        .finish();
}

fn showcase_externally_tagged_enum(runner: &mut ShowcaseRunner) {
    let messages = [
        Message::Text("Hello, world!".to_string()),
        Message::Image {
            url: "https://example.com/cat.jpg".to_string(),
            width: 800,
        },
        Message::Ping,
    ];

    let json_output = facet_json_legacy::to_string_pretty(&messages);

    runner
        .scenario("Externally Tagged Enum (default)")
        .description("Default enum serialization with external tagging: `{\"Variant\": content}`")
        .target_type::<[Message; 3]>()
        .success(&messages)
        .serialized_output(Language::Json, &json_output)
        .finish();
}

fn showcase_internally_tagged_enum(runner: &mut ShowcaseRunner) {
    let responses = [
        ApiResponse::Success {
            data: "Operation completed".to_string(),
        },
        ApiResponse::Error {
            code: 404,
            message: "Not found".to_string(),
        },
    ];

    let json_output = facet_json_legacy::to_string_pretty(&responses);

    runner
        .scenario("Internally Tagged Enum")
        .description("Enum with internal tagging using `#[facet(tag = \"type\")]` - variant name becomes a field.")
        .target_type::<[ApiResponse; 2]>()
        .success(&responses)
        .serialized_output(Language::Json, &json_output)
        .finish();
}

fn showcase_adjacently_tagged_enum(runner: &mut ShowcaseRunner) {
    let events = [
        Event::Click { x: 100, y: 200 },
        Event::KeyPress('A'),
        Event::Resize,
    ];

    let json_output = facet_json_legacy::to_string_pretty(&events);

    runner
        .scenario("Adjacently Tagged Enum")
        .description("Enum with adjacent tagging using `#[facet(tag = \"t\", content = \"c\")]` - variant name and content are separate fields.")
        .target_type::<[Event; 3]>()
        .success(&events)
        .serialized_output(Language::Json, &json_output)
        .finish();
}

fn showcase_untagged_enum(runner: &mut ShowcaseRunner) {
    let values = [
        StringOrNumber::Str("hello".to_string()),
        StringOrNumber::Num(42),
    ];

    let json_output = facet_json_legacy::to_string_pretty(&values);

    runner
        .scenario("Untagged Enum")
        .description("Enum with `#[facet(untagged)]` - no tagging, relies on JSON structure to determine variant.")
        .target_type::<[StringOrNumber; 2]>()
        .success(&values)
        .serialized_output(Language::Json, &json_output)
        .finish();
}

fn showcase_maps_string_keys(runner: &mut ShowcaseRunner) {
    let mut string_map = HashMap::new();
    string_map.insert("one".to_string(), 1);
    string_map.insert("two".to_string(), 2);

    let json_output = facet_json_legacy::to_string_pretty(&string_map);

    runner
        .scenario("Maps with String Keys")
        .description("HashMap with string keys serializes to JSON object.")
        .target_type::<HashMap<String, i32>>()
        .success(&string_map)
        .serialized_output(Language::Json, &json_output)
        .finish();
}

fn showcase_maps_integer_keys(runner: &mut ShowcaseRunner) {
    let mut int_map = HashMap::new();
    int_map.insert(1, "one".to_string());
    int_map.insert(2, "two".to_string());

    let json_output = facet_json_legacy::to_string_pretty(&int_map);

    runner
        .scenario("Maps with Integer Keys")
        .description("HashMap with integer keys - keys are stringified for JSON compatibility.")
        .target_type::<HashMap<i32, String>>()
        .success(&int_map)
        .serialized_output(Language::Json, &json_output)
        .finish();
}

fn showcase_tuple_struct(runner: &mut ShowcaseRunner) {
    let point = Point(10, 20, 30);
    let json_output = facet_json_legacy::to_string_pretty(&point);

    runner
        .scenario("Tuple Struct")
        .description("Tuple struct serializes as JSON array.")
        .target_type::<Point>()
        .success(&point)
        .serialized_output(Language::Json, &json_output)
        .finish();
}

// ============================================================================
// PART 2: Error Showcase Functions
// ============================================================================

fn scenario_syntax_error_unexpected_char(runner: &mut ShowcaseRunner) {
    let json = r#"@invalid"#;

    let result: Result<i32, _> = from_str(json);

    runner
        .scenario("Syntax Error: Unexpected Character")
        .description("Invalid character at the start of JSON input.")
        .input(Language::Json, json)
        .target_type::<i32>()
        .result(&result)
        .finish();
}

fn scenario_syntax_error_in_context(runner: &mut ShowcaseRunner) {
    #[derive(Facet, Debug)]
    struct Data {
        name: String,
        value: i32,
    }

    let json = r#"{"name": "test", "value": @bad}"#;
    let result: Result<Data, _> = from_str(json);

    runner
        .scenario("Syntax Error: Invalid Character in Object")
        .description("Invalid character appears mid-parse with surrounding context visible.")
        .input(Language::Json, json)
        .target_type::<Data>()
        .result(&result)
        .finish();
}

fn scenario_syntax_error_multiline(runner: &mut ShowcaseRunner) {
    #[derive(Facet, Debug)]
    struct Config {
        name: String,
        count: i32,
        active: bool,
    }

    let json = r#"{
  "name": "test",
  "count": ???,
  "active": true
}"#;

    let result: Result<Config, _> = from_str(json);

    runner
        .scenario("Syntax Error: Multiline JSON")
        .description("Error location is correctly identified in multiline JSON.")
        .input(Language::Json, json)
        .target_type::<Config>()
        .result(&result)
        .finish();
}

fn scenario_unknown_field(runner: &mut ShowcaseRunner) {
    #[derive(Facet, Debug)]
    #[facet(deny_unknown_fields)]
    struct User {
        username: String,
        email: String,
    }

    let json = r#"{"username": "alice", "emial": "alice@example.com"}"#;
    let result: Result<User, _> = from_str(json);

    runner
        .scenario("Unknown Field")
        .description(
            "JSON contains a field that doesn't exist in the target struct.\n\
             The error shows the unknown field and lists valid alternatives.",
        )
        .input(Language::Json, json)
        .target_type::<User>()
        .result(&result)
        .finish();
}

fn scenario_type_mismatch(runner: &mut ShowcaseRunner) {
    #[derive(Facet, Debug)]
    struct Item {
        id: u64,
        name: String,
    }

    let json = r#"{"id": 42, "name": 123}"#;
    let result: Result<Item, _> = from_str(json);

    runner
        .scenario("Type Mismatch")
        .description("JSON value type doesn't match the expected Rust type.")
        .input(Language::Json, json)
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

    let json = r#"{"host": "localhost"}"#;
    let result: Result<ServerConfig, _> = from_str(json);

    runner
        .scenario("Missing Required Field")
        .description("JSON is missing a required field that has no default.")
        .input(Language::Json, json)
        .target_type::<ServerConfig>()
        .result(&result)
        .finish();
}

fn scenario_number_overflow(runner: &mut ShowcaseRunner) {
    #[derive(Facet, Debug)]
    struct Counter {
        count: u32,
    }

    let json = r#"{"count": 999999999999}"#;
    let result: Result<Counter, _> = from_str(json);

    runner
        .scenario("Number Out of Range")
        .description("JSON number is too large for the target integer type.")
        .input(Language::Json, json)
        .target_type::<Counter>()
        .result(&result)
        .finish();
}

fn scenario_wrong_type_for_array(runner: &mut ShowcaseRunner) {
    #[derive(Facet, Debug)]
    struct Container {
        items: Vec<i32>,
    }

    let json = r#"{"items": "not an array"}"#;
    let result: Result<Container, _> = from_str(json);

    runner
        .scenario("Expected Array, Got String")
        .description("JSON has a string where an array was expected.")
        .input(Language::Json, json)
        .target_type::<Container>()
        .result(&result)
        .finish();
}

fn scenario_tuple_wrong_size(runner: &mut ShowcaseRunner) {
    let json = r#"[1, 2, 3]"#;
    let result: Result<(i32, i32), _> = from_str(json);

    runner
        .scenario("Tuple Size Mismatch")
        .description("JSON array has wrong number of elements for tuple type.")
        .input(Language::Json, json)
        .target_type::<(i32, i32)>()
        .result(&result)
        .finish();
}

fn scenario_unknown_enum_variant(runner: &mut ShowcaseRunner) {
    #[derive(Facet, Debug)]
    #[repr(u8)]
    #[allow(dead_code)]
    enum Status {
        Active,
        Inactive,
        Pending,
    }

    let json = r#""Unknown""#;
    let result: Result<Status, _> = from_str(json);

    runner
        .scenario("Unknown Enum Variant")
        .description("JSON specifies a variant name that doesn't exist.")
        .input(Language::Json, json)
        .target_type::<Status>()
        .result(&result)
        .finish();
}

fn scenario_wrong_variant_format(runner: &mut ShowcaseRunner) {
    #[derive(Facet, Debug)]
    #[repr(u8)]
    #[allow(dead_code)]
    enum MessageError {
        Text(String),
        Number(i32),
    }

    let json = r#"{"type": "Text", "content": "hello"}"#;
    let result: Result<MessageError, _> = from_str(json);

    runner
        .scenario("Wrong Variant Format")
        .description("Externally tagged enum expects {\"Variant\": content} but got wrong format.")
        .input(Language::Json, json)
        .target_type::<MessageError>()
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

    let json = r#"{"id": "123", "method": "ping"}"#;
    let result: Result<Request, _> = from_str(json);

    runner
        .scenario("Internally Tagged Enum: Missing Tag Field")
        .description("Internally tagged enum requires the tag field to be present.")
        .input(Language::Json, json)
        .target_type::<Request>()
        .result(&result)
        .finish();
}

fn scenario_trailing_data(runner: &mut ShowcaseRunner) {
    let json = r#"42 extra stuff"#;
    let result: Result<i32, _> = from_str(json);

    runner
        .scenario("Trailing Data After Valid JSON")
        .description("Valid JSON followed by unexpected extra content.")
        .input(Language::Json, json)
        .target_type::<i32>()
        .result(&result)
        .finish();
}

fn scenario_empty_input(runner: &mut ShowcaseRunner) {
    let json = r#""#;
    let result: Result<i32, _> = from_str(json);

    runner
        .scenario("Empty Input")
        .description("No JSON content at all.")
        .input(Language::Json, json)
        .target_type::<i32>()
        .result(&result)
        .finish();
}

fn scenario_unicode_content(runner: &mut ShowcaseRunner) {
    #[derive(Facet, Debug)]
    struct EmojiData {
        emoji: String,
        count: i32,
    }

    let json = r#"{"emoji": "🎉🚀", "count": nope}"#;
    let result: Result<EmojiData, _> = from_str(json);

    runner
        .scenario("Error with Unicode Content")
        .description("Error reporting handles unicode correctly.")
        .input(Language::Json, json)
        .target_type::<EmojiData>()
        .result(&result)
        .finish();
}

fn showcase_pretty_vs_compact(runner: &mut ShowcaseRunner) {
    let config = Config {
        debug: true,
        max_connections: 100,
        endpoints: vec![
            "https://api1.example.com".to_string(),
            "https://api2.example.com".to_string(),
        ],
    };

    let compact_json = facet_json_legacy::to_string(&config);
    let pretty_json = facet_json_legacy::to_string_pretty(&config);

    // Compact version
    runner
        .scenario("Compact JSON Output")
        .description("Compact serialization - all on one line, minimal whitespace.")
        .target_type::<Config>()
        .success(&config)
        .serialized_output(Language::Json, &compact_json)
        .finish();

    // Pretty version
    runner
        .scenario("Pretty JSON Output")
        .description("Pretty-printed serialization - formatted with indentation and newlines.")
        .target_type::<Config>()
        .success(&config)
        .serialized_output(Language::Json, &pretty_json)
        .finish();
}
