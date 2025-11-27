//! Error Showcase: Demonstrating facet-yaml error diagnostics
//!
//! This example showcases the rich error reporting capabilities of facet-yaml
//! with miette's beautiful diagnostic output.
//!
//! Run with: cargo run --example error_showcase

use boxen::{BorderStyle, TextAlignment};
use facet::Facet;
use facet_pretty::format_shape;
use facet_yaml::from_str;
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

fn build_yaml_highlighter() -> SyntectHighlighter {
    let syntax_set = SyntaxSet::load_defaults_newlines();
    let theme_set = ThemeSet::load_defaults();
    let theme = theme_set.themes["base16-ocean.dark"].clone();
    SyntectHighlighter::new(syntax_set, theme, false)
}

fn render_error(err: &dyn miette::Diagnostic) -> String {
    let mut output = String::new();
    let handler = GraphicalReportHandler::new_themed(GraphicalTheme::unicode())
        .with_syntax_highlighting(build_yaml_highlighter());
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

fn print_yaml(yaml: &str) {
    let ps = SyntaxSet::load_defaults_newlines();
    let ts = ThemeSet::load_defaults();
    let theme = &ts.themes["base16-ocean.dark"];
    let syntax = ps.find_syntax_by_extension("yaml").unwrap();

    println!();
    println!("{}", "YAML Input:".bold().green());
    println!("{}", "─".repeat(60).dimmed());

    let mut h = HighlightLines::new(syntax, theme);
    for (i, line) in yaml.lines().enumerate() {
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

fn scenario_syntax_error_bad_indentation() {
    print_scenario(
        "Syntax Error: Bad Indentation",
        "YAML indentation is inconsistent or invalid.",
    );

    let yaml = r#"name: test
  nested: value
 wrong: indent"#;
    print_yaml(yaml);

    #[derive(Facet, Debug)]
    struct Config {
        name: String,
    }

    print_type_def(&format_shape(Config::SHAPE));

    let result: Result<Config, _> = from_str(yaml);
    match result {
        Ok(_) => println!("Unexpected success!"),
        Err(e) => {
            println!("\n{}", "Error:".bold().red());
            println!("{}", render_error(&e));
        }
    }
}

fn scenario_syntax_error_invalid_character() {
    print_scenario(
        "Syntax Error: Invalid Character",
        "YAML contains an invalid character in an unexpected location.",
    );

    let yaml = r#"name: @invalid"#;
    print_yaml(yaml);

    #[derive(Facet, Debug)]
    struct Config {
        name: String,
    }

    print_type_def(&format_shape(Config::SHAPE));

    let result: Result<Config, _> = from_str(yaml);
    match result {
        Ok(_) => println!("Unexpected success!"),
        Err(e) => {
            println!("\n{}", "Error:".bold().red());
            println!("{}", render_error(&e));
        }
    }
}

fn scenario_syntax_error_unclosed_quote() {
    print_scenario(
        "Syntax Error: Unclosed Quote",
        "String value has an opening quote but no closing quote.",
    );

    let yaml = r#"message: "hello world
name: test"#;
    print_yaml(yaml);

    #[derive(Facet, Debug)]
    struct Config {
        message: String,
        name: String,
    }

    print_type_def(&format_shape(Config::SHAPE));

    let result: Result<Config, _> = from_str(yaml);
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
        "YAML contains a field that doesn't exist in the target struct.\n\
         The error shows the unknown field and lists valid alternatives.",
    );

    let yaml = r#"username: alice
emial: alice@example.com"#;
    print_yaml(yaml);

    #[derive(Facet, Debug)]
    #[facet(deny_unknown_fields)]
    struct User {
        username: String,
        email: String,
    }

    print_type_def(&format_shape(User::SHAPE));

    let result: Result<User, _> = from_str(yaml);
    match result {
        Ok(_) => println!("Unexpected success!"),
        Err(e) => {
            println!("\n{}", "Error:".bold().red());
            println!("{}", render_error(&e));
        }
    }
}

fn scenario_type_mismatch_string_for_int() {
    print_scenario(
        "Type Mismatch: String for Integer",
        "YAML value is a string where an integer was expected.",
    );

    let yaml = r#"id: 42
count: "not a number""#;
    print_yaml(yaml);

    #[derive(Facet, Debug)]
    struct Item {
        id: u64,
        count: i32,
    }

    print_type_def(&format_shape(Item::SHAPE));

    let result: Result<Item, _> = from_str(yaml);
    match result {
        Ok(_) => println!("Unexpected success!"),
        Err(e) => {
            println!("\n{}", "Error:".bold().red());
            println!("{}", render_error(&e));
        }
    }
}

fn scenario_type_mismatch_int_for_string() {
    print_scenario(
        "Type Mismatch: Integer for String",
        "YAML value is an integer where a string was expected (may succeed with coercion).",
    );

    let yaml = r#"id: 42
name: 123"#;
    print_yaml(yaml);

    #[derive(Facet, Debug)]
    struct Item {
        id: u64,
        name: String,
    }

    print_type_def(&format_shape(Item::SHAPE));

    let result: Result<Item, _> = from_str(yaml);
    match result {
        Ok(item) => {
            println!("\n{}", "Success (with coercion):".bold().green());
            println!("  {:?}", item);
        }
        Err(e) => {
            println!("\n{}", "Error:".bold().red());
            println!("{}", render_error(&e));
        }
    }
}

fn scenario_missing_field() {
    print_scenario(
        "Missing Required Field",
        "YAML is missing a required field that has no default.",
    );

    let yaml = r#"host: localhost"#;
    print_yaml(yaml);

    #[derive(Facet, Debug)]
    struct ServerConfig {
        host: String,
        port: u16,
    }

    print_type_def(
        r#"#[derive(Facet)]
struct ServerConfig {
    host: String,
    port: u16,  // Required but missing from YAML
}"#,
    );

    let result: Result<ServerConfig, _> = from_str(yaml);
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
        "YAML number is too large for the target integer type.",
    );

    let yaml = r#"count: 999999999999"#;
    print_yaml(yaml);

    #[derive(Facet, Debug)]
    struct Counter {
        count: u32,
    }

    print_type_def(
        r#"#[derive(Facet)]
struct Counter {
    count: u32,  // Max value is 4,294,967,295
}"#,
    );

    let result: Result<Counter, _> = from_str(yaml);
    match result {
        Ok(_) => println!("Unexpected success!"),
        Err(e) => {
            println!("\n{}", "Error:".bold().red());
            println!("{}", render_error(&e));
        }
    }
}

fn scenario_wrong_type_for_sequence() {
    print_scenario(
        "Expected Sequence, Got Scalar",
        "YAML has a scalar where a sequence was expected.",
    );

    let yaml = r#"items: "not a sequence""#;
    print_yaml(yaml);

    #[derive(Facet, Debug)]
    struct Container {
        items: Vec<i32>,
    }

    print_type_def(
        r#"#[derive(Facet)]
struct Container {
    items: Vec<i32>,  // Expected sequence, got string
}"#,
    );

    let result: Result<Container, _> = from_str(yaml);
    match result {
        Ok(_) => println!("Unexpected success!"),
        Err(e) => {
            println!("\n{}", "Error:".bold().red());
            println!("{}", render_error(&e));
        }
    }
}

fn scenario_wrong_type_for_mapping() {
    print_scenario(
        "Expected Mapping, Got Scalar",
        "YAML has a scalar where a mapping was expected.",
    );

    let yaml = r#"config: "not a mapping""#;
    print_yaml(yaml);

    #[derive(Facet, Debug)]
    struct Nested {
        value: i32,
    }

    #[derive(Facet, Debug)]
    struct Outer {
        config: Nested,
    }

    print_type_def(
        r#"#[derive(Facet)]
struct Nested {
    value: i32,
}

#[derive(Facet)]
struct Outer {
    config: Nested,  // Expected mapping, got string
}"#,
    );

    let result: Result<Outer, _> = from_str(yaml);
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
        "YAML specifies a variant name that doesn't exist.",
    );

    let yaml = r#"Unknown"#;
    print_yaml(yaml);

    #[derive(Facet, Debug)]
    #[repr(u8)]
    #[allow(dead_code)]
    enum Status {
        Active,
        Inactive,
        Pending,
    }

    print_type_def(
        r#"#[derive(Facet)]
#[repr(u8)]
enum Status {
    Active,
    Inactive,
    Pending,
}
// YAML has "Unknown" which is not a valid variant"#,
    );

    let result: Result<Status, _> = from_str(yaml);
    match result {
        Ok(_) => println!("Unexpected success!"),
        Err(e) => {
            println!("\n{}", "Error:".bold().red());
            println!("{}", render_error(&e));
        }
    }
}

fn scenario_enum_wrong_format() {
    print_scenario(
        "Enum Wrong Format",
        "Externally tagged enum expects {Variant: content} but got wrong format.",
    );

    let yaml = r#"type: Text
content: hello"#;
    print_yaml(yaml);

    #[derive(Facet, Debug)]
    #[repr(u8)]
    #[allow(dead_code)]
    enum Message {
        Text(String),
        Number(i32),
    }

    print_type_def(
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
    );

    let result: Result<Message, _> = from_str(yaml);
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

    let yaml = r#"id: "123"
method: ping"#;
    print_yaml(yaml);

    #[derive(Facet, Debug)]
    #[repr(C)]
    #[facet(tag = "type")]
    #[allow(dead_code)]
    enum Request {
        Ping { id: String },
        Echo { id: String, message: String },
    }

    print_type_def(
        r#"#[derive(Facet)]
#[repr(C)]
#[facet(tag = "type")]
enum Request {
    Ping { id: String },
    Echo { id: String, message: String },
}
// YAML is missing the "type" tag field"#,
    );

    let result: Result<Request, _> = from_str(yaml);
    match result {
        Ok(_) => println!("Unexpected success!"),
        Err(e) => {
            println!("\n{}", "Error:".bold().red());
            println!("{}", render_error(&e));
        }
    }
}

// ============================================================================
// YAML-Specific Features
// ============================================================================

fn scenario_duplicate_key() {
    print_scenario(
        "Duplicate Key",
        "YAML mapping contains the same key more than once.",
    );

    let yaml = r#"name: first
value: 42
name: second"#;
    print_yaml(yaml);

    #[derive(Facet, Debug)]
    struct Config {
        name: String,
        value: i32,
    }

    print_type_def(&format_shape(Config::SHAPE));

    let result: Result<Config, _> = from_str(yaml);
    match result {
        Ok(config) => {
            println!(
                "\n{}",
                "Note: Duplicate keys may be allowed:".bold().yellow()
            );
            println!("  {:?}", config);
        }
        Err(e) => {
            println!("\n{}", "Error:".bold().red());
            println!("{}", render_error(&e));
        }
    }
}

fn scenario_anchor_reference() {
    print_scenario(
        "Anchors and Aliases",
        "YAML anchors and aliases for value reuse.",
    );

    let yaml = r#"defaults: &defaults
  timeout: 30
  retries: 3

production:
  <<: *defaults
  host: prod.example.com

staging:
  <<: *defaults
  host: staging.example.com"#;
    print_yaml(yaml);

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

    print_type_def(&format_shape(AllConfigs::SHAPE));

    let result: Result<AllConfigs, _> = from_str(yaml);
    match result {
        Ok(configs) => {
            println!("\n{}", "Success:".bold().green());
            println!("  {:?}", configs);
        }
        Err(e) => {
            println!("\n{}", "Error:".bold().red());
            println!("{}", render_error(&e));
        }
    }
}

fn scenario_multiline_string() {
    print_scenario(
        "Multiline String Styles",
        "YAML supports various multiline string styles.",
    );

    let yaml = r#"literal: |
  This is a literal block.
  Newlines are preserved.

folded: >
  This is a folded block.
  Lines get folded into
  a single paragraph."#;
    print_yaml(yaml);

    #[derive(Facet, Debug)]
    struct TextContent {
        literal: String,
        folded: String,
    }

    print_type_def(&format_shape(TextContent::SHAPE));

    let result: Result<TextContent, _> = from_str(yaml);
    match result {
        Ok(content) => {
            println!("\n{}", "Success:".bold().green());
            println!("  literal: {:?}", content.literal);
            println!("  folded: {:?}", content.folded);
        }
        Err(e) => {
            println!("\n{}", "Error:".bold().red());
            println!("{}", render_error(&e));
        }
    }
}

// ============================================================================
// Edge Cases
// ============================================================================

fn scenario_empty_input() {
    print_scenario("Empty Input", "No YAML content at all.");

    let yaml = r#""#;
    print_yaml(yaml);

    print_type_def(r#"i32"#);

    let result: Result<i32, _> = from_str(yaml);
    match result {
        Ok(_) => println!("Unexpected success!"),
        Err(e) => {
            println!("\n{}", "Error:".bold().red());
            println!("{}", render_error(&e));
        }
    }
}

fn scenario_null_for_required() {
    print_scenario(
        "Null for Required Field",
        "YAML has explicit null where a value is required.",
    );

    let yaml = r#"name: ~
count: 42"#;
    print_yaml(yaml);

    #[derive(Facet, Debug)]
    struct Item {
        name: String,
        count: i32,
    }

    print_type_def(&format_shape(Item::SHAPE));

    let result: Result<Item, _> = from_str(yaml);
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

    let yaml = r#"emoji: "🎉🚀"
count: nope"#;
    print_yaml(yaml);

    #[derive(Facet, Debug)]
    struct EmojiData {
        emoji: String,
        count: i32,
    }

    print_type_def(&format_shape(EmojiData::SHAPE));

    let result: Result<EmojiData, _> = from_str(yaml);
    match result {
        Ok(_) => println!("Unexpected success!"),
        Err(e) => {
            println!("\n{}", "Error:".bold().red());
            println!("{}", render_error(&e));
        }
    }
}

fn scenario_nested_error() {
    print_scenario(
        "Error in Nested Structure",
        "Error location is correctly identified in deeply nested YAML.",
    );

    let yaml = r#"server:
  host: localhost
  ports:
    http: 8080
    https: "not a number"
  database:
    url: postgres://localhost/db"#;
    print_yaml(yaml);

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

    print_type_def(&format_shape(AppConfig::SHAPE));

    let result: Result<AppConfig, _> = from_str(yaml);
    match result {
        Ok(_) => println!("Unexpected success!"),
        Err(e) => {
            println!("\n{}", "Error:".bold().red());
            println!("{}", render_error(&e));
        }
    }
}

fn scenario_sequence_item_error() {
    print_scenario(
        "Error in Sequence Item",
        "Error in one item of a sequence is reported with context.",
    );

    let yaml = r#"users:
  - name: Alice
    age: 30
  - name: Bob
    age: "twenty-five"
  - name: Charlie
    age: 35"#;
    print_yaml(yaml);

    #[derive(Facet, Debug)]
    struct User {
        name: String,
        age: u32,
    }

    #[derive(Facet, Debug)]
    struct UserList {
        users: Vec<User>,
    }

    print_type_def(&format_shape(UserList::SHAPE));

    let result: Result<UserList, _> = from_str(yaml);
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
            "FACET-YAML ERROR SHOWCASE\n\n\
             Demonstrating rich error diagnostics with miette\n\
             All errors include source context and helpful labels",
        )
        .unwrap();
    println!("{header}");

    // Syntax errors
    scenario_syntax_error_bad_indentation();
    scenario_syntax_error_invalid_character();
    scenario_syntax_error_unclosed_quote();

    // Semantic errors
    scenario_unknown_field();
    scenario_type_mismatch_string_for_int();
    scenario_type_mismatch_int_for_string();
    scenario_missing_field();
    scenario_number_overflow();
    scenario_wrong_type_for_sequence();
    scenario_wrong_type_for_mapping();

    // Enum errors
    scenario_unknown_enum_variant();
    scenario_enum_wrong_format();
    scenario_internally_tagged_missing_tag();

    // YAML-specific features
    scenario_duplicate_key();
    scenario_anchor_reference();
    scenario_multiline_string();

    // Edge cases
    scenario_empty_input();
    scenario_null_for_required();
    scenario_unicode_content();
    scenario_nested_error();
    scenario_sequence_item_error();

    println!();
    let footer = boxen::builder()
        .border_style(BorderStyle::Round)
        .border_color("green")
        .text_alignment(TextAlignment::Center)
        .padding(1)
        .render(
            "END OF SHOWCASE\n\n\
             All diagnostics powered by miette with YAML syntax highlighting",
        )
        .unwrap();
    println!("{footer}");
}
