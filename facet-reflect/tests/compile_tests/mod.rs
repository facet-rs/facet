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

/// Test for lifetime issues in Poke implementation
#[test]
#[cfg(not(miri))]
fn test_partial_poke_lifetime_error() {
    let test = CompilationTest {
        name: "poke_lifetime_error",
        source: include_str!("fixtures/lifetimes.rs"),
        expected_errors: &["error[E0597]: `s` does not live long enough"],
    };

    run_compilation_test(&test);
}

#[test]
#[cfg(not(miri))]
fn test_partial_covariant_growing() {
    let test = CompilationTest {
        name: "covariant_growing",
        source: include_str!("fixtures/partial_covariant_growing.rs"),
        // Either E0521 or "lifetime may not live long enough" depending on Rust version
        expected_errors: &["lifetime may not live long enough"],
    };

    run_compilation_test(&test);
}

#[test]
#[cfg(not(miri))]
fn test_partial_invariant_growing() {
    let test = CompilationTest {
        name: "invariant_growing",
        source: include_str!("fixtures/partial_invariant_growing.rs"),
        // Either E0521 or "lifetime may not live long enough" depending on Rust version
        expected_errors: &["lifetime may not live long enough"],
    };

    run_compilation_test(&test);
}

#[test]
#[cfg(not(miri))]
fn test_partial_contravariant_shrinking() {
    let test = CompilationTest {
        name: "contravariant_shrinking",
        source: include_str!("fixtures/partial_contravariant_shrinking.rs"),
        expected_errors: &["error[E0521]: borrowed data escapes outside of function"],
    };

    run_compilation_test(&test);
}

#[test]
#[cfg(not(miri))]
fn test_partial_invariant_shrinking() {
    let test = CompilationTest {
        name: "invariant_shrinking",
        source: include_str!("fixtures/partial_invariant_shrinking.rs"),
        expected_errors: &["error[E0521]: borrowed data escapes outside of function"],
    };

    run_compilation_test(&test);
}

#[test]
#[cfg(not(miri))]
fn test_peek_covariant_growing() {
    let test = CompilationTest {
        name: "covariant_growing",
        source: include_str!("fixtures/peek_covariant_growing.rs"),
        expected_errors: &["error[E0521]: borrowed data escapes outside of function"],
    };

    run_compilation_test(&test);
}

#[test]
#[cfg(not(miri))]
fn test_peek_invariant_growing() {
    let test = CompilationTest {
        name: "invariant_growing",
        source: include_str!("fixtures/peek_invariant_growing.rs"),
        expected_errors: &["error[E0521]: borrowed data escapes outside of function"],
    };

    run_compilation_test(&test);
}

#[test]
#[cfg(not(miri))]
fn test_peek_contravariant_shrinking() {
    let test = CompilationTest {
        name: "contravariant_shrinking",
        source: include_str!("fixtures/peek_contravariant_shrinking.rs"),
        // Depending on Rust version / platform, error can be either:
        // - E0521 (borrowed data escapes) - the original invariance error
        // - E0515 (cannot return value referencing temporary) - covariant error
        // Either one is acceptable as long as the code fails to compile.
        expected_errors: &["error[E0521]: borrowed data escapes outside of function"],
    };

    run_compilation_test(&test);
}

#[test]
#[cfg(not(miri))]
fn test_peek_invariant_shrinking() {
    let test = CompilationTest {
        name: "invariant_shrinking",
        source: include_str!("fixtures/peek_invariant_shrinking.rs"),
        expected_errors: &["error[E0521]: borrowed data escapes outside of function"],
    };

    run_compilation_test(&test);
}

#[test]
#[cfg(not(miri))]
fn test_peek_owned_dropped_before_ref() {
    let test = CompilationTest {
        name: "owned_dropped_before_ref",
        source: include_str!("fixtures/owned_dropped_before_ref.rs"),
        expected_errors: &["error[E0505]: cannot move out of `owned` because it is borrowed"],
    };

    run_compilation_test(&test);
}

/// Soundness test: ensures the fn pointer UB exploit is prevented.
///
/// Before making Peek invariant with respect to 'facet, code like this would compile
/// and lead to use-after-free (miri would detect it). Now it should fail
/// to compile because Peek is invariant with respect to 'facet.
///
/// See: https://github.com/facet-rs/facet/issues/1168
#[test]
#[cfg(not(miri))]
fn test_peek_fn_ptr_ub_exploit() {
    let test = CompilationTest {
        name: "fn_ptr_ub_exploit",
        source: include_str!("fixtures/fn_ptr_ub_exploit.rs"),
        expected_errors: &["error[E0521]: borrowed data escapes outside of function"],
    };

    run_compilation_test(&test);
}

/// Soundness test for GitHub issue #1555
///
/// Before the fix, OxRef::new was safe but accepted arbitrary pointers,
/// allowing use-after-free through safe code like PartialEq.
///
/// After the fix, OxRef::new is unsafe, so this code should fail to compile.
///
/// See: https://github.com/facet-rs/facet/issues/1555
#[test]
#[cfg(not(miri))]
fn test_oxref_unsound_from_raw_ptr() {
    let test = CompilationTest {
        name: "oxref_unsound_from_raw_ptr",
        source: include_str!("fixtures/oxref_unsound_from_raw_ptr.rs"),
        expected_errors: &["call to unsafe function `OxRef::<'a>::new` is unsafe"],
    };

    run_compilation_test(&test);
}

/// Soundness test for GitHub issue #1563
///
/// After the fix, Opaque<T> requires T: 'static, so borrowed references
/// are rejected and lifetime laundering through Poke is prevented.
#[test]
#[cfg(not(miri))]
fn test_poke_opaque_insufficient_lifetime() {
    let test = CompilationTest {
        name: "opaque_insufficient_lifetime",
        source: include_str!("fixtures/opaque_insufficient_lifetime.rs"),
        expected_errors: &["does not live long enough"],
    };

    run_compilation_test(&test);
}

/// Opaque fields currently require 'static.
#[test]
#[cfg(not(miri))]
fn test_poke_opaque_borrowed_non_facet() {
    let test = CompilationTest {
        name: "opaque_borrowed_non_facet",
        source: include_str!("fixtures/opaque_borrowed_non_facet.rs"),
        expected_errors: &["lifetime may not live long enough"],
    };

    run_compilation_test(&test);
}

/// Soundness test for GitHub issue #1663
///
/// Before the fix, Partial::alloc_shape() was safe but accepted untrusted shapes,
/// allowing UB when the shape didn't match the type being materialized.
///
/// After the fix, Partial::alloc_shape() is unsafe, so this code should fail to compile.
///
/// See: https://github.com/facet-rs/facet/issues/1663
#[test]
#[cfg(not(miri))]
fn test_partial_untrusted_shape() {
    let test = CompilationTest {
        name: "untrusted_shape",
        source: include_str!("fixtures/untrusted_shape.rs"),
        expected_errors: &["call to unsafe function"],
    };

    run_compilation_test(&test);
}

/// Soundness test for GitHub issue #1573
///
/// Before the fix, Attr could store non-Sync data like Rc<T>, but Attr itself
/// was Sync, allowing data races when accessed from multiple threads.
///
/// After the fix, Attr::new requires T: Sync, so this code should fail to compile.
///
/// See: https://github.com/facet-rs/facet/issues/1573
#[test]
#[cfg(not(miri))]
fn test_attr_non_sync_data() {
    let test = CompilationTest {
        name: "attr_non_sync_data",
        source: include_str!("fixtures/non_sync_data.rs"),
        expected_errors: &["Rc<i32>` cannot be shared between threads safely"],
    };

    run_compilation_test(&test);
}

/// Soundness test for GitHub issues #1665, #1684, #1685
///
/// Before the fix, PeekListLike::new(), PeekMap::new(), PeekSet::new(), PeekList::new(),
/// and PeekNdArray::new() were safe but accepted untrusted vtables, allowing UB when
/// those vtables had malicious function pointers.
///
/// After the fix, these constructors are unsafe, so this code should fail to compile.
///
/// See: https://github.com/facet-rs/facet/issues/1665
/// See: https://github.com/facet-rs/facet/issues/1684
/// See: https://github.com/facet-rs/facet/issues/1685
#[test]
#[cfg(not(miri))]
fn test_peek_untrusted_vtable() {
    let test = CompilationTest {
        name: "peek_untrusted_vtable",
        source: include_str!("fixtures/untrusted_vtable.rs"),
        expected_errors: &["call to unsafe function"],
    };

    run_compilation_test(&test);
}
