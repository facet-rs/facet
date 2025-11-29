//! Showcase of proto-attr compile-time error messages
//!
//! This example demonstrates the helpful error messages you get when
//! using extension attributes incorrectly.
//!
//! Run with: cargo run --example proto_attr_showcase

use facet_showcase::{Language, ShowcaseRunner};
use std::process::Command;

fn main() {
    let mut runner =
        ShowcaseRunner::new("proto-attr Compile Error Showcase").language(Language::Rust);
    runner.header();

    scenario_unknown_attribute(&mut runner);
    scenario_typo_skip(&mut runner);
    scenario_skip_with_args(&mut runner);
    scenario_rename_missing_value(&mut runner);
    scenario_column_unknown_field(&mut runner);
    scenario_column_name_missing_value(&mut runner);
    scenario_valid_usage(&mut runner);

    runner.footer();
}

/// Compiles a test snippet and returns the compiler error output.
fn compile_snippet(code: &str) -> String {
    use std::fs;
    use std::path::Path;

    let test_dir = Path::new("/tmp/proto-attr-compile-error-test");
    let src_dir = test_dir.join("src");

    fs::create_dir_all(&src_dir).unwrap();

    let manifest_dir = env!("CARGO_MANIFEST_DIR");
    let proto_attr_crates = Path::new(manifest_dir).parent().unwrap();
    let proto_attr_path = proto_attr_crates.join("proto-attr");
    let proto_ext_path = proto_attr_crates.join("proto-ext");

    fs::write(
        test_dir.join("Cargo.toml"),
        format!(
            r#"[package]
name = "test"
version = "0.1.0"
edition = "2024"

[dependencies]
proto-attr = {{ path = "{}" }}
proto-ext = {{ path = "{}" }}
"#,
            proto_attr_path.display(),
            proto_ext_path.display()
        ),
    )
    .unwrap();

    fs::write(src_dir.join("main.rs"), code).unwrap();

    let output = Command::new("cargo")
        .args(["check", "--color=always"])
        .current_dir(test_dir)
        .env("CARGO_TERM_COLOR", "always")
        .output()
        .expect("Failed to run cargo check");

    String::from_utf8_lossy(&output.stderr).to_string()
}

/// Extract just the error message from cargo output.
fn extract_error(output: &str) -> String {
    let mut lines: Vec<&str> = Vec::new();
    let mut in_error = false;

    for line in output.lines() {
        if line.contains("Compiling")
            || line.contains("Checking")
            || line.contains("Updating")
            || line.contains("Locking")
            || line.contains("Downloading")
            || line.contains("Downloaded")
        {
            continue;
        }

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
    let code = r#"use proto_attr::Faket;

#[derive(Faket)]
struct Config {
    #[faket(proto_ext::foobar)]
    field: String,
}

fn main() {}
"#;

    let output = compile_snippet(code);
    let error = extract_error(&output);

    runner
        .scenario("Unknown Extension Attribute")
        .description(
            "Using an unknown attribute like `proto_ext::foobar` produces a clear error\n\
             listing all available attributes.",
        )
        .input(Language::Rust, code)
        .compiler_error(&error)
        .finish();
}

fn scenario_typo_skip(runner: &mut ShowcaseRunner) {
    let code = r#"use proto_attr::Faket;

#[derive(Faket)]
#[faket(proto_ext::skp)]
struct Config {
    field: String,
}

fn main() {}
"#;

    let output = compile_snippet(code);
    let error = extract_error(&output);

    runner
        .scenario("Typo in Attribute Name")
        .description(
            "Common typos like `skp` instead of `skip` are caught at compile time\n\
             with a helpful \"did you mean?\" suggestion.",
        )
        .input(Language::Rust, code)
        .compiler_error(&error)
        .finish();
}

fn scenario_skip_with_args(runner: &mut ShowcaseRunner) {
    let code = r#"use proto_attr::Faket;

#[derive(Faket)]
#[faket(proto_ext::skip("unexpected"))]
struct Config {
    field: String,
}

fn main() {}
"#;

    let output = compile_snippet(code);
    let error = extract_error(&output);

    runner
        .scenario("Unit Attribute with Arguments")
        .description(
            "The `skip` attribute is a unit variant that takes no arguments.\n\
             Passing arguments produces a clear error explaining the correct usage.",
        )
        .input(Language::Rust, code)
        .compiler_error(&error)
        .finish();
}

fn scenario_rename_missing_value(runner: &mut ShowcaseRunner) {
    let code = r#"use proto_attr::Faket;

#[derive(Faket)]
#[faket(proto_ext::rename)]
struct Config {
    field: String,
}

fn main() {}
"#;

    let output = compile_snippet(code);
    let error = extract_error(&output);

    runner
        .scenario("Newtype Attribute Missing Value")
        .description(
            "The `rename` attribute requires a string value.\n\
             Omitting the value produces an error showing the expected syntax.",
        )
        .input(Language::Rust, code)
        .compiler_error(&error)
        .finish();
}

fn scenario_column_unknown_field(runner: &mut ShowcaseRunner) {
    let code = r#"use proto_attr::Faket;

#[derive(Faket)]
struct User {
    #[faket(proto_ext::column(nam = "user_id"))]
    id: i64,
}

fn main() {}
"#;

    let output = compile_snippet(code);
    let error = extract_error(&output);

    runner
        .scenario("Unknown Field in Struct Attribute")
        .description(
            "Typos in field names like `nam` instead of `name` are caught\n\
             with a \"did you mean?\" suggestion and list of valid fields.",
        )
        .input(Language::Rust, code)
        .compiler_error(&error)
        .finish();
}

fn scenario_column_name_missing_value(runner: &mut ShowcaseRunner) {
    let code = r#"use proto_attr::Faket;

#[derive(Faket)]
struct User {
    #[faket(proto_ext::column(name, primary_key))]
    id: i64,
}

fn main() {}
"#;

    let output = compile_snippet(code);
    let error = extract_error(&output);

    runner
        .scenario("Struct Field Missing Value")
        .description(
            "The `name` field in `column` requires a string value.\n\
             Using it as a flag produces an error showing the correct syntax.",
        )
        .input(Language::Rust, code)
        .compiler_error(&error)
        .finish();
}

fn scenario_valid_usage(runner: &mut ShowcaseRunner) {
    let code = r#"use proto_attr::Faket;

#[derive(Faket)]
#[faket(proto_ext::skip)]
struct SkippedStruct {
    field: String,
}

#[derive(Faket)]
#[faket(proto_ext::rename("NewName"))]
struct RenamedStruct {
    field: String,
}

#[derive(Faket)]
struct User {
    #[faket(proto_ext::column(name = "user_id", primary_key))]
    id: i64,

    #[faket(proto_ext::column(name = "user_name"))]
    name: String,

    #[faket(proto_ext::rename("email_address"))]
    email: String,
}

fn main() {
    println!("Compiles successfully!");
}
"#;

    let output = compile_snippet(code);

    let has_error = output.contains("error[") || output.contains("error:");
    let error_output = if has_error {
        extract_error(&output)
    } else {
        "✓ Compilation successful! No errors.".to_string()
    };

    runner
        .scenario("Valid Usage")
        .description(
            "When extension attributes are used correctly, everything compiles smoothly.\n\
             This shows the intended usage patterns for proto-ext attributes.",
        )
        .input(Language::Rust, code)
        .compiler_error(&error_output)
        .finish();
}
