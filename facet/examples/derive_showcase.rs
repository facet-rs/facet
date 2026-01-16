//! Showcase of facet derive macro error messages
//!
//! This example demonstrates the helpful error messages you get when
//! using #[derive(Facet)] incorrectly.
//!
//! Run with: cargo run --example derive_showcase

use facet_showcase::{Language, ShowcaseRunner};
use std::process::Command;

fn main() {
    let mut runner = ShowcaseRunner::new("Derive Macro Diagnostics")
        .slug("derive-diagnostics")
        .language(Language::Rust);
    runner.header();
    runner.intro(
        "The `#[derive(Facet)]` macro provides helpful compile-time error messages \
         when attributes are used incorrectly. This showcase demonstrates the various \
         error scenarios and their diagnostics.",
    );

    // =========================================================================
    // #[repr(...)] Errors
    // =========================================================================

    runner.section("Representation Errors");

    scenario_repr_c_rust_conflict(&mut runner);
    scenario_repr_c_transparent_conflict(&mut runner);
    scenario_repr_transparent_primitive_conflict(&mut runner);
    scenario_repr_multiple_primitives(&mut runner);
    scenario_repr_unknown_token(&mut runner);
    scenario_repr_multiple_attributes(&mut runner);

    // =========================================================================
    // rename_all Errors
    // =========================================================================

    runner.section("Rename Errors");

    scenario_unknown_rename_all_rule(&mut runner);

    runner.footer();
}

/// Compiles a test snippet and returns the compiler error output.
fn compile_snippet(code: &str) -> String {
    use std::fs;
    use std::path::Path;

    let test_dir = Path::new("/tmp/facet-derive-error-test");
    let src_dir = test_dir.join("src");

    // Create project structure
    fs::create_dir_all(&src_dir).unwrap();

    // Write Cargo.toml with paths relative to this crate's location
    let manifest_dir = env!("CARGO_MANIFEST_DIR");
    let facet_path = Path::new(manifest_dir);

    fs::write(
        test_dir.join("Cargo.toml"),
        format!(
            r#"[package]
name = "test"
version = "0.1.0"
edition = "2021"

[dependencies]
facet = {{ path = "{}" }}
"#,
            facet_path.display(),
        ),
    )
    .unwrap();

    // Write the test code
    fs::write(src_dir.join("main.rs"), code).unwrap();

    // Run cargo check and capture output
    let output = Command::new("cargo")
        .args(["check", "--color=always"])
        .current_dir(test_dir)
        .env("CARGO_TERM_COLOR", "always")
        .output()
        .expect("Failed to run cargo check");

    String::from_utf8_lossy(&output.stderr).to_string()
}

/// Extract just the error message from cargo output, skipping the "Compiling" lines.
fn extract_error(output: &str) -> String {
    let mut lines: Vec<&str> = Vec::new();
    let mut in_error = false;

    for line in output.lines() {
        // Skip "Compiling", "Checking", "Updating", "Locking" lines
        if line.contains("Compiling")
            || line.contains("Checking")
            || line.contains("Updating")
            || line.contains("Locking")
            || line.contains("Downloading")
            || line.contains("Downloaded")
        {
            continue;
        }

        // Start capturing at "error"
        if line.contains("error") {
            in_error = true;
        }

        if in_error {
            lines.push(line);
        }
    }

    lines.join("\n")
}

// ============================================================================
// Representation Error Scenarios
// ============================================================================

fn scenario_repr_c_rust_conflict(runner: &mut ShowcaseRunner) {
    let code = r#"use facet::Facet;

#[derive(Facet)]
#[repr(C, Rust)]
enum Status {
    Active,
    Inactive,
}

fn main() {}
"#;

    let output = compile_snippet(code);
    let error = extract_error(&output);

    runner
        .scenario("Conflicting repr: C and Rust")
        .description(
            "Using both `#[repr(C)]` and `#[repr(Rust)]` is not allowed.\n\
             Facet defers to rustc's E0566 error for this - no duplicate diagnostic.",
        )
        .input(Language::Rust, code)
        .compiler_error(&error)
        .finish();
}

fn scenario_repr_c_transparent_conflict(runner: &mut ShowcaseRunner) {
    let code = r#"use facet::Facet;

#[derive(Facet)]
#[repr(C, transparent)]
struct Wrapper(u32);

fn main() {}
"#;

    let output = compile_snippet(code);
    let error = extract_error(&output);

    runner
        .scenario("Conflicting repr: C and transparent")
        .description(
            "Combining `#[repr(C)]` with `#[repr(transparent)]` is not valid.\n\
             Facet defers to rustc's E0692 error for this - no duplicate diagnostic.",
        )
        .input(Language::Rust, code)
        .compiler_error(&error)
        .finish();
}

fn scenario_repr_transparent_primitive_conflict(runner: &mut ShowcaseRunner) {
    let code = r#"use facet::Facet;

#[derive(Facet)]
#[repr(transparent, u8)]
enum Status {
    Active,
    Inactive,
}

fn main() {}
"#;

    let output = compile_snippet(code);
    let error = extract_error(&output);

    runner
        .scenario("Conflicting repr: transparent and primitive")
        .description(
            "Using `#[repr(transparent)]` with a primitive type like `u8` is not allowed.\n\
             Facet defers to rustc's E0692 error for this - no duplicate diagnostic.",
        )
        .input(Language::Rust, code)
        .compiler_error(&error)
        .finish();
}

fn scenario_repr_multiple_primitives(runner: &mut ShowcaseRunner) {
    let code = r#"use facet::Facet;

#[derive(Facet)]
#[repr(u8, u16)]
enum Priority {
    Low,
    Medium,
    High,
}

fn main() {}
"#;

    let output = compile_snippet(code);
    let error = extract_error(&output);

    runner
        .scenario("Multiple primitive types in repr")
        .description(
            "Specifying multiple primitive types in `#[repr(...)]` is not allowed.\n\
             Facet defers to rustc's E0566 error for this - no duplicate diagnostic.",
        )
        .input(Language::Rust, code)
        .compiler_error(&error)
        .finish();
}

fn scenario_repr_unknown_token(runner: &mut ShowcaseRunner) {
    let code = r#"use facet::Facet;

#[derive(Facet)]
#[repr(packed)]
struct Data {
    a: u8,
    b: u32,
}

fn main() {}
"#;

    let output = compile_snippet(code);
    let error = extract_error(&output);

    runner
        .scenario("Unsupported repr (facet-specific)")
        .description(
            "Using `#[repr(packed)]` is valid Rust, but facet doesn't support it.\n\
             This is a facet-specific error with a helpful message.",
        )
        .input(Language::Rust, code)
        .compiler_error(&error)
        .finish();
}

fn scenario_repr_multiple_attributes(runner: &mut ShowcaseRunner) {
    let code = r#"use facet::Facet;

#[derive(Facet)]
#[repr(C)]
#[repr(u8)]
enum Status {
    Active,
    Inactive,
}

fn main() {}
"#;

    let output = compile_snippet(code);
    let error = extract_error(&output);

    runner
        .scenario("Multiple #[repr] attributes")
        .description(
            "Having multiple separate `#[repr(...)]` attributes triggers rustc's E0566.\n\
             Facet defers to rustc for this - no duplicate diagnostic.",
        )
        .input(Language::Rust, code)
        .compiler_error(&error)
        .finish();
}

// ============================================================================
// Rename Error Scenarios
// ============================================================================

fn scenario_unknown_rename_all_rule(runner: &mut ShowcaseRunner) {
    let code = r#"use facet::Facet;

#[derive(Facet)]
#[facet(rename_all = "SCREAMING_SNAKE")]
struct Config {
    user_name: String,
    max_retries: u32,
}

fn main() {}
"#;

    let output = compile_snippet(code);
    let error = extract_error(&output);

    runner
        .scenario("Unknown rename_all rule (facet-specific)")
        .description(
            "Using an unknown case convention in `rename_all` is a facet-specific error.\n\
             Valid options: `camelCase`, `snake_case`, `kebab-case`, `PascalCase`, `SCREAMING_SNAKE_CASE`.",
        )
        .input(Language::Rust, code)
        .compiler_error(&error)
        .finish();
}
