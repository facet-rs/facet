/// Expands and formats the sample code in `sample/` so we can
/// include it in the documentation.
pub fn cargo_expand_and_format() -> String {
    use std::io::Write;

    let workspace_dir = std::env::current_dir().unwrap();
    let sample_dir = workspace_dir
        .join("outside-workspace")
        .join("sample-for-expand");

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
        panic!(
            "cargo rustc expansion failed:\n--- stderr ---\n{}\n--- stdout ---\n{}",
            String::from_utf8_lossy(&cargo_expand_output.stderr).trim(),
            String::from_utf8_lossy(&cargo_expand_output.stdout).trim()
        );
    }

    // Prepare the code for rustfmt: prepend the necessary lines
    let expanded_code = String::from_utf8(cargo_expand_output.stdout)
        .expect("Failed to convert cargo expand output to UTF-8 string");

    // Replace any ::facet:: references with crate::
    let expanded_code = expanded_code.replace("::facet::", "crate::");
    let expanded_code = expanded_code.replace("use facet::", "use crate::");

    let expanded_code = expanded_code.replace(
        "::impls::_core::marker::PhantomData",
        "::core::marker::PhantomData",
    );

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
        panic!(
            "rustfmt failed:\n--- stderr ---\n{}\n--- stdout ---\n{}",
            String::from_utf8_lossy(&output.stderr).trim(),
            String::from_utf8_lossy(&output.stdout).trim()
        );
    }
    let _execution_time = start_time.elapsed();

    if !output.status.success() {
        panic!("Cargo expand command failed");
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
    let expanded_code = format!("{}\n#![allow(warnings)]\n{}", doc_comments, expanded_code);

    // Ensure a trailing newline for consistency
    if expanded_code.is_empty() {
        String::new()
    } else {
        format!("{}\n", expanded_code)
    }
}
