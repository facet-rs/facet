//! Showcase of facet-kdl compile-time error messages
//!
//! This example demonstrates the helpful error messages you get when
//! using extension attributes incorrectly.
//!
//! Run with: cargo run --example compile_errors_showcase

use facet_showcase::{Language, ShowcaseRunner};
use std::process::Command;

fn main() {
    let mut runner = ShowcaseRunner::new("Diagnostics")
        .language(Language::Rust)
        .with_kdl_syntaxes(concat!(env!("CARGO_MANIFEST_DIR"), "/syntaxes"));
    runner.header();

    // =========================================================================
    // Unknown Extension Attribute
    // =========================================================================

    scenario_unknown_attribute(&mut runner);
    scenario_typo_in_attribute(&mut runner);
    scenario_attribute_with_unexpected_args(&mut runner);
    scenario_valid_usage(&mut runner);

    runner.footer();
}

/// Compiles a test snippet and returns the compiler error output.
fn compile_snippet(code: &str) -> String {
    use std::fs;
    use std::path::Path;

    let test_dir = Path::new("/tmp/facet-compile-error-test");
    let src_dir = test_dir.join("src");

    // Create project structure
    fs::create_dir_all(&src_dir).unwrap();

    // Write Cargo.toml with paths relative to this crate's location
    let manifest_dir = env!("CARGO_MANIFEST_DIR");
    let facet_path = Path::new(manifest_dir).join("../facet");
    let facet_kdl_path = Path::new(manifest_dir);

    fs::write(
        test_dir.join("Cargo.toml"),
        format!(
            r#"[package]
name = "test"
version = "0.1.0"
edition = "2021"

[dependencies]
facet = {{ path = "{}" }}
facet-kdl = {{ path = "{}" }}
"#,
            facet_path.display(),
            facet_kdl_path.display()
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

fn scenario_unknown_attribute(runner: &mut ShowcaseRunner) {
    let code = r#"use facet::Facet;
use facet_kdl_legacy as kdl;

#[derive(Facet)]
struct Config {
    #[facet(kdl::nonexistent)]
    field: String,
}

fn main() {}
"#;

    let output = compile_snippet(code);
    let error = extract_error(&output);

    runner
        .scenario("Unknown Extension Attribute")
        .description(
            "Using an unknown attribute like `kdl::nonexistent` produces a clear error\n\
             that points directly to the attribute and suggests valid options.",
        )
        .input(Language::Rust, code)
        .compiler_error(&error)
        .finish();
}

fn scenario_typo_in_attribute(runner: &mut ShowcaseRunner) {
    let code = r#"use facet::Facet;
use facet_kdl_legacy as kdl;

#[derive(Facet)]
struct Config {
    #[facet(kdl::chld)]
    nested: Inner,
}

#[derive(Facet)]
struct Inner {
    #[facet(kdl::proprty)]
    value: String,
}

fn main() {}
"#;

    let output = compile_snippet(code);
    let error = extract_error(&output);

    runner
        .scenario("Typo in Attribute Name")
        .description(
            "Common typos like `chld` instead of `child` or `proprty` instead of `property`\n\
             are caught at compile time with helpful suggestions.",
        )
        .input(Language::Rust, code)
        .compiler_error(&error)
        .finish();
}

fn scenario_attribute_with_unexpected_args(runner: &mut ShowcaseRunner) {
    let code = r#"use facet::Facet;
use facet_kdl_legacy as kdl;

#[derive(Facet)]
struct Config {
    #[facet(kdl::child = "unexpected")]
    nested: Inner,
}

#[derive(Facet)]
struct Inner {
    value: String,
}

fn main() {}
"#;

    let output = compile_snippet(code);
    let error = extract_error(&output);

    runner
        .scenario("Attribute with Unexpected Arguments")
        .description(
            "Passing arguments to attributes that don't accept them produces a clear error.",
        )
        .input(Language::Rust, code)
        .compiler_error(&error)
        .finish();
}

fn scenario_valid_usage(runner: &mut ShowcaseRunner) {
    let code = r#"use facet::Facet;
use facet_kdl_legacy as kdl;

#[derive(Facet)]
struct Config {
    #[facet(kdl::child)]
    server: Server,

    #[facet(kdl::property)]
    name: String,

    #[facet(kdl::argument)]
    version: u32,
}

#[derive(Facet)]
struct Server {
    #[facet(kdl::property)]
    host: String,

    #[facet(kdl::property)]
    port: u16,
}

fn main() {
    println!("Compiles successfully!");
}
"#;

    let output = compile_snippet(code);

    // Check if compilation succeeded (no "error" in output)
    let has_error = output.contains("error[") || output.contains("error:");
    let error_output = if has_error {
        extract_error(&output)
    } else {
        "Compilation successful! No errors.".to_string()
    };

    runner
        .scenario("Valid Usage")
        .description(
            "When extension attributes are used correctly, everything compiles smoothly.\n\
             This shows the intended usage pattern for KDL attributes.",
        )
        .input(Language::Rust, code)
        .compiler_error(&error_output)
        .finish();
}
