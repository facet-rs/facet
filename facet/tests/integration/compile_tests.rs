//! Compile tests for facet macro error diagnostics.
//!
//! These tests verify that macro error messages point to the correct source locations,
//! enabling IDE features like hover, go-to-definition, and proper error highlighting.

#![cfg(not(miri))]
#![cfg(feature = "slow-tests")]

use std::collections::hash_map::DefaultHasher;
use std::fs;
use std::hash::{Hash, Hasher};
use std::path::Path;

use facet_testhelpers::test;

/// Test case structure for compilation tests
struct CompilationTest {
    /// Source code to compile
    source: &'static str,
    /// Expected error messages to find in the output
    expected_errors: &'static [&'static str],
    /// Name of the test for reporting purposes
    name: &'static str,
}

/// Strips ANSI escape sequences from a string
fn strip_ansi_escapes(s: &str) -> String {
    let mut result = String::with_capacity(s.len());
    let mut chars = s.chars().peekable();

    while let Some(c) = chars.next() {
        if c == '\x1B' {
            if let Some(&'[') = chars.peek() {
                chars.next(); // consume '['
                // Skip until we find the end of the sequence
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

/// Run a single compilation test that is expected to fail
fn run_compilation_test(test: &CompilationTest) {
    println!("{}", format_args!("Running test: {}", test.name));

    // Create a random temp directory for the Cargo project
    let temp_dir = tempfile::tempdir().expect("Failed to create temp directory");
    let project_dir = temp_dir.path();
    println!(
        "{}",
        format_args!("  Project directory: {}", project_dir.display())
    );

    // Get absolute paths to the facet crates
    let workspace_dir = Path::new(env!("CARGO_MANIFEST_DIR")).parent().unwrap();
    let facet_path = workspace_dir.join("facet");

    // Create src directory
    fs::create_dir(project_dir.join("src")).expect("Failed to create src directory");

    // Create Cargo.toml with dependencies
    let cargo_toml = format!(
        r#"
[package]
name = "facet-test-project"
version = "0.1.0"
edition = "2021"

[dependencies]
facet = {{ path = {:?} }}
    "#,
        facet_path.display(),
    );

    // Write the Cargo.toml file
    fs::write(project_dir.join("Cargo.toml"), cargo_toml).expect("Failed to write Cargo.toml");

    // Write the main.rs file
    fs::write(project_dir.join("src").join("main.rs"), test.source)
        .expect("Failed to write main.rs");

    // Generate a unique target directory based on the test name and source code
    let source_hash = hash_source(test.name, test.source);
    let target_dir = format!("/tmp/ui_tests/target_{source_hash}");
    println!("{}", format_args!("  Target directory: {target_dir}"));

    // Run cargo build
    let mut cmd = std::process::Command::new("cargo");
    cmd.current_dir(project_dir)
        .args(["build", "--color=always"])
        .env("CARGO_TERM_COLOR", "always")
        .env("CARGO_TARGET_DIR", &target_dir); // Use source-hash based target directory

    let output = cmd.output().expect("Failed to execute cargo build");

    // Check if compilation failed (as expected)
    let exit_code = output.status.code().unwrap_or(0);
    let stderr = String::from_utf8_lossy(&output.stderr);
    let stdout = String::from_utf8_lossy(&output.stdout);

    // Strip ANSI escape sequences for error matching while preserving original for display
    let stderr_clean = strip_ansi_escapes(&stderr);

    // Verify the compilation failed as expected
    if exit_code == 0 {
        println!("❌ Test failed:");
        println!("  The code compiled successfully, but it should have failed");
        panic!(
            "Test '{}' compiled successfully but should have failed",
            test.name
        );
    } else {
        println!("  ✓ Compilation failed as expected");
    }

    // Check for expected error messages
    let mut missing_errors = Vec::new();
    for &expected_error in test.expected_errors {
        if !stderr_clean.contains(expected_error) {
            missing_errors.push(expected_error);
        } else {
            println!("  ✓ Found expected error: '{expected_error}'");
        }
    }

    // Report any missing expected errors
    if !missing_errors.is_empty() {
        println!("\n❌ MISSING EXPECTED ERRORS:");
        for error in &missing_errors {
            println!("  - '{error}'");
        }

        // Print the error output for debugging
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

    println!("{}", format_args!("  ✓ Test '{}' passed", test.name));
}

/// Test that proxy attribute errors point to the correct span.
///
/// The error should point to `NonExistentProxyType`, not the macro expansion site.
#[test]
#[cfg(not(miri))]
fn test_proxy_unknown_type() {
    let test = CompilationTest {
        name: "proxy_unknown_type",
        source: include_str!("../compile_tests/proxy_unknown_type.rs"),
        // The error code can vary between rustc versions (E0412 vs E0425)
        // but the important thing is the error points to the right location
        expected_errors: &["NonExistentProxyType", "not found in this scope"],
    };

    run_compilation_test(&test);
}

/// Test that default attribute errors point to the correct span.
///
/// The error should point to `MissingDefault::create()`, not the macro expansion site.
#[test]
#[cfg(not(miri))]
fn test_default_unknown_expr() {
    let test = CompilationTest {
        name: "default_unknown_expr",
        source: include_str!("../compile_tests/default_unknown_expr.rs"),
        expected_errors: &["MissingDefault", "undeclared"],
    };

    run_compilation_test(&test);
}

/// Test that invariants attribute errors point to the correct span.
///
/// The error should point to `missing_validator`, not the macro expansion site.
#[test]
#[cfg(not(miri))]
fn test_invariants_unknown_fn() {
    let test = CompilationTest {
        name: "invariants_unknown_fn",
        source: include_str!("../compile_tests/invariants_unknown_fn.rs"),
        expected_errors: &["missing_validator", "not found in this scope"],
    };

    run_compilation_test(&test);
}

/// Test that skip_serializing_if attribute errors point to the correct span.
///
/// The error should point to `nonexistent_predicate`, not the macro expansion site.
#[test]
#[cfg(not(miri))]
fn test_skip_serializing_if_unknown_fn() {
    let test = CompilationTest {
        name: "skip_serializing_if_unknown_fn",
        source: include_str!("../compile_tests/skip_serializing_if_unknown_fn.rs"),
        expected_errors: &["nonexistent_predicate", "not found in this scope"],
    };

    run_compilation_test(&test);
}

/// Test that truthy attribute errors point to the correct span.
///
/// The error should point to `missing_truthy_fn`, not the macro expansion site.
#[test]
#[cfg(not(miri))]
fn test_truthy_unknown_fn() {
    let test = CompilationTest {
        name: "truthy_unknown_fn",
        source: include_str!("../compile_tests/truthy_unknown_fn.rs"),
        expected_errors: &["missing_truthy_fn", "not found in this scope"],
    };

    run_compilation_test(&test);
}
