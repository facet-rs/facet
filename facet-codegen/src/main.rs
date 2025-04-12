use std::io::Write;
use std::path::Path;
use std::process;

mod gen_tuples_impls;
mod readmes;

use facet_ansi::Stylize as _;
use log::{error, info, warn};
use similar::{ChangeTag, TextDiff};

fn main() {
    facet_testhelpers::setup();

    let opts = Options {
        check: std::env::args().any(|arg| arg == "--check"),
    };
    let mut has_diffs = false;

    // Check if current directory has a Cargo.toml with [workspace]
    let cargo_toml_path = std::env::current_dir().unwrap().join("Cargo.toml");
    let cargo_toml_content =
        fs_err::read_to_string(cargo_toml_path).expect("Failed to read Cargo.toml");
    if !cargo_toml_content.contains("[workspace]") {
        error!("🚫 {}", "Cargo.toml does not contain [workspace] (you must run codegen from the workspace root)".red());
        panic!();
    }

    // Run all three code generation tasks in parallel
    let opts_clone1 = opts.clone();
    let tuple_impls_result = std::thread::spawn(move || {
        let mut local_has_diffs = false;
        generate_tuple_impls(&mut local_has_diffs, opts_clone1);
        local_has_diffs
    });

    let opts_clone2 = opts.clone();
    let readme_had_diffs = std::thread::spawn(move || readmes::generate_readme_files(opts_clone2));

    let opts_clone3 = opts.clone();
    let sample_code_result = std::thread::spawn(move || {
        let mut local_has_diffs = false;
        copy_cargo_expand_output(&mut local_has_diffs, &opts_clone3);
        local_has_diffs
    });

    // Collect results and update has_diffs
    has_diffs |= tuple_impls_result
        .join()
        .expect("tuple_impls thread panicked");
    has_diffs |= readme_had_diffs
        .join()
        .expect("readme_files thread panicked");
    has_diffs |= sample_code_result
        .join()
        .expect("sample_code thread panicked");

    if opts.check && has_diffs {
        // Print a big banner with error message about generated files
        error!("┌────────────────────────────────────────────────────────────────────────────┐");
        error!("│                                                                            │");
        error!("│  GENERATED FILES HAVE CHANGED - RUN `just codegen` TO UPDATE THEM          │");
        error!("│                                                                            │");
        error!("│  For README.md files:                                                      │");
        error!("│                                                                            │");
        error!("│  • Don't edit README.md directly - edit the README.md.in template instead  │");
        error!("│  • Then run `just codegen` to regenerate the README.md files               │");
        error!("│  • A pre-commit hook is set up by cargo-husky to do just that              │");
        error!("│                                                                            │");
        error!("│  See CONTRIBUTING.md                                                       │");
        error!("│                                                                            │");
        error!("└────────────────────────────────────────────────────────────────────────────┘");
        process::exit(1);
    }
}

fn copy_cargo_expand_output(has_diffs: &mut bool, opts: &Options) {
    let workspace_dir = std::env::current_dir().unwrap();
    let sample_dir = workspace_dir.join("sample");

    // Run cargo expand command and measure execution time
    let start_time = std::time::Instant::now();

    // Command 1: cargo rustc for expansion
    let cargo_expand_output = std::process::Command::new("cargo")
        .env("RUSTC_BOOTSTRAP", "1") // Necessary for -Z flags
        .current_dir(&sample_dir) // Set working directory instead of changing it
        .arg("rustc")
        .arg("--target-dir")
        .arg("/tmp/facet-codegen-expand") // Use a temporary, less intrusive target dir
        .arg("--lib") // Expand the library crate in the current directory
        .arg("--") // Separator for rustc flags
        .arg("-Zunpretty=expanded") // The flag to expand macros
        .output() // Execute and capture output
        .expect("Failed to execute cargo rustc for expansion");

    // Check if cargo rustc succeeded
    if !cargo_expand_output.status.success() {
        error!(
            "🚫 {}:\n--- stderr ---\n{}\n--- stdout ---\n{}",
            "cargo rustc expansion failed".red(),
            String::from_utf8_lossy(&cargo_expand_output.stderr).trim(),
            String::from_utf8_lossy(&cargo_expand_output.stdout).trim()
        );
        std::process::exit(1);
    }

    // Prepare the code for rustfmt: prepend the necessary lines
    let expanded_code = String::from_utf8(cargo_expand_output.stdout)
        .expect("Failed to convert cargo expand output to UTF-8 string");

    // Command 2: rustfmt to format the expanded code
    let mut rustfmt_cmd = std::process::Command::new("rustfmt")
        .arg("--edition")
        .arg("2024")
        .arg("--emit")
        .arg("stdout")
        .stdin(std::process::Stdio::piped()) // Prepare to pipe stdin
        .stdout(std::process::Stdio::piped()) // Capture stdout
        .stderr(std::process::Stdio::piped()) // Capture stderr
        .spawn()
        .expect("Failed to spawn rustfmt");

    // Write the combined code to rustfmt's stdin in a separate scope
    // to ensure stdin is closed, signaling EOF to rustfmt.
    {
        let mut stdin = rustfmt_cmd
            .stdin
            .take()
            .expect("Failed to open rustfmt stdin");
        stdin
            .write_all(expanded_code.as_bytes())
            .expect("Failed to write to rustfmt stdin");
    } // stdin is closed here

    // Wait for rustfmt to finish and collect its output
    let output = rustfmt_cmd
        .wait_with_output()
        .expect("Failed to wait for rustfmt");

    // Check if rustfmt succeeded (using the final 'output' variable)
    // Note: The original code only checked the final status, which might hide
    // the cargo expand error if rustfmt succeeds. We now check both stages.
    if !output.status.success() {
        error!(
            "🚫 {}:\n--- stderr ---\n{}\n--- stdout ---\n{}",
            "rustfmt failed".red(),
            String::from_utf8_lossy(&output.stderr).trim(),
            String::from_utf8_lossy(&output.stdout).trim()
        );
        // We still need to check the final status for the rest of the function
        // but the process might have already exited if cargo expand failed.
        // If rustfmt itself fails, exit here.
        std::process::exit(1);
    }
    let execution_time = start_time.elapsed();

    if !output.status.success() {
        error!("🚫 {}", "Cargo expand command failed".red());
        std::process::exit(1);
    }

    let expanded_code =
        String::from_utf8(output.stdout).expect("Failed to convert output to string");

    // First collect doc comments, then filter out lines we don't want
    let doc_comments = expanded_code
        .lines()
        .filter(|line| line.trim_start().starts_with("//!"))
        .collect::<Vec<_>>()
        .join("\n");

    let expanded_code = expanded_code
        .lines()
        .filter(|line| {
            let trimmed = line.trim_start();
            !trimmed.starts_with("#![")
                && !trimmed.starts_with("#[facet(")
                && !trimmed.starts_with("#[macro_use]")
                && !trimmed.starts_with("//!")
        })
        .collect::<Vec<_>>()
        .join("\n");

    // Ensure a trailing newline for consistency
    let expanded_code = if expanded_code.is_empty() {
        String::new()
    } else {
        format!("{}\n", expanded_code)
    };

    // Replace any ::facet:: references with crate::
    let expanded_code = expanded_code.replace("::facet::", "crate::");
    let expanded_code = expanded_code.replace("use facet::", "use crate::");

    let expanded_code = format!("{}\n#![allow(warnings)]\n{}", doc_comments, expanded_code);

    let expanded_code = expanded_code.replace(
        "::impls::_core::marker::PhantomData",
        "::core::marker::PhantomData",
    );

    // Write the expanded code to the target file
    let target_path = workspace_dir
        .join("facet")
        .join("src")
        .join("sample_generated_code.rs");

    let was_different = write_if_different(&target_path, expanded_code.into_bytes(), opts.check);
    *has_diffs |= was_different;

    if opts.check {
        info!(
            "✅ Checked {} (took {:?})",
            "sample_generated_code.rs".blue().green(),
            execution_time
        );
    } else if was_different {
        info!(
            "🔧 Generated {} (took {:?})",
            "sample_generated_code.rs".blue().green(),
            execution_time
        );
    } else {
        info!(
            "✅ No changes to {} (took {:?})",
            "sample_generated_code.rs".blue().green(),
            execution_time
        );
    }
}

#[derive(Debug, Clone)]
struct Options {
    check: bool,
}

fn check_diff(path: &Path, new_content: &[u8]) -> bool {
    if !path.exists() {
        warn!(
            "📁 {}: {}",
            path.display(),
            "would create new file".yellow()
        );
        return true;
    }

    let old_content = fs_err::read(path).unwrap();
    if old_content != new_content {
        let old_str = String::from_utf8_lossy(&old_content);
        let new_str = String::from_utf8_lossy(new_content);

        let diff = TextDiff::from_lines(&old_str, &new_str);
        info!("📝 {}", format!("Diff for {}:", path.display()).blue());

        // Track consecutive equal lines
        let mut equal_count = 0;
        let mut last_tag = None;

        for change in diff.iter_all_changes() {
            let tag = change.tag();

            // If we're switching from Equal to another tag, and we have >=4 equal lines, show the count
            if last_tag == Some(ChangeTag::Equal) && tag != ChangeTag::Equal && equal_count > 3 {
                info!(" {} lines omitted.", equal_count - 1);
                equal_count = 0;
            }

            match tag {
                ChangeTag::Equal => {
                    if equal_count == 0 {
                        // Always show the first equal line
                        info!(" {}", change);
                    } else if equal_count < 3 {
                        // Show the 2nd and 3rd equal lines
                        info!(" {}", change);
                    }
                    equal_count += 1;
                }
                ChangeTag::Delete => {
                    equal_count = 0;
                    info!("-{}", change.red());
                }
                ChangeTag::Insert => {
                    equal_count = 0;
                    info!("+{}", change.green());
                }
            }

            last_tag = Some(tag);
        }

        // Handle case where diff ends with equal lines
        if last_tag == Some(ChangeTag::Equal) && equal_count > 3 {
            info!(" {} lines omitted.", equal_count - 1);
        }

        return true;
    }
    false
}

fn write_if_different(path: &Path, content: Vec<u8>, check_mode: bool) -> bool {
    let is_different = check_diff(path, &content);
    if check_mode {
        is_different
    } else if is_different {
        info!("Overwriting {} (had changes)", path.display().blue());
        fs_err::write(path, content).expect("Failed to write file");
        true
    } else {
        false
    }
}

fn generate_tuple_impls(has_diffs: &mut bool, opts: Options) {
    // Start timer to measure execution time
    let start_time = std::time::Instant::now();

    // Define the base path and template path
    let base_path = Path::new("facet-core/src/_trait/impls/tuples_impls.rs");

    let output = gen_tuples_impls::generate_tuples_impls();

    // Format the generated code using rustfmt
    let mut fmt = std::process::Command::new("rustfmt")
        .arg("--edition")
        .arg("2024")
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .spawn()
        .expect("Failed to spawn rustfmt");

    // Write to rustfmt's stdin
    fmt.stdin
        .take()
        .expect("Failed to get stdin")
        .write_all(output.as_bytes())
        .expect("Failed to write to rustfmt stdin");

    // Get formatted output
    let formatted_output = fmt.wait_with_output().expect("Failed to wait for rustfmt");
    if !formatted_output.status.success() {
        // Save the problematic output for inspection
        let _ = std::fs::write("/tmp/output.rs", &output);
        error!(
            "🚫 {} {}",
            "rustfmt failed to format the code.".red(),
            "The unformatted output has been saved to /tmp/output.rs for inspection.".yellow(),
        );

        error!(
            "🚫 {}",
            format!("rustfmt failed with exit code: {}", formatted_output.status).red()
        );
        std::process::exit(1);
    }

    let was_different = write_if_different(base_path, formatted_output.stdout, opts.check);
    *has_diffs |= was_different;

    // Calculate execution time
    let execution_time = start_time.elapsed();

    // Print success message with execution time
    if opts.check {
        info!(
            "✅ Checked {} (took {:?})",
            "tuple implementations".blue().green(),
            execution_time
        );
    } else if was_different {
        info!(
            "🔧 Generated {} (took {:?})",
            "tuple implementations".blue().green(),
            execution_time
        );
    } else {
        info!(
            "✅ No changes to {} (took {:?})",
            "tuple implementations".blue().green(),
            execution_time
        );
    }
}
