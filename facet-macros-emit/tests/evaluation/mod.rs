use std::fs;
use std::path::Path;

use owo_colors::OwoColorize;

/// Test case structure for evaluation tests
struct EvaluationTest {
    /// Source code to compile
    source: &'static str,
    /// Name of the test for reporting purposes
    name: &'static str,
}

/// Run a single evaluation test that is expected to compile and run successfully
fn run_evaluation_test(test: &EvaluationTest) {
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
facet = {{ path = {:?} }}
facet-reflect = {{ path = {:?} }}
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
    let mut build_cmd = std::process::Command::new("cargo");
    build_cmd
        .current_dir(project_dir)
        .args(["build", "--color=always"])
        .env("CARGO_TERM_COLOR", "always")
        .env("CARGO_TARGET_DIR", "/tmp/ui_tests/target"); // Set consistent target directory

    let build_output = build_cmd.output().expect("Failed to execute cargo build");

    // Check if compilation succeeded
    let build_exit_code = build_output.status.code().unwrap_or(1);
    let build_stderr = String::from_utf8_lossy(&build_output.stderr);
    let build_stdout = String::from_utf8_lossy(&build_output.stdout);

    if build_exit_code != 0 {
        println!("{}", "❌ Test failed:".bright_red().bold());
        println!(
            "{}",
            "  The code failed to compile, but it should have succeeded".red()
        );

        // Print the build output for debugging
        println!("{}", "\nCompiler error output:".yellow().bold());
        println!("{build_stderr}");

        if !build_stdout.is_empty() {
            println!("{}", "\nCompiler standard output:".yellow());
            println!("{build_stdout}");
        }

        panic!(
            "Test '{}' failed to compile with exit code {}",
            test.name, build_exit_code
        );
    } else {
        println!("{}", "  ✓ Compilation succeeded".green());
    }

    // Run the compiled program
    let mut run_cmd = std::process::Command::new("cargo");
    run_cmd
        .current_dir(project_dir)
        .args(["run", "--color=always"])
        .env("CARGO_TERM_COLOR", "always")
        .env("CARGO_TARGET_DIR", "/tmp/ui_tests/target");

    let run_output = run_cmd.output().expect("Failed to execute cargo run");

    // Check if the program ran successfully
    let run_exit_code = run_output.status.code().unwrap_or(1);
    let run_stderr = String::from_utf8_lossy(&run_output.stderr);
    let run_stdout = String::from_utf8_lossy(&run_output.stdout);

    if run_exit_code != 0 {
        println!("{}", "❌ Test failed:".bright_red().bold());
        println!(
            "{}",
            format_args!("  The program exited with non-zero status code: {run_exit_code}").red()
        );

        // Print the runtime output for debugging
        if !run_stdout.is_empty() {
            println!("{}", "\nProgram standard output:".yellow().bold());
            println!("{run_stdout}");
        }

        if !run_stderr.is_empty() {
            println!("{}", "\nProgram error output:".yellow().bold());
            println!("{run_stderr}");
        }

        panic!(
            "Test '{}' exited with non-zero status code: {}",
            test.name, run_exit_code
        );
    } else {
        println!("{}", "  ✓ Program ran successfully".green());
    }

    // Print output if present (for informational purposes)
    if !run_stdout.is_empty() {
        println!(
            "{}",
            format_args!("  Program output:\n{}", run_stdout).dimmed()
        );
    }

    println!(
        "{}",
        format_args!("  ✓ Test '{}' passed", test.name)
            .green()
            .bold()
    );
}

#[test]
fn test_single_quotes() {
    // Define the test case
    let test = EvaluationTest {
        name: "single_quotes",
        source: include_str!("./single_quotes.rs"),
    };

    // Run the test
    run_evaluation_test(&test);
}

#[test]
fn test_double_quotes() {
    // Define the test case
    let test = EvaluationTest {
        name: "double_quotes",
        source: include_str!("./double_quotes.rs"),
    };

    // Run the test
    run_evaluation_test(&test);
}

#[test]
fn test_backslash() {
    // Define the test case
    let test = EvaluationTest {
        name: "backslash_test",
        source: include_str!("./backslash_test.rs"),
    };

    // Run the test
    run_evaluation_test(&test);
}

#[test]
fn test_complex_doc() {
    // Define the test case
    let test = EvaluationTest {
        name: "complex_doc",
        source: include_str!("./complex_doc.rs"),
    };

    // Run the test
    run_evaluation_test(&test);
}
