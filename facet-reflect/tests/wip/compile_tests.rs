use std::fs;
use std::path::Path;

use owo_colors::OwoColorize;

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

/// Run a single compilation test that is expected to fail
fn run_compilation_test(test: &CompilationTest) {
    println!(
        "{}",
        format_args!("Running test: {}", test.name).blue().bold()
    );

    // Create a random temp directory for the Cargo project
    let temp_dir = tempfile::tempdir().expect("Failed to create temp directory");
    let project_dir = temp_dir.path();
    println!(
        "{}",
        format_args!("  Project directory: {}", project_dir.display()).dimmed()
    );

    // Get absolute paths to the facet crates
    let workspace_dir = Path::new(env!("CARGO_MANIFEST_DIR")).parent().unwrap();
    let facet_path = workspace_dir.join("facet");
    let facet_reflect_path = workspace_dir.join("facet-reflect");

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
eyre = "0.6"
facet = {{ path = "{}" }}
facet-reflect = {{ path = "{}" }}
    "#,
        facet_path.display(),
        facet_reflect_path.display()
    );

    // Write the Cargo.toml file
    fs::write(project_dir.join("Cargo.toml"), cargo_toml).expect("Failed to write Cargo.toml");

    // Write the main.rs file
    fs::write(project_dir.join("src").join("main.rs"), test.source)
        .expect("Failed to write main.rs");

    // Run cargo build
    let mut cmd = std::process::Command::new("cargo");
    cmd.current_dir(project_dir)
        .args(["build", "--color=always"])
        .env("CARGO_TERM_COLOR", "always")
        .env("CARGO_TARGET_DIR", "/tmp/ui_tests/target"); // Set consistent target directory

    let output = cmd.output().expect("Failed to execute cargo build");

    // Check if compilation failed (as expected)
    let exit_code = output.status.code().unwrap_or(0);
    let stderr = String::from_utf8_lossy(&output.stderr);
    let stdout = String::from_utf8_lossy(&output.stdout);

    // Strip ANSI escape sequences for error matching while preserving original for display
    let stderr_clean = strip_ansi_escapes(&stderr);

    // Verify the compilation failed as expected
    if exit_code == 0 {
        println!("{}", "❌ Test failed:".bright_red().bold());
        println!(
            "{}",
            "  The code compiled successfully, but it should have failed".red()
        );
        panic!(
            "Test '{}' compiled successfully but should have failed",
            test.name
        );
    } else {
        println!("{}", "  ✓ Compilation failed as expected".green());
    }

    // Check for expected error messages
    let mut missing_errors = Vec::new();
    for &expected_error in test.expected_errors {
        if !stderr_clean.contains(expected_error) {
            missing_errors.push(expected_error);
        } else {
            println!(
                "{}",
                format_args!("  ✓ Found expected error: '{}'", expected_error).green()
            );
        }
    }

    // Report any missing expected errors
    if !missing_errors.is_empty() {
        println!("{}", "\n❌ MISSING EXPECTED ERRORS:".bright_red().bold());
        for error in &missing_errors {
            println!("{}", format_args!("  - '{}'", error).red());
        }

        // Print the error output for debugging
        println!("{}", "\nCompiler error output:".yellow().bold());
        println!("{}", stderr);

        if !stdout.is_empty() {
            println!("{}", "\nCompiler standard output:".yellow());
            println!("{}", stdout);
        }

        panic!(
            "Test '{}' did not produce the expected error messages: {:?}",
            test.name, missing_errors
        );
    }

    println!(
        "{}",
        format_args!("  ✓ Test '{}' passed", test.name)
            .green()
            .bold()
    );
}

/// Test for lifetime issues in Poke implementation
#[test]
#[cfg(not(miri))]
fn test_poke_lifetime_error() {
    // Define the test case
    let test = CompilationTest {
        name: "poke_lifetime_error",
        source: include_str!("compile_tests/lifetimes.rs"),
        expected_errors: &["error[E0597]: `s` does not live long enough"],
    };

    // Run the test
    run_compilation_test(&test);
}
