//! Demo of format_shape_with_spans - showing span tracking for error diagnostics
//!
//! Run with: cargo run -p facet-pretty --example span_demo

use facet::Facet;
use facet_pretty::{FormattedShape, PathSegment, format_shape_colored, format_shape_with_spans};
use owo_colors::OwoColorize;

#[derive(Facet)]
struct Config {
    name: String,
    max_retries: u8,
    timeout_ms: u32,
    enabled: bool,
}

#[derive(Facet)]
#[repr(u8)]
#[allow(dead_code)]
enum Status {
    Active,
    Pending,
    Error { code: i32, message: String },
}

fn main() {
    println!("{}", "═".repeat(70).cyan());
    println!(
        "{}",
        " format_shape_colored Demo (Tokyo Night) ".bold().cyan()
    );
    println!("{}", "═".repeat(70).cyan());
    println!();

    // Demo 1: Colored struct
    println!("{}", "1. Colored Struct".bold().yellow());
    println!("{}", "─".repeat(50).dimmed());
    println!("{}", format_shape_colored(Config::SHAPE));
    println!();

    // Demo 2: Colored enum
    println!("{}", "2. Colored Enum".bold().yellow());
    println!("{}", "─".repeat(50).dimmed());
    println!("{}", format_shape_colored(Status::SHAPE));
    println!();

    // Demo 3: Span tracking (plain text)
    println!("{}", "3. Span Tracking (plain text)".bold().yellow());
    println!("{}", "─".repeat(50).dimmed());
    demo_struct();
    println!();

    // Demo 4: Highlighting a specific field (simulating an error)
    println!("{}", "4. Simulated Error Highlight".bold().yellow());
    println!("{}", "─".repeat(50).dimmed());
    demo_error_highlight();
}

fn demo_struct() {
    let result = format_shape_with_spans(Config::SHAPE);

    println!("Formatted shape:");
    println!("{}", result.text.dimmed());
    println!();

    println!("Recorded spans:");
    for (path, field_span) in &result.spans {
        let path_str = format_path(path);
        let (start, end) = field_span.value;
        let spanned = &result.text[start..end];
        println!(
            "  {} => bytes {}..{} = {}",
            path_str.green(),
            start,
            end,
            spanned.cyan().bold()
        );
    }
}

fn demo_error_highlight() {
    let result = format_shape_with_spans(Config::SHAPE);

    // Simulate an error at max_retries field
    let error_path = vec![PathSegment::Field("max_retries")];

    if let Some(field_span) = result.spans.get(&error_path) {
        let (start, end) = field_span.value;
        println!("Simulating error: \"1000 is out of range for u8\"");
        println!();

        // Print the shape with the error field highlighted
        print_with_highlight(&result, start, end, "expected u8 (0..255)");
    }
}

fn print_with_highlight(result: &FormattedShape, start: usize, end: usize, label: &str) {
    // Split text into lines with their byte offsets
    let mut line_start = 0;
    for (line_num, line) in result.text.lines().enumerate() {
        let line_end = line_start + line.len();

        // Check if this line contains the highlighted span
        if start >= line_start && start < line_end + 1 {
            // This line contains the start of the span
            let col_start = start - line_start;
            let col_end = (end - line_start).min(line.len());

            // Print line number and content
            println!("{:>3} │ {}", (line_num + 1).dimmed(), line);

            // Print the underline
            let padding = " ".repeat(col_start);
            let underline = "^".repeat(col_end - col_start);
            println!(
                "    │ {}{}",
                padding,
                format!("{underline} {label}").red().bold()
            );
        } else {
            println!("{:>3} │ {}", (line_num + 1).dimmed(), line);
        }

        line_start = line_end + 1; // +1 for newline
    }
}

fn format_path(path: &[PathSegment]) -> String {
    path.iter()
        .map(|seg| match seg {
            PathSegment::Field(name) => format!(".{name}"),
            PathSegment::Variant(name) => format!("::{name}"),
        })
        .collect::<Vec<_>>()
        .join("")
}
