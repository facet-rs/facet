//! facet-assert Showcase
//!
//! Demonstrates structural assertions without PartialEq.
//!
//! Run with: cargo run -p facet-assert --example assert_showcase

use facet::Facet;
use facet_assert::{SameReport, assert_same, assert_sameish, check_same_report};
use facet_showcase::{Language, OutputMode, ShowcaseRunner, ansi_to_html};
use owo_colors::OwoColorize;

// A type that does NOT implement PartialEq or Debug!
#[derive(Facet)]
struct Config {
    host: String,
    port: u16,
    debug: bool,
    tags: Vec<String>,
}

// Different type name, same structure
#[derive(Facet)]
struct ConfigV2 {
    host: String,
    port: u16,
    debug: bool,
    tags: Vec<String>,
}

#[derive(Facet)]
struct Person {
    name: String,
    age: u32,
    address: Address,
}

#[derive(Facet)]
struct Address {
    street: String,
    city: String,
}

fn main() {
    let mut runner = ShowcaseRunner::new("Assertions").language(Language::Rust);
    let mode = runner.mode();

    runner.header();
    runner.intro("[`facet-assert`](https://docs.rs/facet-assert) provides structural assertions for any `Facet` type without requiring `PartialEq` or `Debug`. Compare values across different types with identical structure, and get precise structural diffs showing exactly which fields differ.");

    // Scenario 1: Same values pass
    scenario_same_values(&mut runner, mode);

    // Scenario 2: Different types, same structure
    scenario_cross_type(&mut runner, mode);

    // Scenario 3: Nested structs
    scenario_nested(&mut runner, mode);

    // Scenario 4: Show what a diff looks like
    scenario_diff_output(&mut runner, mode);

    // Scenario 5: Vector differences
    scenario_vector_diff(&mut runner, mode);

    runner.footer();
}

fn scenario_same_values(runner: &mut ShowcaseRunner, _mode: OutputMode) {
    let config1 = Config {
        host: "localhost".into(),
        port: 8080,
        debug: true,
        tags: vec!["prod".into(), "api".into()],
    };
    let config2 = Config {
        host: "localhost".into(),
        port: 8080,
        debug: true,
        tags: vec!["prod".into(), "api".into()],
    };

    // This passes!
    assert_same!(config1, config2);

    runner
        .scenario("Same Values")
        .description(
            "Two values with identical content pass `assert_same!` — no `PartialEq` required.",
        )
        .target_type::<Config>()
        .success(&config1)
        .finish();
}

fn scenario_cross_type(runner: &mut ShowcaseRunner, _mode: OutputMode) {
    let v1 = Config {
        host: "localhost".into(),
        port: 8080,
        debug: true,
        tags: vec!["prod".into()],
    };
    let v2 = ConfigV2 {
        host: "localhost".into(),
        port: 8080,
        debug: true,
        tags: vec!["prod".into()],
    };

    // This passes! Different types, same structure.
    assert_sameish!(v1, v2);

    let mut scenario = runner.scenario("Cross-Type Comparison");
    scenario = scenario.description(
        "Different type names (`Config` vs `ConfigV2`) with the same structure are considered \"same\". \
         Useful for comparing DTOs across API versions or testing serialization roundtrips.",
    );
    scenario = scenario.target_type::<Config>();
    scenario = scenario.success(&v1);
    scenario.finish();
}

fn scenario_nested(runner: &mut ShowcaseRunner, _mode: OutputMode) {
    let person1 = Person {
        name: "Alice".into(),
        age: 30,
        address: Address {
            street: "123 Main St".into(),
            city: "Springfield".into(),
        },
    };
    let person2 = Person {
        name: "Alice".into(),
        age: 30,
        address: Address {
            street: "123 Main St".into(),
            city: "Springfield".into(),
        },
    };

    assert_same!(person1, person2);

    runner
        .scenario("Nested Structs")
        .description("Nested structs are compared recursively, field by field.")
        .target_type::<Person>()
        .success(&person1)
        .finish();
}

fn scenario_diff_output(runner: &mut ShowcaseRunner, mode: OutputMode) {
    let config_a = Config {
        host: "localhost".into(),
        port: 8080,
        debug: true,
        tags: vec!["prod".into(), "api".into()],
    };
    let config_b = Config {
        host: "prod.example.com".into(),
        port: 443,
        debug: false,
        tags: vec!["prod".into()],
    };

    let report = match check_same_report(&config_a, &config_b) {
        SameReport::Different(report) => report,
        _ => unreachable!(),
    };

    let rust_diff = report.legacy_string();
    let json_diff = report.render_ansi_json();
    let xml_diff = report.render_ansi_xml();

    let outputs = [
        ("Rust Diff Output", rust_diff.as_str()),
        ("JSON Diff Output", json_diff.as_str()),
        ("XML Diff Output", xml_diff.as_str()),
    ];

    print_diff_scenario(
        runner,
        mode,
        "Structural Diff",
        "When values differ, you get a precise structural diff showing exactly which fields changed \
         and at what path — then render it as Rust, JSON, or XML for whichever toolchain you need.",
        &outputs,
    );
}

fn scenario_vector_diff(runner: &mut ShowcaseRunner, mode: OutputMode) {
    let a = vec![1, 2, 3, 4, 5];
    let b = vec![1, 2, 99, 4];

    let report = match check_same_report(&a, &b) {
        SameReport::Different(report) => report,
        _ => unreachable!(),
    };

    let diff = report.legacy_string();
    let outputs = [("Diff Output", diff.as_str())];

    print_diff_scenario(
        runner,
        mode,
        "Vector Differences",
        "Vector comparisons show exactly which indices differ, which elements were added, \
         and which were removed.",
        &outputs,
    );
}

fn print_diff_scenario(
    _runner: &mut ShowcaseRunner,
    mode: OutputMode,
    name: &str,
    description: &str,
    outputs: &[(&str, &str)],
) {
    match mode {
        OutputMode::Terminal => {
            println!();
            println!("{}", "═".repeat(78).dimmed());
            println!("{} {}", "SCENARIO:".bold().cyan(), name.bold().white());
            println!("{}", "─".repeat(78).dimmed());
            println!("{}", description.dimmed());
            println!("{}", "═".repeat(78).dimmed());
            for (label, diff) in outputs {
                println!();
                println!("{}", format!("{label}:").bold().yellow());
                println!("{}", "─".repeat(60).dimmed());
                print!("{diff}");
                println!("{}", "─".repeat(60).dimmed());
            }
        }
        OutputMode::Markdown => {
            println!();
            println!("## {name}");
            println!();
            println!("<section class=\"scenario\">");
            println!("<p class=\"description\">{description}</p>");
            for (label, diff) in outputs {
                println!("<div class=\"diff-output\">");
                println!("<h4>{label}</h4>");
                println!("<pre><code>{}</code></pre>", ansi_to_html(diff));
                println!("</div>");
            }
            println!("</section>");
        }
    }
}
