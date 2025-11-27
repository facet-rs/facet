//! Error Showcase: Demonstrating facet-json error diagnostics
//!
//! This example showcases the rich error reporting capabilities of facet-json
//! with miette's beautiful diagnostic output.
//!
//! Run with: cargo run --example error_showcase

use boxen::{BorderStyle, TextAlignment};
use facet::Facet;
use facet_json::from_str;
use facet_pretty::format_shape;
use miette::{GraphicalReportHandler, GraphicalTheme, highlighters::SyntectHighlighter};
use owo_colors::OwoColorize;
use syntect::{
    easy::HighlightLines,
    highlighting::{Style, ThemeSet},
    parsing::SyntaxSet,
    util::{LinesWithEndings, as_24_bit_terminal_escaped},
};

// ============================================================================
// Helper Functions
// ============================================================================

fn build_json_highlighter() -> SyntectHighlighter {
    let syntax_set = SyntaxSet::load_defaults_newlines();
    let theme_set = ThemeSet::load_defaults();
    let theme = theme_set.themes["base16-ocean.dark"].clone();
    SyntectHighlighter::new(syntax_set, theme, false)
}

fn render_error(err: &dyn miette::Diagnostic) -> String {
    let mut output = String::new();
    let handler = GraphicalReportHandler::new_themed(GraphicalTheme::unicode())
        .with_syntax_highlighting(build_json_highlighter());
    handler.render_report(&mut output, err).unwrap();
    output
}

fn print_scenario(name: &str, description: &str) {
    println!();
    println!("{}", "═".repeat(78).dimmed());
    println!("{} {}", "SCENARIO:".bold().cyan(), name.bold().white());
    println!("{}", "─".repeat(78).dimmed());
    println!("{}", description.dimmed());
    println!("{}", "═".repeat(78).dimmed());
}

fn print_json(json: &str) {
    let ps = SyntaxSet::load_defaults_newlines();
    let ts = ThemeSet::load_defaults();
    let theme = &ts.themes["base16-ocean.dark"];
    let syntax = ps.find_syntax_by_extension("json").unwrap();

    println!();
    println!("{}", "JSON Input:".bold().green());
    println!("{}", "─".repeat(60).dimmed());

    let mut h = HighlightLines::new(syntax, theme);
    for (i, line) in json.lines().enumerate() {
        let line_with_newline = format!("{}\n", line);
        let ranges: Vec<(Style, &str)> = h.highlight_line(&line_with_newline, &ps).unwrap();
        let escaped = as_24_bit_terminal_escaped(&ranges[..], false);
        print!(
            "{} {} {}",
            format!("{:3}", i + 1).dimmed(),
            "│".dimmed(),
            escaped
        );
    }
    print!("\x1b[0m"); // Reset terminal colors
    println!("{}", "─".repeat(60).dimmed());
}

fn print_type_def(type_def: &str) {
    let ps = SyntaxSet::load_defaults_newlines();
    let ts = ThemeSet::load_defaults();
    let theme = &ts.themes["base16-ocean.dark"];
    let syntax = ps.find_syntax_by_extension("rs").unwrap();

    println!();
    println!("{}", "Target Type:".bold().blue());
    println!("{}", "─".repeat(60).dimmed());

    let mut h = HighlightLines::new(syntax, theme);
    for line in LinesWithEndings::from(type_def) {
        let ranges: Vec<(Style, &str)> = h.highlight_line(line, &ps).unwrap();
        let escaped = as_24_bit_terminal_escaped(&ranges[..], false);
        print!("    {}", escaped);
    }
    println!("\x1b[0m"); // Reset terminal colors and add newline
    println!("{}", "─".repeat(60).dimmed());
}

// ============================================================================
// Syntax Errors
// ============================================================================

fn scenario_syntax_error_unexpected_char() {
    print_scenario(
        "Syntax Error: Unexpected Character",
        "Invalid character at the start of JSON input.",
    );

    let json = r#"@invalid"#;
    print_json(json);

    let result: Result<i32, _> = from_str(json);
    match result {
        Ok(_) => println!("Unexpected success!"),
        Err(e) => {
            println!("\n{}", "Error:".bold().red());
            println!("{}", render_error(&e));
        }
    }
}

fn scenario_syntax_error_in_context() {
    print_scenario(
        "Syntax Error: Invalid Character in Object",
        "Invalid character appears mid-parse with surrounding context visible.",
    );

    let json = r#"{"name": "test", "value": @bad}"#;
    print_json(json);

    #[derive(Facet, Debug)]
    struct Data {
        name: String,
        value: i32,
    }

    print_type_def(&format_shape(Data::SHAPE));

    let result: Result<Data, _> = from_str(json);
    match result {
        Ok(_) => println!("Unexpected success!"),
        Err(e) => {
            println!("\n{}", "Error:".bold().red());
            println!("{}", render_error(&e));
        }
    }
}

fn scenario_syntax_error_multiline() {
    print_scenario(
        "Syntax Error: Multiline JSON",
        "Error location is correctly identified in multiline JSON.",
    );

    let json = r#"{
  "name": "test",
  "count": ???,
  "active": true
}"#;
    print_json(json);

    #[derive(Facet, Debug)]
    struct Config {
        name: String,
        count: i32,
        active: bool,
    }

    print_type_def(&format_shape(Config::SHAPE));

    let result: Result<Config, _> = from_str(json);
    match result {
        Ok(_) => println!("Unexpected success!"),
        Err(e) => {
            println!("\n{}", "Error:".bold().red());
            println!("{}", render_error(&e));
        }
    }
}

// ============================================================================
// Semantic Errors
// ============================================================================

fn scenario_unknown_field() {
    print_scenario(
        "Unknown Field",
        "JSON contains a field that doesn't exist in the target struct.\n\
         The error shows the unknown field and lists valid alternatives.",
    );

    let json = r#"{"username": "alice", "emial": "alice@example.com"}"#;
    print_json(json);

    #[derive(Facet, Debug)]
    #[facet(deny_unknown_fields)]
    struct User {
        username: String,
        email: String,
    }

    print_type_def(&format_shape(User::SHAPE));

    let result: Result<User, _> = from_str(json);
    match result {
        Ok(_) => println!("Unexpected success!"),
        Err(e) => {
            println!("\n{}", "Error:".bold().red());
            println!("{}", render_error(&e));
        }
    }
}

fn scenario_type_mismatch() {
    print_scenario(
        "Type Mismatch",
        "JSON value type doesn't match the expected Rust type.",
    );

    let json = r#"{"id": 42, "name": 123}"#;
    print_json(json);

    #[derive(Facet, Debug)]
    struct Item {
        id: u64,
        name: String,
    }

    print_type_def(&format_shape(Item::SHAPE));

    let result: Result<Item, _> = from_str(json);
    match result {
        Ok(_) => println!("Unexpected success!"),
        Err(e) => {
            println!("\n{}", "Error:".bold().red());
            println!("{}", render_error(&e));
        }
    }
}

fn scenario_missing_field() {
    print_scenario(
        "Missing Required Field",
        "JSON is missing a required field that has no default.",
    );

    let json = r#"{"host": "localhost"}"#;
    print_json(json);

    #[derive(Facet, Debug)]
    struct ServerConfig {
        host: String,
        port: u16,
    }

    print_type_def(&format_shape(ServerConfig::SHAPE));

    let result: Result<ServerConfig, _> = from_str(json);
    match result {
        Ok(_) => println!("Unexpected success!"),
        Err(e) => {
            println!("\n{}", "Error:".bold().red());
            println!("{}", render_error(&e));
        }
    }
}

fn scenario_number_overflow() {
    print_scenario(
        "Number Out of Range",
        "JSON number is too large for the target integer type.",
    );

    let json = r#"{"count": 999999999999}"#;
    print_json(json);

    #[derive(Facet, Debug)]
    struct Counter {
        count: u32,
    }

    print_type_def(&format_shape(Counter::SHAPE));

    let result: Result<Counter, _> = from_str(json);
    match result {
        Ok(_) => println!("Unexpected success!"),
        Err(e) => {
            println!("\n{}", "Error:".bold().red());
            println!("{}", render_error(&e));
        }
    }
}

fn scenario_wrong_type_for_array() {
    print_scenario(
        "Expected Array, Got String",
        "JSON has a string where an array was expected.",
    );

    let json = r#"{"items": "not an array"}"#;
    print_json(json);

    #[derive(Facet, Debug)]
    struct Container {
        items: Vec<i32>,
    }

    print_type_def(&format_shape(Container::SHAPE));

    let result: Result<Container, _> = from_str(json);
    match result {
        Ok(_) => println!("Unexpected success!"),
        Err(e) => {
            println!("\n{}", "Error:".bold().red());
            println!("{}", render_error(&e));
        }
    }
}

fn scenario_tuple_wrong_size() {
    print_scenario(
        "Tuple Size Mismatch",
        "JSON array has wrong number of elements for tuple type.",
    );

    let json = r#"[1, 2, 3]"#;
    print_json(json);

    print_type_def(&format_shape(<(i32, i32)>::SHAPE));

    let result: Result<(i32, i32), _> = from_str(json);
    match result {
        Ok(_) => println!("Unexpected success!"),
        Err(e) => {
            println!("\n{}", "Error:".bold().red());
            println!("{}", render_error(&e));
        }
    }
}

// ============================================================================
// Enum Errors
// ============================================================================

fn scenario_unknown_enum_variant() {
    print_scenario(
        "Unknown Enum Variant",
        "JSON specifies a variant name that doesn't exist.",
    );

    let json = r#""Unknown""#;
    print_json(json);

    #[derive(Facet, Debug)]
    #[repr(u8)]
    #[allow(dead_code)]
    enum Status {
        Active,
        Inactive,
        Pending,
    }

    print_type_def(&format_shape(Status::SHAPE));

    let result: Result<Status, _> = from_str(json);
    match result {
        Ok(_) => println!("Unexpected success!"),
        Err(e) => {
            println!("\n{}", "Error:".bold().red());
            println!("{}", render_error(&e));
        }
    }
}

fn scenario_wrong_variant_format() {
    print_scenario(
        "Wrong Variant Format",
        "Externally tagged enum expects {\"Variant\": content} but got wrong format.",
    );

    let json = r#"{"type": "Text", "content": "hello"}"#;
    print_json(json);

    #[derive(Facet, Debug)]
    #[repr(u8)]
    #[allow(dead_code)]
    enum Message {
        Text(String),
        Number(i32),
    }

    print_type_def(&format_shape(Message::SHAPE));

    let result: Result<Message, _> = from_str(json);
    match result {
        Ok(_) => println!("Unexpected success!"),
        Err(e) => {
            println!("\n{}", "Error:".bold().red());
            println!("{}", render_error(&e));
        }
    }
}

fn scenario_internally_tagged_missing_tag() {
    print_scenario(
        "Internally Tagged Enum: Missing Tag Field",
        "Internally tagged enum requires the tag field to be present.",
    );

    let json = r#"{"id": "123", "method": "ping"}"#;
    print_json(json);

    #[derive(Facet, Debug)]
    #[repr(C)]
    #[facet(tag = "type")]
    #[allow(dead_code)]
    enum Request {
        Ping { id: String },
        Echo { id: String, message: String },
    }

    print_type_def(&format_shape(Request::SHAPE));

    let result: Result<Request, _> = from_str(json);
    match result {
        Ok(_) => println!("Unexpected success!"),
        Err(e) => {
            println!("\n{}", "Error:".bold().red());
            println!("{}", render_error(&e));
        }
    }
}

// ============================================================================
// Edge Cases
// ============================================================================

fn scenario_trailing_data() {
    print_scenario(
        "Trailing Data After Valid JSON",
        "Valid JSON followed by unexpected extra content.",
    );

    let json = r#"42 extra stuff"#;
    print_json(json);

    print_type_def(&format_shape(<i32>::SHAPE));

    let result: Result<i32, _> = from_str(json);
    match result {
        Ok(_) => println!("Unexpected success!"),
        Err(e) => {
            println!("\n{}", "Error:".bold().red());
            println!("{}", render_error(&e));
        }
    }
}

fn scenario_empty_input() {
    print_scenario("Empty Input", "No JSON content at all.");

    let json = r#""#;
    print_json(json);

    print_type_def(&format_shape(<i32>::SHAPE));

    let result: Result<i32, _> = from_str(json);
    match result {
        Ok(_) => println!("Unexpected success!"),
        Err(e) => {
            println!("\n{}", "Error:".bold().red());
            println!("{}", render_error(&e));
        }
    }
}

fn scenario_unicode_content() {
    print_scenario(
        "Error with Unicode Content",
        "Error reporting handles unicode correctly.",
    );

    let json = r#"{"emoji": "🎉🚀", "count": nope}"#;
    print_json(json);

    #[derive(Facet, Debug)]
    struct EmojiData {
        emoji: String,
        count: i32,
    }

    print_type_def(&format_shape(EmojiData::SHAPE));

    let result: Result<EmojiData, _> = from_str(json);
    match result {
        Ok(_) => println!("Unexpected success!"),
        Err(e) => {
            println!("\n{}", "Error:".bold().red());
            println!("{}", render_error(&e));
        }
    }
}

// ============================================================================
// Main
// ============================================================================

fn main() {
    println!();
    let header = boxen::builder()
        .border_style(BorderStyle::Round)
        .border_color("cyan")
        .text_alignment(TextAlignment::Center)
        .padding(1)
        .render(
            "FACET-JSON ERROR SHOWCASE\n\n\
             Demonstrating rich error diagnostics with miette\n\
             All errors include source context and helpful labels",
        )
        .unwrap();
    println!("{header}");

    // Syntax errors
    scenario_syntax_error_unexpected_char();
    scenario_syntax_error_in_context();
    scenario_syntax_error_multiline();

    // Semantic errors
    scenario_unknown_field();
    scenario_type_mismatch();
    scenario_missing_field();
    scenario_number_overflow();
    scenario_wrong_type_for_array();
    scenario_tuple_wrong_size();

    // Enum errors
    scenario_unknown_enum_variant();
    scenario_wrong_variant_format();
    scenario_internally_tagged_missing_tag();

    // Edge cases
    scenario_trailing_data();
    scenario_empty_input();
    scenario_unicode_content();

    println!();
    let footer = boxen::builder()
        .border_style(BorderStyle::Round)
        .border_color("green")
        .text_alignment(TextAlignment::Center)
        .padding(1)
        .render(
            "END OF SHOWCASE\n\n\
             All diagnostics powered by miette with JSON syntax highlighting",
        )
        .unwrap();
    println!("{footer}");
}
