//! Error Showcase: Demonstrating facet-json error diagnostics
//!
//! This example showcases the rich error reporting capabilities of facet-json
//! with miette's beautiful diagnostic output.
//!
//! Run with: cargo run --example json_error_showcase

use facet::Facet;
use facet_json::from_str;
use facet_showcase::{Language, ShowcaseRunner};

fn main() {
    let mut runner = ShowcaseRunner::new("facet-json Error Showcase").language(Language::Json);
    runner.header();

    // =========================================================================
    // Syntax Errors
    // =========================================================================

    scenario_syntax_error_unexpected_char(&mut runner);
    scenario_syntax_error_in_context(&mut runner);
    scenario_syntax_error_multiline(&mut runner);

    // =========================================================================
    // Semantic Errors
    // =========================================================================

    scenario_unknown_field(&mut runner);
    scenario_type_mismatch(&mut runner);
    scenario_missing_field(&mut runner);
    scenario_number_overflow(&mut runner);
    scenario_wrong_type_for_array(&mut runner);
    scenario_tuple_wrong_size(&mut runner);

    // =========================================================================
    // Enum Errors
    // =========================================================================

    scenario_unknown_enum_variant(&mut runner);
    scenario_wrong_variant_format(&mut runner);
    scenario_internally_tagged_missing_tag(&mut runner);

    // =========================================================================
    // Edge Cases
    // =========================================================================

    scenario_trailing_data(&mut runner);
    scenario_empty_input(&mut runner);
    scenario_unicode_content(&mut runner);

    runner.footer();
}

// ============================================================================
// Syntax Errors
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
    enum Message {
        Text(String),
        Number(i32),
    }

    let json = r#"{"type": "Text", "content": "hello"}"#;
    let result: Result<Message, _> = from_str(json);

    runner
        .scenario("Wrong Variant Format")
        .description("Externally tagged enum expects {\"Variant\": content} but got wrong format.")
        .input(Language::Json, json)
        .target_type::<Message>()
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

// ============================================================================
// Edge Cases
// ============================================================================

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
