//! Compile-time error tests for proto-attr.
//!
//! These tests verify that the attribute grammar system produces
//! helpful compile-time error messages.

#![cfg(feature = "slow-tests")]

use std::collections::hash_map::DefaultHasher;
use std::fs;
use std::hash::{Hash, Hasher};
use std::path::Path;

/// Test case structure for compilation tests
struct CompilationTest {
    /// Source code to compile
    source: &'static str,
    /// Expected error messages to find in the output
    expected_errors: &'static [&'static str],
    /// Name of the test for reporting purposes
    name: &'static str,
    /// Whether the test should compile successfully (false = should fail)
    should_compile: bool,
}

/// Strips ANSI escape sequences from a string
fn strip_ansi_escapes(s: &str) -> String {
    let mut result = String::with_capacity(s.len());
    let mut chars = s.chars().peekable();

    while let Some(c) = chars.next() {
        if c == '\x1B' {
            if let Some(&'[') = chars.peek() {
                chars.next();
                for c in chars.by_ref() {
                    if c.is_ascii_alphabetic() || c == 'm' {
                        break;
                    }
                }
            } else {
                result.push(c);
            }
        } else {
            result.push(c);
        }
    }
    result
}

/// Calculate a hash for the source code to create a unique target directory
fn hash_source(name: &str, source: &str) -> u64 {
    let mut hasher = DefaultHasher::new();
    name.hash(&mut hasher);
    source.hash(&mut hasher);
    hasher.finish()
}

/// Run a single compilation test
fn run_compilation_test(test: &CompilationTest) {
    println!("Running test: {}", test.name);

    let temp_dir = tempfile::tempdir().expect("Failed to create temp directory");
    let project_dir = temp_dir.path();
    println!("  Project directory: {}", project_dir.display());

    // Get absolute paths to the proto-attr crates
    let workspace_dir = Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .parent()
        .unwrap();
    let proto_attr_path = workspace_dir.join("crates/proto-attr");
    let proto_ext_path = workspace_dir.join("crates/proto-ext");

    fs::create_dir(project_dir.join("src")).expect("Failed to create src directory");

    let cargo_toml = format!(
        r#"
[package]
name = "proto-attr-test-project"
version = "0.1.0"
edition = "2024"

[dependencies]
proto-attr = {{ path = {:?} }}
proto-ext = {{ path = {:?} }}
"#,
        proto_attr_path.display(),
        proto_ext_path.display()
    );

    fs::write(project_dir.join("Cargo.toml"), cargo_toml).expect("Failed to write Cargo.toml");
    fs::write(project_dir.join("src").join("main.rs"), test.source)
        .expect("Failed to write main.rs");

    let source_hash = hash_source(test.name, test.source);
    let target_dir = format!("/tmp/ui_tests/proto_attr_target_{source_hash}");
    println!("  Target directory: {target_dir}");

    let mut cmd = std::process::Command::new("cargo");
    cmd.current_dir(project_dir)
        .args(["build", "--color=always"])
        .env("CARGO_TERM_COLOR", "always")
        .env("CARGO_TARGET_DIR", &target_dir);

    let output = cmd.output().expect("Failed to execute cargo build");

    let exit_code = output.status.code().unwrap_or(0);
    let stderr = String::from_utf8_lossy(&output.stderr);
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr_clean = strip_ansi_escapes(&stderr);

    if test.should_compile {
        if exit_code != 0 {
            println!("❌ Test failed:");
            println!("  The code failed to compile, but it should have succeeded");
            println!("\nCompiler error output:");
            println!("{stderr}");
            panic!(
                "Test '{}' failed to compile but should have succeeded",
                test.name
            );
        }
        println!("  ✓ Compilation succeeded as expected");
    } else {
        if exit_code == 0 {
            println!("❌ Test failed:");
            println!("  The code compiled successfully, but it should have failed");
            panic!(
                "Test '{}' compiled successfully but should have failed",
                test.name
            );
        }
        println!("  ✓ Compilation failed as expected");
    }

    let mut missing_errors = Vec::new();
    for &expected_error in test.expected_errors {
        if !stderr_clean.contains(expected_error) {
            missing_errors.push(expected_error);
        } else {
            println!("  ✓ Found expected error: '{expected_error}'");
        }
    }

    if !missing_errors.is_empty() {
        println!("\n❌ MISSING EXPECTED ERRORS:");
        for error in &missing_errors {
            println!("  - '{error}'");
        }

        println!("\nCompiler error output:");
        println!("{stderr}");

        if !stdout.is_empty() {
            println!("\nCompiler standard output:");
            println!("{stdout}");
        }

        panic!(
            "Test '{}' did not produce the expected error messages: {:?}",
            test.name, missing_errors
        );
    }

    println!("\nCompiler output:");
    println!("{stderr}");

    println!("  ✓ Test '{}' passed", test.name);
}

// =============================================================================
// VALID TESTS (should compile)
// =============================================================================

#[test]
fn test_derive_valid_skip() {
    run_compilation_test(&CompilationTest {
        name: "derive_valid_skip",
        source: include_str!("compile_tests/derive_valid_skip.rs"),
        expected_errors: &[],
        should_compile: true,
    });
}

#[test]
fn test_derive_valid_rename() {
    run_compilation_test(&CompilationTest {
        name: "derive_valid_rename",
        source: include_str!("compile_tests/derive_valid_rename.rs"),
        expected_errors: &[],
        should_compile: true,
    });
}

#[test]
fn test_derive_valid_column() {
    run_compilation_test(&CompilationTest {
        name: "derive_valid_column",
        source: include_str!("compile_tests/derive_valid_column.rs"),
        expected_errors: &[],
        should_compile: true,
    });
}

// =============================================================================
// ERROR TESTS (should fail with helpful messages)
// =============================================================================

#[test]
fn test_derive_unknown_attr_typo() {
    run_compilation_test(&CompilationTest {
        name: "derive_unknown_attr_typo",
        source: include_str!("compile_tests/derive_unknown_attr_typo.rs"),
        expected_errors: &["unknown attribute", "did you mean `skip`"],
        should_compile: false,
    });
}

#[test]
fn test_derive_skip_with_args() {
    run_compilation_test(&CompilationTest {
        name: "derive_skip_with_args",
        source: include_str!("compile_tests/derive_skip_with_args.rs"),
        expected_errors: &["`skip` does not take arguments"],
        should_compile: false,
    });
}

#[test]
fn test_derive_rename_missing_value() {
    run_compilation_test(&CompilationTest {
        name: "derive_rename_missing_value",
        source: include_str!("compile_tests/derive_rename_missing_value.rs"),
        expected_errors: &["`rename` requires a string value"],
        should_compile: false,
    });
}

#[test]
fn test_derive_column_unknown_field() {
    run_compilation_test(&CompilationTest {
        name: "derive_column_unknown_field",
        source: include_str!("compile_tests/derive_column_unknown_field.rs"),
        expected_errors: &["unknown field", "did you mean `name`"],
        should_compile: false,
    });
}

#[test]
fn test_derive_column_name_missing_value() {
    run_compilation_test(&CompilationTest {
        name: "derive_column_name_missing_value",
        source: include_str!("compile_tests/derive_column_name_missing_value.rs"),
        expected_errors: &["`name` requires a string value"],
        should_compile: false,
    });
}
