//! Showcase of `from_value` deserialization and error diagnostics
//!
//! Run with: cargo run -p facet-value --example from_value_showcase

use facet::Facet;
use facet_showcase::{Language, OutputMode, ShowcaseRunner, ansi_to_html};
use facet_value::{VString, Value, ValueError, from_value, value};
use owo_colors::OwoColorize;
use std::collections::HashMap;

// ============================================================================
// Example types
// ============================================================================

#[derive(Debug, Facet, PartialEq)]
struct Person {
    name: String,
    age: u32,
    email: Option<String>,
}

#[derive(Debug, Facet, PartialEq)]
struct Address {
    street: String,
    city: String,
    zip: String,
}

#[derive(Debug, Facet, PartialEq)]
struct Employee {
    person: Person,
    address: Address,
    department: String,
}

#[derive(Debug, Facet, PartialEq)]
#[repr(u8)]
#[allow(dead_code)]
enum Status {
    Active,
    Inactive,
    Pending,
}

#[derive(Debug, Facet, PartialEq)]
#[repr(u8)]
#[allow(dead_code)]
enum Message {
    Text(String),
    Number(i32),
    Data { id: u64, payload: String },
}

#[derive(Debug, Facet, PartialEq)]
struct Config {
    name: String,
    #[facet(default)]
    enabled: bool,
    #[facet(default)]
    max_retries: Option<u32>,
}

fn main() {
    let mut runner = ShowcaseRunner::new("From Value").language(Language::Rust);

    runner.header();
    runner.intro("[`facet-value`](https://docs.rs/facet-value) provides a dynamic `Value` type and conversion to/from any `Facet` type. Use it for format-agnostic data manipulation, testing, or bridging between different serialization formats.");

    // =========================================================================
    // PART 1: Successful Deserialization
    // =========================================================================

    runner.section("Happy Path");

    showcase_simple_struct(&mut runner);
    showcase_nested_struct(&mut runner);
    showcase_enums(&mut runner);
    showcase_collections(&mut runner);
    showcase_defaults(&mut runner);

    // =========================================================================
    // PART 2: Error Diagnostics
    // =========================================================================

    runner.section("Errors");

    showcase_error_type_mismatch(&mut runner);
    showcase_error_number_out_of_range(&mut runner);
    showcase_error_wrong_array_length(&mut runner);
    showcase_error_invalid_enum_variant(&mut runner);
    showcase_error_struct_for_array(&mut runner);

    runner.footer();
}

fn showcase_simple_struct(runner: &mut ShowcaseRunner) {
    let v = value!({
        "name": "Alice",
        "age": 30,
        "email": "alice@example.com"
    });

    let person: Person = from_value(v.clone()).unwrap();

    runner
        .scenario("Simple Struct")
        .description("Deserialize a `Value` map into a struct with basic fields.")
        .input_value(&v)
        .target_type::<Person>()
        .success(&person)
        .finish();
}

fn showcase_nested_struct(runner: &mut ShowcaseRunner) {
    let v = value!({
        "person": {
            "name": "Bob",
            "age": 42,
            "email": null
        },
        "address": {
            "street": "123 Main St",
            "city": "Springfield",
            "zip": "12345"
        },
        "department": "Engineering"
    });

    let emp: Employee = from_value(v.clone()).unwrap();

    runner
        .scenario("Nested Structs")
        .description("Nested structs are deserialized recursively.")
        .input_value(&v)
        .target_type::<Employee>()
        .success(&emp)
        .finish();
}

fn showcase_enums(runner: &mut ShowcaseRunner) {
    // Unit variant
    let v: Value = VString::new("Active").into();
    let status: Status = from_value(v.clone()).unwrap();

    runner
        .scenario("Unit Enum Variant")
        .description("A string value deserializes into a unit variant.")
        .input_value(&v)
        .target_type::<Status>()
        .success(&status)
        .finish();

    // Tuple variant (externally tagged)
    let v = value!({"Text": "Hello world!"});
    let msg: Message = from_value(v.clone()).unwrap();

    runner
        .scenario("Tuple Enum Variant")
        .description("Externally tagged enum: `{\"Variant\": content}`.")
        .input_value(&v)
        .target_type::<Message>()
        .success(&msg)
        .finish();

    // Struct variant
    let v = value!({"Data": {"id": 42, "payload": "secret data"}});
    let msg: Message = from_value(v.clone()).unwrap();

    runner
        .scenario("Struct Enum Variant")
        .description("Struct variants deserialize with named fields.")
        .input_value(&v)
        .target_type::<Message>()
        .success(&msg)
        .finish();
}

fn showcase_collections(runner: &mut ShowcaseRunner) {
    // Vec
    let v = value!([1, 2, 3, 4, 5]);
    let nums: Vec<i32> = from_value(v.clone()).unwrap();

    runner
        .scenario("Vec Deserialization")
        .description("Arrays deserialize into `Vec<T>`.")
        .input_value(&v)
        .target_type::<Vec<i32>>()
        .success(&nums)
        .finish();

    // Fixed array
    let v = value!(["a", "b", "c"]);
    let arr: [String; 3] = from_value(v.clone()).unwrap();

    runner
        .scenario("Fixed-Size Array")
        .description("Arrays with exact length deserialize into `[T; N]`.")
        .input_value(&v)
        .target_type::<[String; 3]>()
        .success(&arr)
        .finish();

    // HashMap
    let v = value!({"x": 10, "y": 20, "z": 30});
    let map: HashMap<String, i32> = from_value(v.clone()).unwrap();

    runner
        .scenario("HashMap")
        .description("Objects deserialize into `HashMap<String, T>`.")
        .input_value(&v)
        .target_type::<HashMap<String, i32>>()
        .success(&map)
        .finish();

    // Nested: Vec<Option<i32>>
    let v = value!([1, null, 3, null, 5]);
    let opts: Vec<Option<i32>> = from_value(v.clone()).unwrap();

    runner
        .scenario("Nested Collections")
        .description("`null` values become `None` in `Option<T>`.")
        .input_value(&v)
        .target_type::<Vec<Option<i32>>>()
        .success(&opts)
        .finish();
}

fn showcase_defaults(runner: &mut ShowcaseRunner) {
    // Only required field provided
    let v = value!({"name": "minimal"});
    let cfg: Config = from_value(v.clone()).unwrap();

    runner
        .scenario("Default Field Values")
        .description(
            "Fields marked with `#[facet(default)]` use `Default::default()` when missing.",
        )
        .input_value(&v)
        .target_type::<Config>()
        .success(&cfg)
        .finish();
}

// ============================================================================
// Error diagnostics showcase
// ============================================================================

fn print_error_scenario(
    runner: &mut ShowcaseRunner,
    name: &str,
    description: &str,
    error: ValueError,
) {
    let mode = runner.mode();
    let report = error.into_report();
    let rendered = report.render();

    match mode {
        OutputMode::Terminal => {
            println!();
            println!("{}", "═".repeat(78).dimmed());
            println!("{} {}", "SCENARIO:".bold().cyan(), name.bold().white());
            println!("{}", "─".repeat(78).dimmed());
            println!("{}", description.dimmed());
            println!("{}", "═".repeat(78).dimmed());
            println!();
            print!("{rendered}");
        }
        OutputMode::Markdown => {
            println!();
            println!("### {name}");
            println!();
            println!("<section class=\"scenario\">");
            println!("<p class=\"description\">{description}</p>");
            println!("<div class=\"error\">");
            println!("<h4>Error</h4>");
            println!("<pre><code>{}</code></pre>", ansi_to_html(&rendered));
            println!("</div>");
            println!("</section>");
        }
    }
}

fn showcase_error_type_mismatch(runner: &mut ShowcaseRunner) {
    let v: Value = VString::new("not a number").into();
    let result: Result<i32, _> = from_value(v);
    let error = result.unwrap_err();

    print_error_scenario(
        runner,
        "Error: Type Mismatch",
        "Trying to deserialize a string as an integer.",
        error,
    );
}

fn showcase_error_number_out_of_range(runner: &mut ShowcaseRunner) {
    let v = Value::from(1000i64);
    let result: Result<u8, _> = from_value(v);
    let error = result.unwrap_err();

    print_error_scenario(
        runner,
        "Error: Number Out of Range",
        "Value 1000 is too large for u8 (max 255).",
        error,
    );
}

fn showcase_error_wrong_array_length(runner: &mut ShowcaseRunner) {
    let v = value!([1, 2, 3, 4]);
    let result: Result<[i32; 3], _> = from_value(v);
    let error = result.unwrap_err();

    print_error_scenario(
        runner,
        "Error: Wrong Array Length",
        "Array has 4 elements but target type expects exactly 3.",
        error,
    );
}

fn showcase_error_invalid_enum_variant(runner: &mut ShowcaseRunner) {
    let v: Value = VString::new("Unknown").into();
    let result: Result<Status, _> = from_value(v);
    let error = result.unwrap_err();

    print_error_scenario(
        runner,
        "Error: Invalid Enum Variant",
        "\"Unknown\" is not a valid variant of Status.",
        error,
    );
}

fn showcase_error_struct_for_array(runner: &mut ShowcaseRunner) {
    let v = value!([1, 2, 3]);
    let result: Result<Person, _> = from_value(v);
    let error = result.unwrap_err();

    print_error_scenario(
        runner,
        "Error: Expected Object, Got Array",
        "Cannot deserialize an array as a struct.",
        error,
    );
}
