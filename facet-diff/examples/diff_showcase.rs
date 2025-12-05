//! facet-diff Showcase
//!
//! Demonstrates structural diffing capabilities.
//!
//! Run with: cargo run -p facet-diff --example diff_showcase

use facet::Facet;
use facet_diff::FacetDiff;
use facet_showcase::{Language, OutputMode, ShowcaseRunner, ansi_to_html};
use owo_colors::OwoColorize;

#[derive(Facet)]
struct Config {
    name: String,
    version: u32,
    debug: bool,
    features: Vec<String>,
}

#[derive(Facet)]
struct User {
    name: String,
    email: Option<String>,
    bio: Option<String>,
    age: Option<u32>,
}

#[derive(Facet, PartialEq)]
struct Document {
    title: String,
    author: String,
    tags: Vec<String>,
}

#[derive(Facet)]
#[repr(C)]
#[allow(dead_code)]
enum Status {
    Pending,
    InProgress { assignee: String },
    Completed { by: String, notes: Option<String> },
}

fn main() {
    let mut runner = ShowcaseRunner::new("Structural Diff").language(Language::Rust);
    let mode = runner.mode();

    runner.header();
    runner.intro("[`facet-diff`](https://docs.rs/facet-diff) provides structural diffing for any `Facet` type. Get readable, colored diffs showing exactly what changed between two values — perfect for debugging, testing, and understanding data transformations.");

    // Basic struct diff
    scenario_struct_diff(&mut runner, mode);

    // Option field handling
    scenario_option_fields(&mut runner, mode);

    // Nested structures
    scenario_nested_diff(&mut runner, mode);

    // Vector diffs
    scenario_vector_diff(&mut runner, mode);

    // Enum diffs
    scenario_enum_diff(&mut runner, mode);

    runner.footer();
}

fn scenario_struct_diff(runner: &mut ShowcaseRunner, mode: OutputMode) {
    let config_a = Config {
        name: "my-app".into(),
        version: 1,
        debug: true,
        features: vec!["logging".into(), "metrics".into()],
    };
    let config_b = Config {
        name: "my-app".into(),
        version: 2,
        debug: false,
        features: vec!["logging".into(), "tracing".into()],
    };

    let diff = config_a.diff(&config_b);

    print_diff_scenario(
        runner,
        mode,
        "Struct Field Changes",
        "Compare two structs and see exactly which fields changed. Unchanged fields are collapsed into a summary.",
        &format!("{diff}"),
    );
}

fn scenario_option_fields(runner: &mut ShowcaseRunner, mode: OutputMode) {
    let user_a = User {
        name: "Alice".into(),
        email: None,
        bio: None,
        age: Some(30),
    };
    let user_b = User {
        name: "Alice".into(),
        email: Some("alice@example.com".into()),
        bio: Some("Software engineer".into()),
        age: Some(31),
    };

    let diff = user_a.diff(&user_b);

    print_diff_scenario(
        runner,
        mode,
        "Option Field Changes",
        "Option fields show clean `None` → `Some(...)` transitions without verbose type names.",
        &format!("{diff}"),
    );
}

fn scenario_nested_diff(runner: &mut ShowcaseRunner, mode: OutputMode) {
    let doc_a = Document {
        title: "API Guide".into(),
        author: "Alice".into(),
        tags: vec!["api".into(), "guide".into()],
    };
    let doc_b = Document {
        title: "API Reference".into(),
        author: "Bob".into(),
        tags: vec!["api".into(), "reference".into()],
    };

    let diff = doc_a.diff(&doc_b);

    print_diff_scenario(
        runner,
        mode,
        "Nested Structure Diffs",
        "Structs with nested vectors are diffed recursively, showing changes at any depth.",
        &format!("{diff}"),
    );
}

fn scenario_vector_diff(runner: &mut ShowcaseRunner, mode: OutputMode) {
    // Use a pattern that works with the current diff algorithm
    let items_a: Vec<i32> = vec![1, 2, 3, 4, 5];
    let items_b: Vec<i32> = vec![1, 2, 99, 4, 5];

    let diff = items_a.diff(&items_b);

    print_diff_scenario(
        runner,
        mode,
        "Vector Diffs",
        "Vector comparisons identify which elements changed while preserving context around the changes.",
        &format!("{diff}"),
    );
}

fn scenario_enum_diff(runner: &mut ShowcaseRunner, mode: OutputMode) {
    let status_a = Status::InProgress {
        assignee: "Alice".into(),
    };
    let status_b = Status::Completed {
        by: "Alice".into(),
        notes: Some("Shipped in v2.0".into()),
    };

    let diff = status_a.diff(&status_b);

    print_diff_scenario(
        runner,
        mode,
        "Enum Variant Changes",
        "When enum variants differ entirely, the diff shows a clean replacement. When only the variant's fields differ, those specific changes are highlighted.",
        &format!("{diff}"),
    );

    // Also show same-variant diff
    let status_c = Status::Completed {
        by: "Alice".into(),
        notes: None,
    };
    let status_d = Status::Completed {
        by: "Bob".into(),
        notes: Some("Peer reviewed".into()),
    };

    let diff2 = status_c.diff(&status_d);

    print_diff_scenario(
        runner,
        mode,
        "Same Variant, Different Fields",
        "When comparing the same enum variant with different field values, only the changed fields are shown.",
        &format!("{diff2}"),
    );
}

fn print_diff_scenario(
    _runner: &mut ShowcaseRunner,
    mode: OutputMode,
    name: &str,
    description: &str,
    diff: &str,
) {
    match mode {
        OutputMode::Terminal => {
            println!();
            println!("{}", "═".repeat(78).dimmed());
            println!("{} {}", "SCENARIO:".bold().cyan(), name.bold().white());
            println!("{}", "─".repeat(78).dimmed());
            println!("{}", description.dimmed());
            println!("{}", "═".repeat(78).dimmed());
            println!();
            println!("{}", "Diff Output:".bold().yellow());
            println!("{}", "─".repeat(60).dimmed());
            print!("{diff}");
            println!();
            println!("{}", "─".repeat(60).dimmed());
        }
        OutputMode::Markdown => {
            println!();
            println!("## {name}");
            println!();
            println!("<section class=\"scenario\">");
            println!("<p class=\"description\">{description}</p>");
            println!("<div class=\"diff-output\">");
            println!("<h4>Diff Output</h4>");
            println!("<pre><code>{}</code></pre>", ansi_to_html(diff));
            println!("</div>");
            println!("</section>");
        }
    }
}
