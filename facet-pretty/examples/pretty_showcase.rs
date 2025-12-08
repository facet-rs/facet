//! facet-pretty Showcase
//!
//! Demonstrates pretty-printing values and their shapes side-by-side.
//!
//! Run with: cargo run -p facet-pretty --example pretty_showcase

use facet::Facet;
use facet_pretty::FacetPretty;
use facet_showcase::{Language, OutputMode, ShowcaseRunner, ansi_to_html};
use owo_colors::OwoColorize;
use std::collections::HashMap;

fn main() {
    let mut runner = ShowcaseRunner::new("Pretty Printing").language(Language::Rust);
    let mode = runner.mode();

    runner.header();
    runner.intro("[`facet-pretty`](https://docs.rs/facet-pretty) provides colorful, readable pretty-printing for any `Facet` type. But it can also print *the shape itself* — showing the structure of your types at compile time. Below we show each value alongside its shape.");

    // Primitives
    scenario_primitives(&mut runner, mode);

    // Tuples
    scenario_tuples(&mut runner, mode);

    // Structs
    scenario_structs(&mut runner, mode);

    // Enums
    scenario_enums(&mut runner, mode);

    // Collections
    scenario_collections(&mut runner, mode);

    // Options and Results
    scenario_option_result(&mut runner, mode);

    // Nested types
    scenario_nested(&mut runner, mode);

    runner.footer();
}

// ============================================================================
// Helper for printing value + shape pairs
// ============================================================================

fn print_value_and_shape<'a, T: Facet<'a>>(
    runner: &mut ShowcaseRunner,
    mode: OutputMode,
    name: &str,
    description: &str,
    value: &'a T,
) {
    let value_pretty = format!("{}", value.pretty());
    let shape_pretty = facet_pretty::format_shape(T::SHAPE);

    match mode {
        OutputMode::Terminal => {
            println!();
            println!("{}", "═".repeat(78).dimmed());
            println!("{} {}", "SCENARIO:".bold().cyan(), name.bold().white());
            println!("{}", "─".repeat(78).dimmed());
            println!("{}", description.dimmed());
            println!("{}", "═".repeat(78).dimmed());
            println!();
            println!("{}", "Value:".bold().green());
            println!("{}", "─".repeat(60).dimmed());
            println!("  {value_pretty}");
            println!("{}", "─".repeat(60).dimmed());
            println!();
            println!("{}", "Shape:".bold().blue());
            println!("{}", "─".repeat(60).dimmed());
            print!(
                "{}",
                runner
                    .highlighter()
                    .highlight_to_terminal(&shape_pretty, Language::Rust)
            );
            println!("{}", "─".repeat(60).dimmed());
        }
        OutputMode::Markdown => {
            println!();
            println!("## {name}");
            println!();
            println!("<section class=\"scenario\">");
            println!("<p class=\"description\">{description}</p>");
            println!("<div class=\"value-output\">");
            println!("<h4>Value</h4>");
            println!("<pre><code>{}</code></pre>", ansi_to_html(&value_pretty));
            println!("</div>");
            println!("<div class=\"shape-output\">");
            println!("<h4>Shape</h4>");
            println!(
                "{}",
                runner
                    .highlighter()
                    .highlight_to_html(&shape_pretty, Language::Rust)
            );
            println!("</div>");
            println!("</section>");
        }
    }
}

// ============================================================================
// Scenarios
// ============================================================================

fn scenario_primitives(runner: &mut ShowcaseRunner, mode: OutputMode) {
    print_value_and_shape(
        runner,
        mode,
        "Primitives: Integers",
        "Simple numeric types show their value directly, and their shape reveals the underlying primitive type.",
        &42i32,
    );

    print_value_and_shape(
        runner,
        mode,
        "Primitives: Floats",
        "Floating-point numbers are displayed with their full precision.",
        &std::f64::consts::PI,
    );

    print_value_and_shape(
        runner,
        mode,
        "Primitives: Booleans",
        "Boolean values are shown as `true` or `false`.",
        &true,
    );

    print_value_and_shape(
        runner,
        mode,
        "Primitives: Characters",
        "Character values are displayed with their Unicode representation.",
        &'🦀',
    );

    print_value_and_shape(
        runner,
        mode,
        "Primitives: Strings",
        "String types show their content in quotes.",
        &String::from("Hello, facet!"),
    );
}

fn scenario_tuples(runner: &mut ShowcaseRunner, mode: OutputMode) {
    print_value_and_shape(
        runner,
        mode,
        "Tuples: Pair",
        "Tuples are displayed with their elements, and the shape shows each element's type.",
        &(3.5f64, 41u32),
    );

    print_value_and_shape(
        runner,
        mode,
        "Tuples: Triple",
        "Larger tuples work the same way — each element type is tracked.",
        &("Alice", 30u32, true),
    );
}

#[derive(Facet)]
struct Point {
    x: f64,
    y: f64,
}

#[derive(Facet)]
struct Person {
    name: String,
    age: u32,
    email: Option<String>,
}

fn scenario_structs(runner: &mut ShowcaseRunner, mode: OutputMode) {
    let point = Point { x: 1.5, y: 2.5 };
    print_value_and_shape(
        runner,
        mode,
        "Structs: Simple",
        "Struct fields are displayed with their names and values. The shape shows field names, types, and offsets.",
        &point,
    );

    let person = Person {
        name: "Alice".into(),
        age: 30,
        email: Some("alice@example.com".into()),
    };
    print_value_and_shape(
        runner,
        mode,
        "Structs: With Optional Fields",
        "Optional fields show `Some(...)` or `None`. The shape includes the full Option type.",
        &person,
    );
}

#[derive(Facet)]
#[repr(u8)]
#[allow(dead_code)]
enum Color {
    Red,
    Green,
    Blue,
    Rgb(u8, u8, u8),
}

#[derive(Facet)]
#[repr(u8)]
#[allow(dead_code)]
enum Message {
    Quit,
    Move { x: i32, y: i32 },
    Write(String),
}

fn scenario_enums(runner: &mut ShowcaseRunner, mode: OutputMode) {
    let color = Color::Blue;
    print_value_and_shape(
        runner,
        mode,
        "Enums: Unit Variant",
        "Unit variants display just the variant name. The shape shows all possible variants.",
        &color,
    );

    let rgb = Color::Rgb(255, 128, 0);
    print_value_and_shape(
        runner,
        mode,
        "Enums: Tuple Variant",
        "Tuple variants show their contained values.",
        &rgb,
    );

    let msg = Message::Move { x: 10, y: 20 };
    print_value_and_shape(
        runner,
        mode,
        "Enums: Struct Variant",
        "Struct variants display their field names and values.",
        &msg,
    );
}

fn scenario_collections(runner: &mut ShowcaseRunner, mode: OutputMode) {
    let numbers: Vec<i32> = vec![1, 2, 3, 4, 5];
    print_value_and_shape(
        runner,
        mode,
        "Collections: Vec",
        "Vectors show their elements in a list. The shape describes the element type.",
        &numbers,
    );

    let array: [u8; 4] = [10, 20, 30, 40];
    print_value_and_shape(
        runner,
        mode,
        "Collections: Array",
        "Fixed-size arrays show their elements. The shape includes the array length.",
        &array,
    );

    let mut map: HashMap<String, i32> = HashMap::new();
    map.insert("one".into(), 1);
    map.insert("two".into(), 2);
    map.insert("three".into(), 3);
    print_value_and_shape(
        runner,
        mode,
        "Collections: HashMap",
        "Maps display their key-value pairs. The shape describes both key and value types.",
        &map,
    );
}

fn scenario_option_result(runner: &mut ShowcaseRunner, mode: OutputMode) {
    let some_value: Option<String> = Some("present".into());
    print_value_and_shape(
        runner,
        mode,
        "Option: Some",
        "Option::Some displays its contained value.",
        &some_value,
    );

    let none_value: Option<i32> = None;
    print_value_and_shape(
        runner,
        mode,
        "Option: None",
        "Option::None displays cleanly without the type clutter.",
        &none_value,
    );

    let ok_result: Result<i32, String> = Ok(42);
    print_value_and_shape(
        runner,
        mode,
        "Result: Ok",
        "Result::Ok displays its success value.",
        &ok_result,
    );

    let err_result: Result<i32, String> = Err("something went wrong".into());
    print_value_and_shape(
        runner,
        mode,
        "Result: Err",
        "Result::Err displays the error value.",
        &err_result,
    );
}

#[derive(Facet)]
struct Address {
    street: String,
    city: String,
    zip: String,
}

#[derive(Facet)]
struct Company {
    name: String,
    address: Address,
    employees: Vec<Person>,
}

fn scenario_nested(runner: &mut ShowcaseRunner, mode: OutputMode) {
    let company = Company {
        name: "Acme Corp".into(),
        address: Address {
            street: "123 Main St".into(),
            city: "Springfield".into(),
            zip: "12345".into(),
        },
        employees: vec![
            Person {
                name: "Alice".into(),
                age: 30,
                email: Some("alice@acme.com".into()),
            },
            Person {
                name: "Bob".into(),
                age: 25,
                email: None,
            },
        ],
    };

    print_value_and_shape(
        runner,
        mode,
        "Nested Structures",
        "Complex nested types are pretty-printed with proper indentation. The shape shows the full type hierarchy.",
        &company,
    );
}
