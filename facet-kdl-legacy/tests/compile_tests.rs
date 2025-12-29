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

/// Run a single compilation test
fn run_compilation_test(test: &CompilationTest) {
    println!("Running test: {}", test.name);

    // Create a random temp directory for the Cargo project
    let temp_dir = tempfile::tempdir().expect("Failed to create temp directory");
    let project_dir = temp_dir.path();
    println!("  Project directory: {}", project_dir.display());

    // Get absolute paths to the facet crates
    let workspace_dir = Path::new(env!("CARGO_MANIFEST_DIR")).parent().unwrap();
    let facet_path = workspace_dir.join("facet");
    let facet_kdl_legacy_path = workspace_dir.join("facet-kdl-legacy");

    // Create src directory
    fs::create_dir(project_dir.join("src")).expect("Failed to create src directory");

    // Create Cargo.toml with dependencies
    let cargo_toml = format!(
        r#"
[package]
name = "facet-kdl-test-project"
version = "0.1.0"
edition = "2021"

[dependencies]
facet = {{ path = {:?} }}
facet-kdl-legacy = {{ path = {:?} }}
    "#,
        facet_path.display(),
        facet_kdl_legacy_path.display()
    );

    // Write the Cargo.toml file
    fs::write(project_dir.join("Cargo.toml"), cargo_toml).expect("Failed to write Cargo.toml");

    // Write the main.rs file
    fs::write(project_dir.join("src").join("main.rs"), test.source)
        .expect("Failed to write main.rs");

    // Generate a unique target directory based on the test name and source code
    let source_hash = hash_source(test.name, test.source);
    let target_dir = format!("/tmp/ui_tests/kdl_target_{source_hash}");
    println!("  Target directory: {target_dir}");

    // Run cargo build
    let mut cmd = std::process::Command::new("cargo");
    cmd.current_dir(project_dir)
        .args(["build", "--color=always"])
        .env("CARGO_TERM_COLOR", "always")
        .env("CARGO_TARGET_DIR", &target_dir);

    let output = cmd.output().expect("Failed to execute cargo build");

    // Check if compilation succeeded or failed
    let exit_code = output.status.code().unwrap_or(0);
    let stderr = String::from_utf8_lossy(&output.stderr);
    let stdout = String::from_utf8_lossy(&output.stdout);

    // Strip ANSI escape sequences for error matching while preserving original for display
    let stderr_clean = strip_ansi_escapes(&stderr);

    if test.should_compile {
        // Test should compile successfully
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
        // Test should fail to compile
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

    // Always show compiler output for inspection
    println!("\nCompiler error output:");
    println!("{stderr}");

    println!("  ✓ Test '{}' passed", test.name);
}

// =============================================================================
// Tests for valid KDL attributes (should compile)
// =============================================================================

#[test]
#[cfg(not(miri))]
fn test_valid_kdl_child_compiles() {
    let test = CompilationTest {
        name: "valid_kdl_child",
        source: include_str!("compile_tests/valid_kdl_child.rs"),
        expected_errors: &[],
        should_compile: true,
    };
    run_compilation_test(&test);
}

#[test]
#[cfg(not(miri))]
fn test_valid_kdl_argument_compiles() {
    let test = CompilationTest {
        name: "valid_kdl_argument",
        source: include_str!("compile_tests/valid_kdl_argument.rs"),
        expected_errors: &[],
        should_compile: true,
    };
    run_compilation_test(&test);
}

#[test]
#[cfg(not(miri))]
fn test_valid_kdl_property_compiles() {
    let test = CompilationTest {
        name: "valid_kdl_property",
        source: include_str!("compile_tests/valid_kdl_property.rs"),
        expected_errors: &[],
        should_compile: true,
    };
    run_compilation_test(&test);
}

// =============================================================================
// Tests for invalid KDL attributes (should fail to compile with helpful errors)
// =============================================================================

#[test]
#[cfg(not(miri))]
fn test_invalid_kdl_attr_fails_to_compile() {
    let test = CompilationTest {
        name: "invalid_kdl_nonexistent",
        source: include_str!("compile_tests/invalid_kdl_nonexistent.rs"),
        expected_errors: &["unknown attribute"],
        should_compile: false,
    };
    run_compilation_test(&test);
}

#[test]
#[cfg(not(miri))]
fn test_typo_kdl_chld_fails_to_compile() {
    let test = CompilationTest {
        name: "typo_kdl_chld",
        source: include_str!("compile_tests/typo_kdl_chld.rs"),
        // New error format includes typo suggestion
        expected_errors: &["unknown attribute", "did you mean"],
        should_compile: false,
    };
    run_compilation_test(&test);
}
