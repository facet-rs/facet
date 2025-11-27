//! Showcase of facet-json serialization
//!
//! This example demonstrates various serialization scenarios with
//! syntax-highlighted JSON output and Rust type definitions via facet-pretty.

use facet::Facet;
use std::collections::HashMap;
use syntect::easy::HighlightLines;
use syntect::highlighting::{Style, ThemeSet};
use syntect::parsing::SyntaxSet;
use syntect::util::{LinesWithEndings, as_24_bit_terminal_escaped};

fn main() {
    let ps = SyntaxSet::load_defaults_newlines();
    let ts = ThemeSet::load_defaults();
    let theme = &ts.themes["base16-ocean.dark"];

    let json_syntax = ps.find_syntax_by_extension("json").unwrap();
    let rust_syntax = ps.find_syntax_by_extension("rs").unwrap();

    println!("\n{}", "═".repeat(70));
    println!("  facet-json Serialization Showcase");
    println!("{}\n", "═".repeat(70));

    // =========================================================================
    // Basic Struct
    // =========================================================================
    showcase(
        "Basic Struct",
        &Person {
            name: "Alice".to_string(),
            age: 30,
            email: Some("alice@example.com".to_string()),
        },
        &ps,
        theme,
        rust_syntax,
        json_syntax,
    );

    // =========================================================================
    // Nested Structs
    // =========================================================================
    showcase(
        "Nested Structs",
        &Company {
            name: "Acme Corp".to_string(),
            address: Address {
                street: "123 Main St".to_string(),
                city: "Springfield".to_string(),
            },
            employees: vec!["Bob".to_string(), "Carol".to_string(), "Dave".to_string()],
        },
        &ps,
        theme,
        rust_syntax,
        json_syntax,
    );

    // =========================================================================
    // Externally Tagged Enum (default)
    // =========================================================================
    showcase(
        "Externally Tagged Enum (default)",
        &[
            Message::Text("Hello, world!".to_string()),
            Message::Image {
                url: "https://example.com/cat.jpg".to_string(),
                width: 800,
            },
            Message::Ping,
        ],
        &ps,
        theme,
        rust_syntax,
        json_syntax,
    );

    // =========================================================================
    // Internally Tagged Enum
    // =========================================================================
    showcase(
        "Internally Tagged Enum (#[facet(tag = \"type\")])",
        &[
            ApiResponse::Success {
                data: "Operation completed".to_string(),
            },
            ApiResponse::Error {
                code: 404,
                message: "Not found".to_string(),
            },
        ],
        &ps,
        theme,
        rust_syntax,
        json_syntax,
    );

    // =========================================================================
    // Adjacently Tagged Enum
    // =========================================================================
    showcase(
        "Adjacently Tagged Enum (#[facet(tag = \"t\", content = \"c\")])",
        &[
            Event::Click { x: 100, y: 200 },
            Event::KeyPress('A'),
            Event::Resize,
        ],
        &ps,
        theme,
        rust_syntax,
        json_syntax,
    );

    // =========================================================================
    // Untagged Enum
    // =========================================================================
    showcase(
        "Untagged Enum (#[facet(untagged)])",
        &[
            StringOrNumber::Str("hello".to_string()),
            StringOrNumber::Num(42),
        ],
        &ps,
        theme,
        rust_syntax,
        json_syntax,
    );

    // =========================================================================
    // Maps with Various Key Types
    // =========================================================================
    let mut string_map = HashMap::new();
    string_map.insert("one".to_string(), 1);
    string_map.insert("two".to_string(), 2);

    let mut int_map = HashMap::new();
    int_map.insert(1, "one");
    int_map.insert(2, "two");

    showcase(
        "Maps (string keys)",
        &string_map,
        &ps,
        theme,
        rust_syntax,
        json_syntax,
    );

    showcase(
        "Maps (integer keys → stringified)",
        &int_map,
        &ps,
        theme,
        rust_syntax,
        json_syntax,
    );

    // =========================================================================
    // Tuple Structs
    // =========================================================================
    showcase(
        "Tuple Struct",
        &Point(10, 20, 30),
        &ps,
        theme,
        rust_syntax,
        json_syntax,
    );

    // =========================================================================
    // Pretty vs Compact
    // =========================================================================
    println!("\n{}", "─".repeat(70));
    println!("  Pretty vs Compact Output");
    println!("{}\n", "─".repeat(70));

    let data = Config {
        debug: true,
        max_connections: 100,
        endpoints: vec![
            "https://api1.example.com".to_string(),
            "https://api2.example.com".to_string(),
        ],
    };

    println!("Rust definition:");
    let rust_def = facet_pretty::format_shape(Config::SHAPE);
    highlight_code(&rust_def, &ps, theme, rust_syntax);

    println!("\nCompact JSON:");
    let compact = facet_json::to_string(&data);
    highlight_code(&compact, &ps, theme, json_syntax);

    println!("\nPretty JSON:");
    let pretty = facet_json::to_string_pretty(&data);
    highlight_code(&pretty, &ps, theme, json_syntax);

    println!("\n{}", "═".repeat(70));
}

fn showcase<T: facet::Facet<'static>>(
    title: &str,
    value: &T,
    ps: &SyntaxSet,
    theme: &syntect::highlighting::Theme,
    rust_syntax: &syntect::parsing::SyntaxReference,
    json_syntax: &syntect::parsing::SyntaxReference,
) {
    println!("{}", "─".repeat(70));
    println!("  {}", title);
    println!("{}\n", "─".repeat(70));

    println!("Rust definition:");
    let rust_def = facet_pretty::format_shape(T::SHAPE);
    highlight_code(&rust_def, ps, theme, rust_syntax);

    println!("\nValue (via facet-pretty):");
    let peek = facet_reflect::Peek::new(value);
    let pretty_value = facet_pretty::PrettyPrinter::new()
        .with_colors(false)
        .format_peek(peek);
    highlight_code(&pretty_value, ps, theme, rust_syntax);

    println!("\nJSON output:");
    let json = facet_json::to_string_pretty(value);
    highlight_code(&json, ps, theme, json_syntax);
    println!();
}

fn highlight_code(
    code: &str,
    ps: &SyntaxSet,
    theme: &syntect::highlighting::Theme,
    syntax: &syntect::parsing::SyntaxReference,
) {
    let mut h = HighlightLines::new(syntax, theme);
    for line in LinesWithEndings::from(code) {
        let ranges: Vec<(Style, &str)> = h.highlight_line(line, ps).unwrap();
        let escaped = as_24_bit_terminal_escaped(&ranges[..], false);
        print!("    {}", escaped);
    }
    println!("\x1b[0m"); // Reset terminal colors
}

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
enum Message {
    Text(String),
    Image { url: String, width: u32 },
    Ping,
}

#[derive(Facet)]
#[repr(C)]
#[facet(tag = "type")]
enum ApiResponse {
    Success { data: String },
    Error { code: i32, message: String },
}

#[derive(Facet)]
#[repr(C)]
#[facet(tag = "t", content = "c")]
enum Event {
    Click { x: i32, y: i32 },
    KeyPress(char),
    Resize,
}

#[derive(Facet)]
#[repr(u8)]
#[facet(untagged)]
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
