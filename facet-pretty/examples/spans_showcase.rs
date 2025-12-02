//! Showcase of format_shape_with_spans - span tracking for error diagnostics
//!
//! Run with: cargo run -p facet-pretty --example spans_showcase

use facet::Facet;
use facet_pretty::{FieldSpan, PathSegment, format_shape_colored, format_shape_with_spans};
use facet_showcase::{OutputMode, ShowcaseRunner, ansi_to_html};
use miette::{
    Diagnostic, GraphicalReportHandler, GraphicalTheme, LabeledSpan, NamedSource, Severity,
};
use owo_colors::OwoColorize;
use std::collections::HashMap;
use std::fmt;

// ============================================================================
// Test Types
// ============================================================================

#[derive(Facet)]
struct Config {
    name: String,
    max_retries: u8,
    timeout_ms: u32,
    enabled: bool,
}

#[derive(Facet)]
struct Person {
    name: String,
    age: u8,
    email: Option<String>,
}

#[derive(Facet)]
struct Address {
    street: String,
    city: String,
    zip: String,
}

#[derive(Facet)]
struct Employee {
    person: Person,
    address: Address,
    tags: Vec<String>,
    metadata: HashMap<String, i32>,
}

#[derive(Facet)]
#[repr(u8)]
#[allow(dead_code)]
enum Status {
    Active,
    Pending,
    Error { code: i32, message: String },
}

#[derive(Facet)]
#[repr(u8)]
#[allow(dead_code)]
enum Message {
    Text(String),
    Number(i32),
    Pair(String, i32),
    Data { id: u64, payload: Vec<u8> },
}

// ============================================================================
// Error type using miette
// ============================================================================

#[derive(Debug)]
struct ShapeError {
    src: NamedSource<String>,
    label: LabeledSpan,
    message: String,
}

impl fmt::Display for ShapeError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.message)
    }
}

impl std::error::Error for ShapeError {}

impl Diagnostic for ShapeError {
    fn source_code(&self) -> Option<&dyn miette::SourceCode> {
        Some(&self.src)
    }

    fn labels(&self) -> Option<Box<dyn Iterator<Item = LabeledSpan> + '_>> {
        Some(Box::new(std::iter::once(self.label.clone())))
    }

    fn severity(&self) -> Option<Severity> {
        Some(Severity::Error)
    }
}

// ============================================================================
// Highlight Mode
// ============================================================================

#[derive(Clone, Copy)]
enum HighlightMode {
    /// Highlight only the field/variant name
    Key,
    /// Highlight only the type annotation
    Value,
    /// Highlight both key and value merged
    Both,
}

// ============================================================================
// Main
// ============================================================================

fn main() {
    let mut runner = ShowcaseRunner::new("Spans");
    runner.header();

    runner.section("Highlight Modes");
    demo_highlight_key(&mut runner);
    demo_highlight_value(&mut runner);
    demo_highlight_both(&mut runner);

    runner.section("Nested Structures");
    demo_nested_struct(&mut runner);
    demo_nested_field(&mut runner);

    runner.section("Enum Variants");
    demo_unit_variant(&mut runner);
    demo_tuple_variant(&mut runner);
    demo_struct_variant(&mut runner);

    runner.section("Collections");
    demo_vec_field(&mut runner);
    demo_option_field(&mut runner);
    demo_hashmap_field(&mut runner);

    runner.footer();
}

// ============================================================================
// Demos
// ============================================================================

fn demo_highlight_key(runner: &mut ShowcaseRunner) {
    show_error(
        runner,
        "Highlight Field Name",
        "Point to the field name when it's unknown or unexpected.",
        Config::SHAPE,
        &[PathSegment::Field("max_retries")],
        HighlightMode::Key,
        "unknown field `max_retries`",
        "not expected here",
    );
}

fn demo_highlight_value(runner: &mut ShowcaseRunner) {
    show_error(
        runner,
        "Highlight Type",
        "Point to the type when the value doesn't match.",
        Config::SHAPE,
        &[PathSegment::Field("max_retries")],
        HighlightMode::Value,
        "value 1000 is out of range for u8",
        "expected 0..255",
    );
}

fn demo_highlight_both(runner: &mut ShowcaseRunner) {
    show_error(
        runner,
        "Highlight Entire Field",
        "Point to both name and type for context.",
        Config::SHAPE,
        &[PathSegment::Field("timeout_ms")],
        HighlightMode::Both,
        "missing required field",
        "this field is required",
    );
}

fn demo_nested_struct(runner: &mut ShowcaseRunner) {
    show_error(
        runner,
        "Nested Struct Field",
        "Highlight a field inside a nested struct.",
        Employee::SHAPE,
        &[PathSegment::Field("person")],
        HighlightMode::Value,
        "invalid person data",
        "expected valid Person",
    );
}

fn demo_nested_field(runner: &mut ShowcaseRunner) {
    show_error(
        runner,
        "Deeply Nested Field",
        "Highlight a deeply nested field path.",
        Employee::SHAPE,
        &[PathSegment::Field("address")],
        HighlightMode::Both,
        "address validation failed",
        "city is required",
    );
}

fn demo_unit_variant(runner: &mut ShowcaseRunner) {
    show_error(
        runner,
        "Unit Variant",
        "Highlight an enum variant name.",
        Status::SHAPE,
        &[PathSegment::Variant("Active")],
        HighlightMode::Value,
        "invalid variant",
        "not allowed in this context",
    );
}

fn demo_tuple_variant(runner: &mut ShowcaseRunner) {
    show_error(
        runner,
        "Tuple Variant",
        "Highlight a tuple variant.",
        Message::SHAPE,
        &[PathSegment::Variant("Text")],
        HighlightMode::Value,
        "type mismatch",
        "expected Number, got Text",
    );
}

fn demo_struct_variant(runner: &mut ShowcaseRunner) {
    show_error(
        runner,
        "Struct Variant Field",
        "Highlight a field inside a struct variant.",
        Status::SHAPE,
        &[PathSegment::Variant("Error"), PathSegment::Field("code")],
        HighlightMode::Value,
        "error code out of range",
        "must be positive",
    );
}

fn demo_vec_field(runner: &mut ShowcaseRunner) {
    show_error(
        runner,
        "Vec Field",
        "Highlight a Vec field type.",
        Employee::SHAPE,
        &[PathSegment::Field("tags")],
        HighlightMode::Value,
        "invalid tags",
        "expected array of strings",
    );
}

fn demo_option_field(runner: &mut ShowcaseRunner) {
    show_error(
        runner,
        "Option Field",
        "Highlight an Option field.",
        Person::SHAPE,
        &[PathSegment::Field("email")],
        HighlightMode::Both,
        "invalid email format",
        "must be a valid email address",
    );
}

fn demo_hashmap_field(runner: &mut ShowcaseRunner) {
    show_error(
        runner,
        "HashMap Field",
        "Highlight a HashMap field.",
        Employee::SHAPE,
        &[PathSegment::Field("metadata")],
        HighlightMode::Value,
        "invalid metadata",
        "keys must be alphanumeric",
    );
}

// ============================================================================
// Error Display Helper
// ============================================================================

#[allow(clippy::too_many_arguments)]
fn show_error(
    runner: &mut ShowcaseRunner,
    name: &str,
    description: &str,
    shape: &'static facet_core::Shape,
    path: &[PathSegment],
    mode: HighlightMode,
    error_message: &str,
    label_text: &str,
) {
    let result = format_shape_with_spans(shape);
    let colored = format_shape_colored(shape);
    let path_vec: Vec<PathSegment> = path.to_vec();

    let field_span = result.spans.get(&path_vec).cloned().unwrap_or_default();
    let (start, end) = compute_span(&field_span, mode);

    // Create miette error
    let error = ShapeError {
        src: NamedSource::new("target type", result.text.clone()),
        label: LabeledSpan::at(start..end, label_text),
        message: error_message.to_string(),
    };

    // Render with miette
    let mut output = String::new();
    let handler = GraphicalReportHandler::new_themed(GraphicalTheme::unicode());
    handler.render_report(&mut output, &error).unwrap();

    let mode_output = runner.mode();

    match mode_output {
        OutputMode::Terminal => {
            println!();
            println!("{}", "═".repeat(78).dimmed());
            println!("{} {}", "SCENARIO:".bold().cyan(), name.bold().white());
            println!("{}", "─".repeat(78).dimmed());
            println!("{}", description.dimmed());
            println!("{}", "═".repeat(78).dimmed());
            println!();

            // Show the type with colored syntax
            println!("{}", "Target Type:".bold().blue());
            println!("{}", "─".repeat(60).dimmed());
            for line in colored.lines() {
                println!("  {line}");
            }
            println!("{}", "─".repeat(60).dimmed());
            println!();

            // Show the miette error
            print!("{output}");
        }
        OutputMode::Markdown => {
            println!();
            println!("### {name}");
            println!();
            println!("<section class=\"scenario\">");
            println!("<p class=\"description\">{description}</p>");

            println!("<div class=\"target-type\">");
            println!("<h4>Target Type</h4>");
            println!("<pre><code>{}</code></pre>", ansi_to_html(&colored));
            println!("</div>");

            println!("<div class=\"error\">");
            println!("<h4>Error</h4>");
            println!("<pre><code>{}</code></pre>", ansi_to_html(&output));
            println!("</div>");

            println!("</section>");
        }
    }
}

fn compute_span(field_span: &FieldSpan, mode: HighlightMode) -> (usize, usize) {
    match mode {
        HighlightMode::Key => field_span.key,
        HighlightMode::Value => field_span.value,
        HighlightMode::Both => {
            let start = field_span.key.0.min(field_span.value.0);
            let end = field_span.key.1.max(field_span.value.1);
            (start, end)
        }
    }
}
