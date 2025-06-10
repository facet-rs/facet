// measure-bloat/src/build.rs

use anyhow::{Context, Result};
use owo_colors::OwoColorize;
use std::collections::HashMap;
use std::path::Path;
use std::process::Command;
use std::time::Instant;

use crate::types::{
    BuildTimingSummary, BuildWithLllvmIrOpts, CrateLlvmLines, CrateTiming, LlvmBuildOutput,
    LlvmFunction, LlvmLinesSummary,
};
use indicatif::{ProgressBar, ProgressStyle}; // For progress indication

/// Builds the project for a given binary/example, enabling LLVM IR emission and collecting timings.
///
/// # Arguments
/// * `binary_to_build`: The name of the binary or example to build (e.g., "my_app", "example_benchmark").
/// * `active_workspace_path`: The root path of the workspace where `cargo build` will be executed.
/// * `build_artifacts_target_dir`: The isolated directory where `cargo` will place build artifacts (`target/`).
/// * `opts`: Additional build options like manifest path and environment variables.
///
/// # Returns
/// A `Result` containing `LlvmBuildOutput`, which includes the path to the
/// `build_artifacts_target_dir` and the `BuildTimingSummary`.
pub(crate) fn build_project_for_analysis(
    binary_to_build: &str,
    active_workspace_path: &Path,
    build_artifacts_target_dir: &Path,
    opts: &BuildWithLllvmIrOpts,
) -> Result<LlvmBuildOutput> {
    let start_time = Instant::now();
    let pb = ProgressBar::new_spinner();
    pb.set_style(
        ProgressStyle::default_spinner()
            .tick_chars("⠁⠂⠄⡀⢀⠠⠐⠈ ")
            .template(&format!(
                "{{spinner:.green}} [build] Building '{}' for analysis in {:?}...",
                binary_to_build, active_workspace_path
            ))
            .expect("BUG: Invalid indicatif template for build progress"),
    );
    pb.enable_steady_tick(std::time::Duration::from_millis(100));

    // Ensure the isolated build target directory exists
    std::fs::create_dir_all(build_artifacts_target_dir).with_context(|| {
        format!(
            "Failed to create isolated build target directory: {:?}",
            build_artifacts_target_dir
        )
    })?;

    let mut rustflags = opts.env_vars.get("RUSTFLAGS").cloned().unwrap_or_default();
    if !rustflags.contains("--emit=llvm-ir") {
        rustflags.push_str(" --emit=llvm-ir");
    }

    let mut command = Command::new("cargo");
    command.current_dir(active_workspace_path); // Run cargo from the active workspace
    command.arg("build");
    command.arg("--release");
    command.arg("--message-format=json-diagnostic-rendered-ansi"); // For better error parsing
    command.arg("--timings=json"); // Enable build timings, output to <target_dir>/cargo-timings.json
    command.arg("-Z").arg("unstable-options"); // Required on stable for timings=json

    // Determine if it's a binary or an example.
    // This heuristic checks for an `examples/<name>.rs` file within the active workspace.
    let example_path = active_workspace_path
        .join("examples")
        .join(format!("{}.rs", binary_to_build));
    let is_example = example_path.exists() && example_path.is_file();

    if is_example {
        command.arg("--example").arg(binary_to_build);
    } else {
        command.arg("--bin").arg(binary_to_build);
    }

    command.env("RUSTFLAGS", rustflags.trim());
    // Unstable timings flag requires nightly or explicit RUSTC_BOOTSTRAP=1
    command.env("RUSTC_BOOTSTRAP", "1");
    command.arg("--manifest-path").arg(&opts.manifest_path); // Manifest path from BuildOpts
    command.arg("--target-dir").arg(build_artifacts_target_dir);

    for (key, value) in &opts.env_vars {
        if key != "RUSTFLAGS" {
            // RUSTFLAGS is already handled
            command.env(key, value);
        }
    }
    log::debug!(
        "{} {} Running cargo command: {:?}",
        "🚀".bright_blue(),
        "[build]".bright_black(),
        command
    );

    let output = command.output().with_context(|| {
        format!(
            "Failed to execute cargo build for LLVM IR for '{}' in workspace {:?}",
            binary_to_build, active_workspace_path
        )
    })?;

    pb.disable_steady_tick();
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        let stdout_for_error = String::from_utf8_lossy(&output.stdout);
        pb.finish_with_message(format!(
            "{} [build] Build FAILED for {}",
            "❌".red(),
            binary_to_build.bright_red()
        ));
        anyhow::bail!(
            "cargo build for LLVM IR failed for '{}':\nStatus: {}\nCWD: {:?}\nManifest: {}\nTarget Dir: {:?}\nStderr:\n{}\nStdout:\n{}",
            binary_to_build,
            output.status,
            active_workspace_path,
            opts.manifest_path,
            build_artifacts_target_dir,
            stderr,
            stdout_for_error
        );
    }
    pb.finish_with_message(format!(
        "{} [build] Build SUCCEEDED for {}",
        "✅".green(),
        binary_to_build.bright_green()
    ));

    // Capture stdout for substance analysis. This must be valid UTF-8.
    let cargo_stdout_json_lines = String::from_utf8(output.stdout)
        .context("Failed to convert cargo build stdout to UTF-8 string")?;

    let timing_summary = parse_cargo_timings(build_artifacts_target_dir).with_context(|| {
        format!(
            "Failed to parse build timings for '{}' from artifacts in {:?}",
            binary_to_build, build_artifacts_target_dir
        )
    })?;

    log::info!(
        "[build] LLVM IR build for {} completed in {}s. Artifacts in: {}",
        binary_to_build.bright_green(),
        format!("{:.2}", start_time.elapsed().as_secs_f64()).bright_yellow(),
        build_artifacts_target_dir.to_string_lossy().bright_cyan()
    );

    Ok(LlvmBuildOutput {
        target_dir: build_artifacts_target_dir.to_path_buf(),
        timing_summary,
        cargo_stdout_json_lines,
    })
}

/// Parses the `cargo-timings.json` file produced by a cargo build.
fn parse_cargo_timings(build_artifacts_target_dir: &Path) -> Result<BuildTimingSummary> {
    // Cargo places cargo-timings.json at the root of the target-dir
    let timings_file_path = build_artifacts_target_dir.join("cargo-timings.json");

    if !timings_file_path.exists() {
        log::warn!(
            "{} {} cargo-timings.json not found at {}. Build time breakdown will be incomplete.",
            "⚠️".yellow(),
            "[build]".bright_black(),
            timings_file_path.to_string_lossy().bright_red()
        );
        return Ok(BuildTimingSummary {
            total_duration: std::time::Duration::from_secs(0), // Signifies missing detailed data
            crate_timings: Vec::new(),
        });
    }
    log::debug!(
        "{} {} Parsing cargo timings from: {}",
        "📊".bright_blue(),
        "[build]".bright_black(),
        timings_file_path.to_string_lossy().bright_cyan()
    );
    parse_cargo_timings_from_file(&timings_file_path)
}

/// Helper to parse a specific cargo timings JSON file.
fn parse_cargo_timings_from_file(file_path: &Path) -> Result<BuildTimingSummary> {
    let file_content = std::fs::read_to_string(file_path)
        .with_context(|| format!("[build] Failed to read timings file: {:?}", file_path))?;

    let mut crate_timings_map: HashMap<String, f64> = HashMap::new();
    let mut total_duration_secs_from_finished_event = 0.0;

    for line in file_content.lines() {
        if let Ok(json_value) = serde_json::from_str::<serde_json::Value>(line.trim()) {
            if let Some(reason) = json_value.get("reason").and_then(|v| v.as_str()) {
                if reason == "compiler-artifact" {
                    if let (Some(target_val), Some(duration_val)) =
                        (json_value.get("target"), json_value.get("duration"))
                    {
                        if let (Some(target_obj), Some(duration)) =
                            (target_val.as_object(), duration_val.as_f64())
                        {
                            if let Some(crate_name_val) = target_obj.get("name") {
                                if let Some(crate_name) = crate_name_val.as_str() {
                                    *crate_timings_map
                                        .entry(crate_name.to_string())
                                        .or_insert(0.0) += duration;
                                }
                            }
                        }
                    }
                } else if reason == "build-finished" {
                    if let Some(total_field) = json_value
                        .get("total_time_in_seconds")
                        .and_then(|v| v.as_f64())
                    {
                        total_duration_secs_from_finished_event = total_field;
                    }
                }
                // Add parsing for "build-script-executed" if needed
            }
        }
    }

    let final_total_duration_secs = if total_duration_secs_from_finished_event > 0.0 {
        total_duration_secs_from_finished_event
    } else {
        // Fallback: sum of compiler-artifact durations if build-finished event is missing or malformed.
        // This is less accurate as it might miss some parts of the build or double count.
        log::warn!(
            "{} {} 'build-finished' event with 'total_time_in_seconds' not found or zero in timings file. Summing artifact durations as a fallback.",
            "⚠️".yellow(),
            "[build]".bright_black()
        );
        crate_timings_map.values().sum()
    };

    let mut crate_timings_vec: Vec<CrateTiming> = crate_timings_map
        .into_iter()
        .map(|(name, duration)| CrateTiming { name, duration })
        .collect();

    crate_timings_vec.sort_by(|a, b| {
        b.duration
            .partial_cmp(&a.duration)
            .unwrap_or(std::cmp::Ordering::Equal)
    });

    Ok(BuildTimingSummary {
        total_duration: std::time::Duration::from_secs_f64(final_total_duration_secs),
        crate_timings: crate_timings_vec,
    })
}

/// Fetches LLVM lines data using `cargo llvm-lines`.
///
/// # Arguments
/// * `build_artifacts_target_dir`: Path to the target directory from `LlvmBuildOutput`.
/// * `binary_to_analyze`: The name of the binary or example that was built.
/// * `_crates_to_analyze`: (Currently unused) List of crate names to focus analysis on.
///   `cargo llvm-lines` output typically includes all linked crates.
/// * `active_workspace_manifest_path`: Path to `Cargo.toml` of the workspace for `cargo llvm-lines`.
pub(crate) fn fetch_llvm_lines_data(
    build_artifacts_target_dir: &Path,
    binary_to_analyze: &str,
    _crates_to_analyze: &[String], // Placeholder, llvm-lines output is parsed for all crates
    active_workspace_manifest_path: &Path,
) -> Result<LlvmLinesSummary> {
    let pb = ProgressBar::new_spinner();
    pb.set_style(
        ProgressStyle::default_spinner()
            .tick_chars("⠁⠂⠄⡀⢀⠠⠐⠈ ")
            .template(&format!(
                "{{spinner:.green}} [build] Fetching LLVM lines for '{}'...",
                binary_to_analyze
            ))
            .expect("BUG: Invalid indicatif template for llvm-lines progress"),
    );
    pb.enable_steady_tick(std::time::Duration::from_millis(100));

    let example_path = active_workspace_manifest_path
        .parent()
        .unwrap_or_else(|| Path::new(".")) // Should be active_workspace_path
        .join("examples")
        .join(format!("{}.rs", binary_to_analyze));
    let is_example = example_path.exists() && example_path.is_file();

    let mut command = Command::new("cargo");
    command.arg("llvm-lines");
    command
        .arg("--manifest-path")
        .arg(active_workspace_manifest_path);
    command.arg("--target-dir").arg(build_artifacts_target_dir); // Crucial: use artifacts from our specific build

    if is_example {
        command.arg("--example").arg(binary_to_analyze);
    } else {
        command.arg("--bin").arg(binary_to_analyze);
    }
    log::debug!(
        "{} {} Running cargo llvm-lines command: {:?}",
        "🚀".bright_blue(),
        "[build]".bright_black(),
        command
    );

    let output = command.output().with_context(|| {
        format!(
            "Failed to execute cargo llvm-lines for '{}'",
            binary_to_analyze
        )
    })?;

    pb.disable_steady_tick();
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        pb.finish_with_message(format!(
            "{} [build] cargo llvm-lines FAILED for {}",
            "❌".red(),
            binary_to_analyze.bright_red()
        ));
        anyhow::bail!(
            "cargo llvm-lines failed for binary/example '{}':\nManifest: {:?}\nTarget Dir: {:?}\nStderr: {}",
            binary_to_analyze,
            active_workspace_manifest_path,
            build_artifacts_target_dir,
            stderr
        );
    }
    pb.finish_with_message(format!(
        "{} [build] cargo llvm-lines SUCCEEDED for {}",
        "✅".green(),
        binary_to_analyze.bright_green()
    ));

    let stdout = String::from_utf8_lossy(&output.stdout);
    parse_llvm_lines_output(&stdout)
}

/// Parses the text output of `cargo llvm-lines`.
fn parse_llvm_lines_output(output: &str) -> Result<LlvmLinesSummary> {
    let mut crate_results_map: HashMap<String, (u64, u64)> = HashMap::new(); // (lines, copies)
    let mut top_functions: Vec<LlvmFunction> = Vec::new();

    let mut lines_iter = output.lines();
    // Skip header lines (usually 2: "Lines Copies Crate Function" and "---- ---- ---- ----")
    lines_iter.next(); // Header
    lines_iter.next(); // Separator

    for line_str in lines_iter {
        let parts: Vec<&str> = line_str.split_whitespace().collect();

        if parts.contains(&"(TOTAL)") {
            continue; // We sum totals from individual entries
        }

        // Expected format: <lines> (<p1>%) <copies> (<p2>%) <crate> <function ...>
        // Or without percentages: <lines> <copies> <crate> <function ...>
        if parts.len() < 4 {
            log::trace!(
                "[build] Skipping short/malformed llvm-lines line: '{}'",
                line_str
            );
            continue;
        }

        let Ok(line_count) = parts[0].parse::<u64>() else {
            log::trace!(
                "[build] Failed to parse line count from '{}' in line: '{}'",
                parts[0],
                line_str
            );
            continue;
        };

        let mut current_idx = 1;
        if parts
            .get(current_idx)
            .is_some_and(|s| s.starts_with('(') && s.ends_with('%'))
        {
            current_idx += 1; // Skip percentage for lines
        }

        let Some(copies_str) = parts.get(current_idx) else {
            log::trace!(
                "[build] Not enough parts for copies in line: '{}'",
                line_str
            );
            continue;
        };
        let Ok(copy_count) = copies_str.parse::<u64>() else {
            log::trace!(
                "[build] Failed to parse copy count from '{}' in line: '{}'",
                copies_str,
                line_str
            );
            continue;
        };
        current_idx += 1;

        if parts
            .get(current_idx)
            .is_some_and(|s| s.starts_with('(') && s.ends_with('%'))
        {
            current_idx += 1; // Skip percentage for copies
        }

        let Some(crate_name) = parts.get(current_idx) else {
            log::trace!(
                "[build] Not enough parts for crate name in line: '{}'",
                line_str
            );
            continue;
        };
        current_idx += 1;

        if parts.len() <= current_idx {
            log::trace!(
                "[build] Not enough parts for function name in line: '{}'",
                line_str
            );
            continue;
        }
        let function_name = parts[current_idx..].join(" ");

        // Aggregate for crate results
        let entry = crate_results_map
            .entry(crate_name.to_string())
            .or_insert((0, 0));
        entry.0 += line_count;
        entry.1 += copy_count;

        top_functions.push(LlvmFunction {
            name: function_name,
            lines: line_count,
            copies: copy_count,
        });
    }

    let mut crate_results: Vec<CrateLlvmLines> = crate_results_map
        .into_iter()
        .map(|(name, (lines, copies))| CrateLlvmLines {
            name,
            lines,
            copies,
        })
        .collect();

    crate_results.sort_by(|a, b| b.lines.cmp(&a.lines).then_with(|| a.name.cmp(&b.name)));
    top_functions.sort_by(|a, b| b.lines.cmp(&a.lines).then_with(|| a.name.cmp(&b.name)));

    Ok(LlvmLinesSummary {
        crate_results,
        top_functions,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs::File;
    use std::io::Write;
    use tempfile::tempdir;

    #[test]
    fn test_parse_llvm_lines_simple_output() {
        let output = r#"
Lines          Copies          Crate           Function
-------------  --------------  --------------  -------------
100 (50.0%)    1 (33.3%)       my_crate        function_a
50 (25.0%)     1 (33.3%)       my_crate        function_b
50 (25.0%)     1 (33.3%)       another_crate   function_c
200            3               (TOTAL)
"#;
        let summary = parse_llvm_lines_output(output).unwrap();

        assert_eq!(summary.crate_results.len(), 2);
        let my_crate_res = summary
            .crate_results
            .iter()
            .find(|r| r.name == "my_crate")
            .unwrap();
        assert_eq!(my_crate_res.lines, 150);
        assert_eq!(my_crate_res.copies, 2);

        let another_crate_res = summary
            .crate_results
            .iter()
            .find(|r| r.name == "another_crate")
            .unwrap();
        assert_eq!(another_crate_res.lines, 50);
        assert_eq!(another_crate_res.copies, 1);

        assert_eq!(summary.top_functions.len(), 3);
        assert_eq!(summary.top_functions[0].name, "function_a");
        assert_eq!(summary.top_functions[0].lines, 100);
    }

    #[test]
    fn test_parse_llvm_lines_no_percentage_on_copies() {
        let output = r#"
Lines          Copies          Crate           Function
-------------  --------------  --------------  -------------
120 (60.0%)    2               my_crate        func_x[0]
80 (40.0%)     1               my_crate        func_y
200            3               (TOTAL)
"#;
        let summary = parse_llvm_lines_output(output).unwrap();
        assert_eq!(summary.crate_results.len(), 1);
        let my_crate_res = summary.crate_results.first().unwrap();
        assert_eq!(my_crate_res.lines, 200);
        assert_eq!(my_crate_res.copies, 3);

        assert_eq!(summary.top_functions.len(), 2);
        assert_eq!(summary.top_functions[0].name, "func_x[0]");
        assert_eq!(summary.top_functions[0].lines, 120);
        assert_eq!(summary.top_functions[0].copies, 2);
    }

    #[test]
    fn test_parse_llvm_lines_no_percentages_at_all() {
        let output = r#"
Lines          Copies          Crate           Function
-------------  --------------  --------------  -------------
120            2               my_crate        func_x[0]
80             1               another_crate   func_y some_generic<T>
200            3               (TOTAL)
"#;
        let summary = parse_llvm_lines_output(output).unwrap();
        assert_eq!(summary.crate_results.len(), 2);
        let my_crate_res = summary
            .crate_results
            .iter()
            .find(|r| r.name == "my_crate")
            .unwrap();
        assert_eq!(my_crate_res.lines, 120);

        let another_crate_res = summary
            .crate_results
            .iter()
            .find(|r| r.name == "another_crate")
            .unwrap();
        assert_eq!(another_crate_res.lines, 80);

        assert_eq!(summary.top_functions[0].name, "func_x[0]");
        assert_eq!(summary.top_functions[1].name, "func_y some_generic<T>");
    }

    #[test]
    fn test_parse_real_cargo_timing_output_with_build_finished() {
        let dir = tempdir().unwrap();
        let file_path = dir.path().join("cargo-timings.json");
        let mut file = File::create(&file_path).unwrap();
        // Line resembling a compiler artifact
        writeln!(file, r#"{{"reason":"compiler-artifact","package_id":"some_crate 0.1.0 (path+file:///path/to/some_crate)","target":{{"name":"some_crate"}},"duration":5.123}}"#).unwrap();
        // Line resembling another compiler artifact
        writeln!(file, r#"{{"reason":"compiler-artifact","package_id":"another_crate 0.1.0 (path+file:///path/to/another_crate)","target":{{"name":"another_crate"}},"duration":2.050}}"#).unwrap();
        // Line resembling build-finished event
        writeln!(
            file,
            r#"{{"reason":"build-finished","success":true,"total_time_in_seconds":8.500}}"#
        )
        .unwrap();
        drop(file);

        let summary = parse_cargo_timings_from_file(&file_path).unwrap();

        assert_eq!(summary.total_duration.as_secs_f64(), 8.500);
        assert_eq!(summary.crate_timings.len(), 2);

        let some_crate_timing = summary
            .crate_timings
            .iter()
            .find(|ct| ct.name == "some_crate")
            .expect("some_crate timing not found");
        assert_eq!(some_crate_timing.duration, 5.123);

        let another_crate_timing = summary
            .crate_timings
            .iter()
            .find(|ct| ct.name == "another_crate")
            .expect("another_crate timing not found");
        assert_eq!(another_crate_timing.duration, 2.050);
    }

    #[test]
    fn test_parse_cargo_timing_output_no_build_finished_total() {
        let dir = tempdir().unwrap();
        let file_path = dir.path().join("cargo-timings.json");
        let mut file = File::create(&file_path).unwrap();
        writeln!(file, r#"{{"reason":"compiler-artifact","package_id":"crate_a 0.1.0","target":{{"name":"crate_a"}},"duration":3.0}}"#).unwrap();
        writeln!(file, r#"{{"reason":"compiler-artifact","package_id":"crate_b 0.1.0","target":{{"name":"crate_b"}},"duration":4.0}}"#).unwrap();
        // No build-finished event with total_time_in_seconds
        writeln!(file, r#"{{"reason":"build-finished","success":true}}"#).unwrap();

        drop(file);

        let summary = parse_cargo_timings_from_file(&file_path).unwrap();

        // Should fallback to summing durations
        assert!((summary.total_duration.as_secs_f64() - 7.0).abs() < f64::EPSILON);
        assert_eq!(summary.crate_timings.len(), 2);
    }
}
