//! xtask for facet workspace

use std::{
    fs,
    path::PathBuf,
    process::{Command, Stdio},
    sync::mpsc,
    thread,
    time::Instant,
};

use facet::Facet;
use facet_args as args;
use facet_json::to_string;

/// xtask commands for the facet workspace.
#[derive(Facet, Debug)]
struct XtaskArgs {
    /// Command to run
    #[facet(args::subcommand)]
    command: XtaskCommand,
}

/// Available xtask commands.
#[derive(Facet, Debug)]
#[repr(u8)]
enum XtaskCommand {
    /// Generate all showcase markdown files for the website
    Showcases,

    /// Generate deterministic schema set for bloat/compile benches
    Schema,

    /// Generate schema, then build facet/serde variants (debug or release)
    SchemaBuild {
        /// Target triple to build for
        #[facet(default, args::named)]
        target: Option<String>,

        /// Build in release mode
        #[facet(args::named)]
        release: bool,

        /// Rust toolchain to use (e.g., nightly)
        #[facet(default, args::named)]
        toolchain: Option<String>,

        /// Timings format: html, json, or trace
        #[facet(default, args::named)]
        timings_format: Option<String>,

        /// Also generate JSON timings in addition to primary format
        #[facet(args::named)]
        also_json: bool,

        /// Include JSON serialization (now a no-op, kept for backwards compatibility)
        #[facet(args::named)]
        json: bool,
    },

    /// Measure compile times, binary size, LLVM lines, etc.
    Measure {
        /// Experiment name for the report
        #[facet(args::positional)]
        name: String,
    },

    /// Interactive TUI to explore metrics from reports/metrics.jsonl
    Metrics,

    /// Generate unified benchmark code
    GenBenchmarks,

    /// Generate TypeScript types for the frontend SPA from run_types.rs
    GenTypes,

    /// Run benchmarks, parse output, generate HTML report
    Bench(benchmark_defs::BenchReportArgs),
}

fn main() {
    let args: XtaskArgs = match args::from_std_args() {
        Ok(args) => args,
        Err(e) => {
            let is_help = e.is_help_request();
            eprintln!("{e}");
            // Exit with code 0 for help requests, 1 for actual errors
            std::process::exit(if is_help { 0 } else { 1 });
        }
    };

    match args.command {
        XtaskCommand::Showcases => generate_showcases(),
        XtaskCommand::Schema => generate_schema(),
        XtaskCommand::SchemaBuild {
            target,
            release,
            toolchain,
            timings_format,
            also_json,
            json,
        } => schema_build(target, release, toolchain, timings_format, also_json, json),
        XtaskCommand::Measure { name } => measure(&name),
        XtaskCommand::Metrics => metrics_tui(),
        XtaskCommand::GenBenchmarks => gen_benchmarks(),
        XtaskCommand::GenTypes => gen_types(),
        XtaskCommand::Bench(args) => bench_report(args),
    }
}

fn gen_benchmarks() {
    // Delegate to the benchmark-generator binary
    let status = Command::new("cargo")
        .args(["run", "-p", "benchmark-generator", "--release", "--"])
        .stdout(Stdio::inherit())
        .stderr(Stdio::inherit())
        .status()
        .expect("Failed to run benchmark-generator");

    if !status.success() {
        std::process::exit(status.code().unwrap_or(1));
    }
}

fn gen_types() {
    // Delegate to the gen-run-types binary
    let status = Command::new("cargo")
        .args([
            "run",
            "-p",
            "benchmark-analyzer",
            "--bin",
            "gen-run-types",
            "--release",
            "--",
        ])
        .stdout(Stdio::inherit())
        .stderr(Stdio::inherit())
        .status()
        .expect("Failed to run gen-run-types");

    if !status.success() {
        std::process::exit(status.code().unwrap_or(1));
    }
}

fn metrics_tui() {
    // Delegate to the metrics-tui binary
    let status = Command::new("cargo")
        .args(["run", "-p", "metrics-tui", "--release", "--"])
        .stdout(Stdio::inherit())
        .stderr(Stdio::inherit())
        .stdin(Stdio::inherit())
        .status()
        .expect("Failed to run metrics-tui");

    if !status.success() {
        std::process::exit(status.code().unwrap_or(1));
    }
}

fn bench_report(args: benchmark_defs::BenchReportArgs) {
    // Delegate to the benchmark-analyzer binary
    let mut cmd_args = vec![
        "run".to_string(),
        "-p".to_string(),
        "benchmark-analyzer".to_string(),
        "--bin".to_string(),
        "benchmark-analyzer".to_string(),
        "--release".to_string(),
        "--".to_string(),
    ];
    if let Some(filter) = &args.filter {
        cmd_args.push(filter.to_string());
    }
    if args.serve {
        cmd_args.push("--serve".to_string());
    }
    if args.no_run {
        cmd_args.push("--no-run".to_string());
    }
    if args.no_index {
        cmd_args.push("--no-index".to_string());
    }
    if args.push {
        cmd_args.push("--push".to_string());
    }

    let status = Command::new("cargo")
        .args(&cmd_args)
        .stdout(Stdio::inherit())
        .stderr(Stdio::inherit())
        .status()
        .expect("Failed to run benchmark-analyzer");

    if !status.success() {
        std::process::exit(status.code().unwrap_or(1));
    }
}

fn generate_showcases() {
    let workspace_root = workspace_root();
    let output_dir = workspace_root.join("docs/content/guide/showcases");

    fs::create_dir_all(&output_dir).expect("Failed to create output directory");

    // Gather provenance info once for all showcases
    let commit_full = get_git_output(&["rev-parse", "HEAD"]);
    let commit_short = get_git_output(&["rev-parse", "--short", "HEAD"]);
    let timestamp = get_iso_timestamp();
    let rustc_version = get_rustc_version();
    let github_repo = get_github_repo();

    // Find all *_showcase.rs examples
    let mut showcases = Vec::new();
    for entry in fs::read_dir(&workspace_root).expect("Failed to read workspace root") {
        let entry = entry.expect("Failed to read entry");
        let path = entry.path();

        if !path.is_dir() {
            continue;
        }

        let examples_dir = path.join("examples");
        if !examples_dir.exists() {
            continue;
        }

        let pkg_name = path.file_name().unwrap().to_str().unwrap().to_string();

        for example in fs::read_dir(&examples_dir).expect("Failed to read examples dir") {
            let example = example.expect("Failed to read example");
            let example_path = example.path();

            if let Some(name) = example_path.file_name().and_then(|n| n.to_str())
                && name.ends_with("_showcase.rs")
            {
                let example_name = name.trim_end_matches(".rs").to_string();
                let output_name = example_name.trim_end_matches("_showcase").to_string();
                // Compute relative path to source file
                let source_file = format!("{pkg_name}/examples/{name}");
                showcases.push((pkg_name.clone(), example_name, output_name, source_file));
            }
        }
    }

    showcases.sort();

    let total = showcases.len();
    println!("Generating {total} showcases in parallel...");

    // Channel to collect results
    let (tx, rx) = mpsc::channel();

    // Spawn threads for each showcase
    let handles: Vec<_> = showcases
        .into_iter()
        .map(|(pkg, example, output, source_file)| {
            let tx = tx.clone();
            let output_dir = output_dir.clone();
            let commit_full = commit_full.clone();
            let commit_short = commit_short.clone();
            let timestamp = timestamp.clone();
            let rustc_version = rustc_version.clone();
            let github_repo = github_repo.clone();

            thread::spawn(move || {
                let output_path = output_dir.join(format!("{output}.md"));

                let result = Command::new("cargo")
                    .args(["run", "-p", &pkg, "--example", &example, "--all-features"])
                    .env("FACET_SHOWCASE_OUTPUT", "markdown")
                    .env("FACET_SHOWCASE_COMMIT", &commit_full)
                    .env("FACET_SHOWCASE_COMMIT_SHORT", &commit_short)
                    .env("FACET_SHOWCASE_TIMESTAMP", &timestamp)
                    .env("FACET_SHOWCASE_RUSTC_VERSION", &rustc_version)
                    .env("FACET_SHOWCASE_GITHUB_REPO", &github_repo)
                    .env("FACET_SHOWCASE_SOURCE_FILE", &source_file)
                    .stdout(Stdio::piped())
                    .stderr(Stdio::piped())
                    .output();

                let status = match result {
                    Ok(output_result) if output_result.status.success() => {
                        fs::write(&output_path, &output_result.stdout)
                            .expect("Failed to write output file");
                        Ok(())
                    }
                    Ok(output_result) => {
                        let stderr = String::from_utf8_lossy(&output_result.stderr);
                        Err(stderr.lines().take(10).collect::<Vec<_>>().join("\n"))
                    }
                    Err(e) => Err(e.to_string()),
                };

                tx.send((pkg, example, output, status)).unwrap();
            })
        })
        .collect();

    // Drop the original sender so rx.iter() terminates
    drop(tx);

    // Collect and print results
    let mut successes = 0;
    let mut failures = Vec::new();

    for (pkg, example, output, status) in rx {
        match status {
            Ok(()) => {
                println!("  {pkg}::{example} -> {output}.md");
                successes += 1;
            }
            Err(e) => {
                failures.push(format!("{pkg}::{example}: {e}"));
            }
        }
    }

    // Wait for all threads to complete
    for handle in handles {
        handle.join().unwrap();
    }

    println!();
    println!("Generated {successes}/{total} showcases");

    if !failures.is_empty() {
        println!();
        println!("Failures:");
        for failure in failures {
            println!("  {failure}");
        }
    }
}

fn workspace_root() -> PathBuf {
    let output = Command::new("cargo")
        .args(["locate-project", "--workspace", "--message-format=plain"])
        .output()
        .expect("Failed to run cargo locate-project");

    let path = String::from_utf8(output.stdout).expect("Invalid UTF-8");
    PathBuf::from(path.trim())
        .parent()
        .expect("No parent directory")
        .to_path_buf()
}

// ============================================================================
// Compile-time measurement

#[derive(Debug, Default, Facet)]
struct Metrics {
    timestamp: String,
    commit: String,
    branch: String,
    experiment: String,
    compile_secs: f64,
    bin_unstripped: u64,
    bin_stripped: u64,
    llvm_lines: u64,
    llvm_copies: u64,
    type_sizes_total: u64,
    // Self-profile metrics (in milliseconds)
    selfprof: SelfProfileMetrics,
}

#[derive(Debug, Default, Facet)]
struct SelfProfileMetrics {
    llvm_module_optimize_ms: u64,
    llvm_module_codegen_ms: u64,
    llvm_lto_optimize_ms: u64,
    llvm_thin_lto_ms: u64,
    typeck_ms: u64,
    mir_borrowck_ms: u64,
    expand_proc_macro_ms: u64,
    eval_to_allocation_raw_ms: u64,
    codegen_module_ms: u64,
}

impl Metrics {
    fn to_jsonl(&self) -> String {
        to_string(self).expect("Failed to serialize Metrics")
    }
}

fn measure(experiment_name: &str) {
    // Sanitize experiment name for filename
    let experiment_name: String = experiment_name
        .chars()
        .map(|c| {
            if c.is_alphanumeric() || c == '-' || c == '_' {
                c
            } else {
                '-'
            }
        })
        .collect();

    let workspace = workspace_root();
    let reports_dir = workspace.join("reports");
    fs::create_dir_all(&reports_dir).expect("Failed to create reports directory");

    // Get git info
    let datetime = get_datetime();
    let timestamp = get_iso_timestamp();
    let commit_sha_short = get_git_output(&["rev-parse", "--short", "HEAD"]);
    let commit_sha_full = get_git_output(&["rev-parse", "HEAD"]);
    let branch = get_git_output(&["rev-parse", "--abbrev-ref", "HEAD"]);

    let report_filename = format!("{datetime}-{commit_sha_short}-{experiment_name}.txt");

    // Metrics we'll collect for JSONL
    let mut metrics = Metrics {
        timestamp: timestamp.clone(),
        commit: commit_sha_short.clone(),
        branch: branch.clone(),
        experiment: experiment_name.clone(),
        ..Default::default()
    };
    let report_path = reports_dir.join(&report_filename);

    println!("=== Facet Compile-Time Measurement ===");
    println!("Experiment: {experiment_name}");
    println!("Report: {}", report_path.display());
    println!();

    let mut report = String::new();
    report.push_str("=== Facet Compile-Time Measurement Report ===\n");
    report.push_str(&format!("Date: {datetime}\n"));
    report.push_str(&format!("Commit: {commit_sha_full}\n"));
    report.push_str(&format!("Branch: {branch}\n"));
    report.push_str(&format!("Experiment: {experiment_name}\n"));
    report.push('\n');
    report.push_str("=== Build Configuration ===\n");
    report.push_str("Package: facet-bloatbench\n");
    report.push_str("Features: facet\n");
    report.push_str("Profile: release\n");
    report.push_str("Toolchain: nightly\n");
    report.push('\n');

    let target_dir = workspace.join("target").join("measure");

    // Touch source file to ensure recompilation
    let generated_rs = workspace.join("facet-bloatbench/src/generated.rs");
    if let Ok(file) = fs::OpenOptions::new().write(true).open(&generated_rs) {
        let _ = file.set_modified(std::time::SystemTime::now());
    }

    // Step 1: Combined build with macro-stats, type-sizes, timings, and binary size
    println!("Step 1/3: Clean build with macro-stats + type-sizes + timings...");
    if target_dir.exists() {
        let _ = fs::remove_dir_all(&target_dir);
    }

    let start = Instant::now();
    let build_output = Command::new("cargo")
        .arg("+nightly")
        .args([
            "rustc",
            "-p",
            "facet-bloatbench",
            "--lib",
            "--features",
            "facet",
            "--release",
            "-Zunstable-options",
            "--timings=json",
        ])
        .arg("--target-dir")
        .arg(&target_dir)
        .args(["--", "-Zmacro-stats", "-Zprint-type-sizes"])
        .env("CARGO_INCREMENTAL", "0")
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .output()
        .expect("Failed to run cargo build");

    if !build_output.status.success() {
        let stderr = String::from_utf8_lossy(&build_output.stderr);
        eprintln!("Build failed:\n{stderr}");
        std::process::exit(1);
    }

    // Build binary (quick - deps already built)
    let _ = Command::new("cargo")
        .arg("+nightly")
        .args([
            "build",
            "-p",
            "facet-bloatbench",
            "--features",
            "facet",
            "--release",
        ])
        .arg("--target-dir")
        .arg(&target_dir)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .output();

    let compile_time = start.elapsed();

    // Parse outputs
    let stdout = String::from_utf8_lossy(&build_output.stdout);
    let stderr = String::from_utf8_lossy(&build_output.stderr);

    metrics.compile_secs = compile_time.as_secs_f64();
    report.push_str("=== Compile Time ===\n");
    report.push_str(&format!("Total: {:.2}s\n", metrics.compile_secs));
    report.push('\n');
    println!("  Compile time: {:.2}s", metrics.compile_secs);

    // Binary size
    let binary_path = target_dir.join("release").join("facet-bloatbench");
    let unstripped_size = fs::metadata(&binary_path).map(|m| m.len()).unwrap_or(0);

    let stripped_path = target_dir.join("release").join("facet-bloatbench.stripped");
    let stripped_size = if unstripped_size > 0 {
        fs::copy(&binary_path, &stripped_path).ok();
        Command::new("strip").arg(&stripped_path).status().ok();
        let size = fs::metadata(&stripped_path).map(|m| m.len()).unwrap_or(0);
        let _ = fs::remove_file(&stripped_path);
        size
    } else {
        0
    };

    metrics.bin_unstripped = unstripped_size;
    metrics.bin_stripped = stripped_size;
    report.push_str("=== Binary Size ===\n");
    report.push_str(&format!(
        "Unstripped: {} KB ({} bytes)\n",
        unstripped_size / 1024,
        unstripped_size
    ));
    report.push_str(&format!(
        "Stripped:   {} KB ({} bytes)\n",
        stripped_size / 1024,
        stripped_size
    ));
    report.push('\n');
    println!(
        "  Binary size: {} KB (stripped: {} KB)",
        unstripped_size / 1024,
        stripped_size / 1024
    );

    // Macro stats from stderr
    report.push_str("=== Macro Stats ===\n");
    let mut in_bloatbench_section = false;
    let mut macro_lines = Vec::new();
    for line in stderr.lines() {
        if line.contains("MACRO EXPANSION STATS: facet_bloatbench")
            || line.contains("MACRO EXPANSION STATS: facet-bloatbench")
        {
            in_bloatbench_section = true;
            macro_lines.push(line);
        } else if in_bloatbench_section {
            if line.starts_with("macro-stats ===") && !macro_lines.is_empty() {
                macro_lines.push(line);
                in_bloatbench_section = false;
            } else {
                macro_lines.push(line);
            }
        }
    }
    if macro_lines.is_empty() {
        report.push_str("(No macro stats found for facet-bloatbench)\n");
    } else {
        for line in &macro_lines {
            report.push_str(line);
            report.push('\n');
        }
    }
    println!("  Macro stats: {} lines", macro_lines.len());
    report.push('\n');

    // Type sizes from stdout
    report.push_str("=== Type Sizes ===\n");
    let print_type_lines = stdout
        .lines()
        .filter(|l| l.contains("print-type-size"))
        .count();
    let type_lines: Vec<&str> = stdout
        .lines()
        .filter(|l| {
            l.contains("print-type-size")
                && (l.contains("facet_bloatbench")
                    || l.contains("facet_core::")
                    || l.contains("facet_reflect::")
                    || l.contains("facet_json::")
                    || l.contains("facet_solver::"))
        })
        .collect();

    // Sum up type sizes - parse lines like "print-type-size type: `Foo`: 123 bytes, alignment: 8 bytes"
    let mut total_type_size: u64 = 0;
    for line in &type_lines {
        if line.contains(" type: ") {
            // Extract size from "... : NNN bytes, alignment..."
            if let Some(bytes_pos) = line.find(" bytes,") {
                // Find the number before " bytes,"
                let before_bytes = &line[..bytes_pos];
                if let Some(last_colon) = before_bytes.rfind(": ") {
                    let size_str = before_bytes[last_colon + 2..].trim();
                    if let Ok(size) = size_str.parse::<u64>() {
                        total_type_size += size;
                    }
                }
            }
        }
    }
    metrics.type_sizes_total = total_type_size;

    if type_lines.is_empty() {
        report.push_str(&format!(
            "(No facet-related type sizes found, {print_type_lines} total lines)\n",
        ));
    } else {
        report.push_str(&format!(
            "Total size of facet types: {total_type_size} bytes\n\n",
        ));
        for line in &type_lines {
            report.push_str(line);
            report.push('\n');
        }
    }
    println!(
        "  Type sizes: {} facet lines, {} total, {} bytes total",
        type_lines.len(),
        print_type_lines,
        total_type_size
    );
    report.push('\n');

    // Build timings from JSON file
    report.push_str("=== Build Timings ===\n");
    let timings_dir = target_dir.join("cargo-timings");
    if let Ok(entries) = fs::read_dir(&timings_dir) {
        for entry in entries.flatten() {
            let path = entry.path();
            if path.extension().map(|s| s == "json").unwrap_or(false) {
                if let Ok(contents) = fs::read_to_string(&path) {
                    report.push_str(&format!("Timings file: {}\n", path.display()));
                    report.push_str(&contents);
                    report.push('\n');
                }
                break;
            }
        }
    }
    println!("  Build timings collected");

    // Step 2: cargo llvm-lines (separate because it needs different invocation)
    println!("Step 2/3: Running cargo llvm-lines...");
    let llvm_lines_output = Command::new("cargo")
        .arg("+nightly")
        .args([
            "llvm-lines",
            "-p",
            "facet-bloatbench",
            "--lib",
            "--features",
            "facet",
            "--release",
        ])
        .arg("--target-dir")
        .arg(&target_dir)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .output();

    report.push_str("\n=== LLVM Lines (top 200) ===\n");
    match llvm_lines_output {
        Ok(output) if output.status.success() => {
            let llvm_stdout = String::from_utf8_lossy(&output.stdout);
            let lines: Vec<&str> = llvm_stdout.lines().take(200).collect();
            for line in &lines {
                report.push_str(line);
                report.push('\n');
            }
            // Parse TOTAL line: "  Lines         Copies       Function name"
            // First line after header is typically total: "  123456          789  (TOTAL)"
            if let Some(total_line) = llvm_stdout.lines().find(|l| l.contains("(TOTAL)")) {
                println!("  {total_line}");
                // Parse: "  123456          789  (TOTAL)"
                let parts: Vec<&str> = total_line.split_whitespace().collect();
                if parts.len() >= 2 {
                    if let Ok(lines_count) = parts[0].parse::<u64>() {
                        metrics.llvm_lines = lines_count;
                    }
                    if let Ok(copies_count) = parts[1].parse::<u64>() {
                        metrics.llvm_copies = copies_count;
                    }
                }
            }
        }
        Ok(output) => {
            let stderr = String::from_utf8_lossy(&output.stderr);
            report.push_str(&format!(
                "(cargo-llvm-lines failed: {})\n",
                stderr.lines().next().unwrap_or("unknown")
            ));
            println!("  (cargo-llvm-lines failed)");
        }
        Err(e) => {
            report.push_str(&format!("(cargo-llvm-lines not available: {e})\n"));
            println!("  (cargo-llvm-lines not installed)");
        }
    }

    // Step 3: Self-profile (separate because it needs -Zself-profile)
    println!("Step 3/3: Collecting rustc self-profile...");
    let selfprof_target_dir = workspace.join("target").join("measure-selfprof");
    let selfprof_output_dir = selfprof_target_dir.join("self-profile");
    if selfprof_target_dir.exists() {
        let _ = fs::remove_dir_all(&selfprof_target_dir);
    }
    fs::create_dir_all(&selfprof_output_dir).ok();

    // Touch to force recompile
    if let Ok(file) = fs::OpenOptions::new().write(true).open(&generated_rs) {
        let _ = file.set_modified(std::time::SystemTime::now());
    }

    let selfprof_output = Command::new("cargo")
        .arg("+nightly")
        .args([
            "rustc",
            "-p",
            "facet-bloatbench",
            "--lib",
            "--features",
            "facet",
            "--release",
        ])
        .arg("--target-dir")
        .arg(&selfprof_target_dir)
        .arg("--")
        .arg(format!("-Zself-profile={}", selfprof_output_dir.display()))
        .arg("-Zself-profile-events=default")
        .env("CARGO_INCREMENTAL", "0")
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .output();

    report.push_str("\n=== Rustc Self-Profile ===\n");
    match selfprof_output {
        Ok(output) => {
            if output.status.success() {
                let mut found_profile = false;
                if let Ok(entries) = fs::read_dir(&selfprof_output_dir) {
                    for entry in entries.flatten() {
                        let path = entry.path();
                        if path
                            .extension()
                            .map(|s| s == "mm_profdata")
                            .unwrap_or(false)
                        {
                            report.push_str(&format!("Self-profile data: {}\n", path.display()));
                            found_profile = true;

                            // Try summarize tool
                            let summarize = Command::new("summarize")
                                .arg("summarize")
                                .arg(&path)
                                .stdout(Stdio::piped())
                                .stderr(Stdio::piped())
                                .output();

                            match summarize {
                                Ok(sum_out) if sum_out.status.success() => {
                                    let stdout = String::from_utf8_lossy(&sum_out.stdout);
                                    report.push_str("--- summarize output ---\n");
                                    report.push_str(&stdout);
                                    report.push('\n');

                                    // Parse self-profile metrics from summarize output
                                    parse_selfprof_metrics(&stdout, &mut metrics.selfprof);
                                }
                                _ => {
                                    report.push_str("(summarize tool not available)\n");
                                }
                            }
                            break;
                        }
                    }
                }
                if !found_profile {
                    report.push_str("(No self-profile data found)\n");
                }
                println!("  Self-profile data collected");
            } else {
                let stderr = String::from_utf8_lossy(&output.stderr);
                report.push_str(&format!(
                    "(Self-profile failed: {})\n",
                    stderr.lines().next().unwrap_or("unknown")
                ));
                println!("  (Self-profile failed)");
            }
        }
        Err(e) => {
            report.push_str(&format!("(Failed to collect self-profile: {e})\n"));
            println!("  (Failed to collect self-profile)");
        }
    }

    // Write report
    fs::write(&report_path, &report).expect("Failed to write report");

    // Append to metrics.jsonl
    let metrics_path = reports_dir.join("metrics.jsonl");
    let mut jsonl_file = fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(&metrics_path)
        .expect("Failed to open metrics.jsonl");
    use std::io::Write;
    writeln!(jsonl_file, "{}", metrics.to_jsonl()).expect("Failed to write metrics");

    println!();
    println!("=== Report written to {} ===", report_path.display());
    println!("=== Metrics appended to {} ===", metrics_path.display());
    println!();
    println!("Summary:");
    println!("  Compile time: {:.2}s", metrics.compile_secs);
    println!(
        "  Binary size:  {} KB (stripped: {} KB)",
        metrics.bin_unstripped / 1024,
        metrics.bin_stripped / 1024
    );
    println!(
        "  LLVM lines:   {} ({} copies)",
        metrics.llvm_lines, metrics.llvm_copies
    );
    println!("  Type sizes:   {} bytes total", metrics.type_sizes_total);
}

fn get_datetime() -> String {
    let output = Command::new("date")
        .arg("+%Y-%m-%d-%H%M")
        .output()
        .expect("Failed to get date");
    String::from_utf8_lossy(&output.stdout).trim().to_string()
}

fn get_iso_timestamp() -> String {
    let output = Command::new("date")
        .arg("-Iseconds")
        .output()
        .expect("Failed to get date");
    String::from_utf8_lossy(&output.stdout).trim().to_string()
}

fn get_git_output(args: &[&str]) -> String {
    let output = Command::new("git")
        .args(args)
        .output()
        .expect("Failed to run git command");
    String::from_utf8_lossy(&output.stdout).trim().to_string()
}

fn get_rustc_version() -> String {
    let output = Command::new("rustc")
        .arg("--version")
        .output()
        .expect("Failed to get rustc version");
    String::from_utf8_lossy(&output.stdout).trim().to_string()
}

fn get_github_repo() -> String {
    // Try to extract repo from git remote URL
    let remote = get_git_output(&["remote", "get-url", "origin"]);
    // Handle both SSH and HTTPS URLs:
    // git@github.com:facet-rs/facet.git -> facet-rs/facet
    // https://github.com/facet-rs/facet.git -> facet-rs/facet
    if let Some(rest) = remote.strip_prefix("git@github.com:") {
        rest.trim_end_matches(".git").to_string()
    } else if let Some(rest) = remote.strip_prefix("https://github.com/") {
        rest.trim_end_matches(".git").to_string()
    } else {
        // Fallback to hardcoded value
        "facet-rs/facet".to_string()
    }
}

/// Parse self-profile summarize output to extract key metrics.
/// Lines look like:
/// | LLVM_module_optimize ..................................................  | 1.45s     | 12.030          | ...
fn parse_selfprof_metrics(output: &str, metrics: &mut SelfProfileMetrics) {
    for line in output.lines() {
        if !line.starts_with('|') {
            continue;
        }
        let parts: Vec<&str> = line.split('|').collect();
        if parts.len() < 3 {
            continue;
        }

        let item = parts[1].trim();
        let self_time = parts[2].trim();

        // Parse time like "1.45s" or "986.56ms" into milliseconds
        let ms = parse_time_to_ms(self_time);

        // Match against known items (they have dots padding them)
        if item.starts_with("LLVM_module_optimize ") {
            metrics.llvm_module_optimize_ms = ms;
        } else if item.starts_with("LLVM_module_codegen_emit_obj ") {
            metrics.llvm_module_codegen_ms = ms;
        } else if item.starts_with("LLVM_lto_optimize ") {
            metrics.llvm_lto_optimize_ms = ms;
        } else if item.starts_with("LLVM_thinlto ") {
            metrics.llvm_thin_lto_ms = ms;
        } else if item.starts_with("typeck ") {
            metrics.typeck_ms = ms;
        } else if item.starts_with("mir_borrowck ") {
            metrics.mir_borrowck_ms = ms;
        } else if item.starts_with("expand_proc_macro ") {
            metrics.expand_proc_macro_ms = ms;
        } else if item.starts_with("eval_to_allocation_raw ") {
            metrics.eval_to_allocation_raw_ms = ms;
        } else if item.starts_with("codegen_module ") {
            metrics.codegen_module_ms = ms;
        }
    }
}

/// Parse time string like "1.45s" or "986.56ms" into milliseconds
fn parse_time_to_ms(s: &str) -> u64 {
    let s = s.trim();
    if let Some(secs) = s.strip_suffix('s') {
        if let Some(ms_str) = secs.strip_suffix('m') {
            // It's actually "XXXms" - strip the 'm' we already took
            ms_str.parse::<f64>().map(|v| v as u64).unwrap_or(0)
        } else {
            // It's seconds
            secs.parse::<f64>()
                .map(|v| (v * 1000.0) as u64)
                .unwrap_or(0)
        }
    } else if let Some(ms_str) = s.strip_suffix("ms") {
        ms_str.parse::<f64>().map(|v| v as u64).unwrap_or(0)
    } else {
        0
    }
}

// ============================================================================
// Schema generator (bloat / compile-time benchmarks)

fn generate_schema() {
    let cfg = SchemaConfig::from_env();
    let mut generator = SchemaGenerator::new(cfg);

    let output = generator.render();

    let out_path = workspace_root()
        .join("facet-bloatbench")
        .join("src")
        .join("generated.rs");

    fs::create_dir_all(
        out_path
            .parent()
            .expect("generated.rs should have a parent directory"),
    )
    .expect("Failed to create output directory");

    fs::write(&out_path, output).expect("Failed to write generated schema");

    println!(
        "Wrote schema with {} structs and {} enums to {}",
        generator.structs.len(),
        generator.enums.len(),
        out_path.display()
    );
}

fn schema_build(
    target: Option<String>,
    release: bool,
    toolchain: Option<String>,
    timings_format: Option<String>,
    also_json: bool,
    include_json: bool,
) {
    let timings_format = timings_format.unwrap_or_else(|| "html".to_string());

    // If timings requested but no toolchain specified, default to nightly to avoid -Z errors on stable.
    let toolchain = toolchain.or_else(|| Some("nightly".to_string()));

    let schema_rustflags = std::env::var("FACET_SCHEMA_RUSTFLAGS").ok();

    let workspace = workspace_root();
    let base_target_dir = std::env::var("FACET_SCHEMA_TARGET_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|_| workspace.join("target").join("schema-build"));

    generate_schema();

    let build = |feature: &str, fmt: &str, clean: bool| {
        let mut cmd = Command::new("cargo");
        if let Some(tc) = &toolchain {
            cmd.arg(format!("+{tc}"));
        }
        cmd.arg("build")
            .arg("-p")
            .arg("facet-bloatbench")
            .arg("--no-default-features")
            .arg("--features");

        // facet feature now always includes facet-json, serde feature includes serde_json
        // The include_json flag is now a no-op but kept for backwards compatibility
        let _ = include_json;
        cmd.arg(feature);

        if release {
            cmd.arg("--release");
        }
        if let Some(t) = &target {
            cmd.arg("--target").arg(t);
        }

        // Separate incremental caches for facet vs serde to avoid cross-contamination
        cmd.env(
            "CARGO_TARGET_DIR",
            base_target_dir.join(feature).to_string_lossy().to_string(),
        );

        // Section timings are unstable; requires nightly.
        cmd.arg("-Z").arg("section-timings");
        match fmt {
            "html" => {
                cmd.arg("--timings");
            }
            other => {
                cmd.arg("-Z").arg("unstable-options");
                cmd.arg(format!("--timings={other}"));
            }
        }

        println!(
            "Building facet-bloatbench ({feature}, {fmt}){}{}",
            if release { " --release" } else { "" },
            target
                .as_ref()
                .map(|t| format!(" --target {t}"))
                .unwrap_or_default()
        );

        // Clean previous build directory to keep timings comparable
        let feature_target = base_target_dir.join(feature);
        if clean && feature_target.exists() {
            let _ = fs::remove_dir_all(&feature_target);
        }

        // Override RUSTFLAGS for the schema build without affecting xtask itself.
        if let Some(rf) = &schema_rustflags {
            cmd.env("RUSTFLAGS", rf);
        }

        let status = cmd.status().expect("failed to run cargo build");
        if !status.success() {
            std::process::exit(status.code().unwrap_or(1));
        }

        if fmt == "json" {
            let timings_dir = feature_target.join("cargo-timings");
            if let Ok(entries) = fs::read_dir(&timings_dir) {
                let mut newest = None;
                for e in entries.flatten() {
                    let path = e.path();
                    if path.extension().map(|s| s == "json").unwrap_or(false)
                        && let Ok(meta) = e.metadata()
                    {
                        let mtime = meta.modified().ok();
                        if newest
                            .as_ref()
                            .map(|(_, t)| mtime > Some(*t))
                            .unwrap_or(true)
                            && let Some(t) = mtime
                        {
                            newest = Some((path.clone(), t));
                        }
                    }
                }
                if let Some((p, _)) = newest {
                    println!("Latest JSON timings: {}", p.display());
                } else {
                    println!("No JSON timings found in {}", timings_dir.display());
                }
            } else {
                println!("No JSON timings dir found at {}", timings_dir.display());
            }
        }
    };

    build("facet", &timings_format, true);
    if also_json && timings_format != "json" {
        build("facet", "json", false);
    }

    build("serde", &timings_format, true);
    if also_json && timings_format != "json" {
        build("serde", "json", false);
    }
}

#[derive(Debug, Clone, Copy)]
struct SchemaConfig {
    seed: u64,
    structs: usize,
    enums: usize,
    max_fields: usize,
    max_variants: usize,
    max_depth: usize,
}

impl SchemaConfig {
    fn from_env() -> Self {
        let parse = |key: &str, default: u64| -> u64 {
            std::env::var(key)
                .ok()
                .and_then(|s| s.parse().ok())
                .unwrap_or(default)
        };

        SchemaConfig {
            seed: parse("FACET_SCHEMA_SEED", 42),
            structs: parse("FACET_SCHEMA_STRUCTS", 120) as usize,
            enums: parse("FACET_SCHEMA_ENUMS", 40) as usize,
            max_fields: parse("FACET_SCHEMA_MAX_FIELDS", 12) as usize,
            max_variants: parse("FACET_SCHEMA_MAX_VARIANTS", 8) as usize,
            max_depth: parse("FACET_SCHEMA_MAX_DEPTH", 3) as usize,
        }
    }
}

#[derive(Debug, Clone)]
struct StructSpec {
    name: String,
    fields: Vec<(String, TypeSpec)>,
}

#[derive(Debug, Clone)]
struct EnumSpec {
    name: String,
    variants: Vec<VariantSpec>,
}

#[derive(Debug, Clone)]
struct VariantSpec {
    name: String,
    kind: VariantKind,
}

#[derive(Debug, Clone)]
enum VariantKind {
    Unit,
    Tuple(Vec<TypeSpec>),
    Struct(Vec<(String, TypeSpec)>),
}

#[derive(Debug, Clone)]
enum TypeSpec {
    Primitive(&'static str),
    BorrowedStr,
    CowStr,
    Option(Box<TypeSpec>),
    Vec(Box<TypeSpec>),
    User(String),
}

impl TypeSpec {
    fn fmt(&self, mode: Mode) -> String {
        match (self, mode) {
            (TypeSpec::Primitive(p), _) => (*p).to_string(),
            (TypeSpec::BorrowedStr, Mode::Facet) => "String".to_string(),
            (TypeSpec::BorrowedStr, Mode::Serde) => "String".to_string(),
            (TypeSpec::CowStr, Mode::Facet) => "Cow<'static, str>".to_string(),
            (TypeSpec::CowStr, Mode::Serde) => "String".to_string(),
            (TypeSpec::Option(inner), m) => format!("Option<{}>", inner.fmt(m)),
            (TypeSpec::Vec(inner), m) => format!("Vec<{}>", inner.fmt(m)),
            (TypeSpec::User(name), _) => name.clone(),
        }
    }

    fn needs_cow(&self) -> bool {
        match self {
            TypeSpec::CowStr => true,
            TypeSpec::Option(inner) | TypeSpec::Vec(inner) => inner.needs_cow(),
            _ => false,
        }
    }
}

#[derive(Copy, Clone)]
enum Mode {
    Facet,
    Serde,
}

struct SchemaGenerator {
    cfg: SchemaConfig,
    rng: Lcg,
    structs: Vec<StructSpec>,
    enums: Vec<EnumSpec>,
    type_pool: Vec<String>,
}

impl SchemaGenerator {
    fn new(cfg: SchemaConfig) -> Self {
        let rng = Lcg::new(cfg.seed);
        let mut generator = SchemaGenerator {
            cfg,
            rng,
            structs: Vec::new(),
            enums: Vec::new(),
            type_pool: Vec::new(),
        };
        generator.build();
        generator
    }

    fn build(&mut self) {
        for idx in 0..self.cfg.structs {
            let name = format!("Struct{idx:03}");
            let field_count = self.rng.range(2, self.cfg.max_fields.max(2));
            let mut fields = Vec::new();
            for fidx in 0..field_count {
                let fname = format!("field_{fidx}");
                let ty = self.random_type(0);
                fields.push((fname, ty));
            }
            self.structs.push(StructSpec {
                name: name.clone(),
                fields,
            });
            // only expose completed types to avoid self-recursive shapes
            self.type_pool.push(name);
        }

        for idx in 0..self.cfg.enums {
            let name = format!("Enum{idx:03}");
            let variant_count = self.rng.range(2, self.cfg.max_variants.max(2));
            let mut variants = Vec::new();
            for vidx in 0..variant_count {
                let vname = format!("V{vidx}");
                let kind = match self.rng.next_u32() % 3 {
                    0 => VariantKind::Unit,
                    1 => {
                        let tuple_len = self.rng.range(1, 4);
                        let mut items = Vec::new();
                        for _ in 0..tuple_len {
                            items.push(self.random_type(0));
                        }
                        VariantKind::Tuple(items)
                    }
                    _ => {
                        let struct_len = self.rng.range(1, 4);
                        let mut items = Vec::new();
                        for fidx in 0..struct_len {
                            items.push((format!("f{fidx}"), self.random_type(0)));
                        }
                        VariantKind::Struct(items)
                    }
                };
                variants.push(VariantSpec { name: vname, kind });
            }
            self.enums.push(EnumSpec {
                name: name.clone(),
                variants,
            });
            self.type_pool.push(name);
        }
    }

    fn random_type(&mut self, depth: usize) -> TypeSpec {
        const PRIMS: &[&str] = &[
            "u8", "u16", "u32", "u64", "i32", "i64", "f32", "f64", "bool", "String",
        ];

        if depth >= self.cfg.max_depth {
            return if self.rng.next_u32().is_multiple_of(5) {
                self.user_type()
            } else {
                TypeSpec::Primitive(PRIMS[(self.rng.next_u32() as usize) % PRIMS.len()])
            };
        }

        match self.rng.next_u32() % 8 {
            0 => TypeSpec::BorrowedStr,
            1 => TypeSpec::CowStr,
            2 => TypeSpec::Option(Box::new(self.random_type(depth + 1))),
            3 => TypeSpec::Vec(Box::new(self.random_type(depth + 1))),
            4 => self.user_type(),
            _ => TypeSpec::Primitive(PRIMS[(self.rng.next_u32() as usize) % PRIMS.len()]),
        }
    }

    fn user_type(&mut self) -> TypeSpec {
        if self.type_pool.is_empty() {
            TypeSpec::Primitive("u8")
        } else {
            let idx = (self.rng.next_u32() as usize) % self.type_pool.len();
            TypeSpec::User(self.type_pool[idx].clone())
        }
    }

    fn render(&mut self) -> String {
        let mut out = String::new();
        out.push_str("// @generated by `cargo xtask schema`\n");
        out.push_str("// deterministic schema for compile-time/code-size benchmarking\n");
        out.push_str("#![allow(dead_code)]\n");
        out.push_str("#![allow(clippy::all)]\n\n");

        self.render_module(
            &mut out,
            Mode::Facet,
            "facet_types",
            "facet",
            "facet::Facet",
            "#[derive(Facet)]",
        );
        out.push('\n');
        self.render_module(
            &mut out,
            Mode::Serde,
            "serde_types",
            "serde",
            "serde::{Deserialize, Serialize}",
            "#[derive(Serialize, Deserialize)]",
        );

        out
    }

    fn render_module(
        &self,
        out: &mut String,
        mode: Mode,
        module: &str,
        cfg_feature: &str,
        uses: &str,
        derive: &str,
    ) {
        let uses_cow = matches!(mode, Mode::Facet)
            && (self
                .structs
                .iter()
                .any(|s| s.fields.iter().any(|(_, t)| t.needs_cow()))
                || self
                    .enums
                    .iter()
                    .any(|e| e.variants.iter().any(variant_needs_cow)));

        out.push_str(&format!(
            "#[cfg(feature = \"{cfg_feature}\")]\npub mod {module} {{\n",
        ));
        out.push_str(&format!("    use {uses};\n"));
        if uses_cow {
            out.push_str("    use std::borrow::Cow;\n");
        }
        out.push('\n');

        for s in &self.structs {
            out.push_str(&format!("    {derive}\n"));
            out.push_str("    #[derive(Default)]\n");
            out.push_str(&format!("    pub struct {} {{\n", s.name));
            for (fname, ty) in &s.fields {
                out.push_str(&format!("        pub {}: {},\n", fname, ty.fmt(mode)));
            }
            out.push_str("    }\n\n");
        }

        for e in &self.enums {
            out.push_str(&format!("    {derive}\n"));
            out.push_str("    #[repr(u16)]\n");
            out.push_str(&format!("    pub enum {} {{\n", e.name));
            for v in &e.variants {
                match &v.kind {
                    VariantKind::Unit => {
                        out.push_str(&format!("        {},\n", v.name));
                    }
                    VariantKind::Tuple(items) => {
                        let items_str = items
                            .iter()
                            .map(|t| t.fmt(mode))
                            .collect::<Vec<_>>()
                            .join(", ");
                        out.push_str(&format!("        {}({}),\n", v.name, items_str));
                    }
                    VariantKind::Struct(fields) => {
                        out.push_str(&format!("        {} {{\n", v.name));
                        for (fname, ty) in fields {
                            out.push_str(&format!("            {}: {},\n", fname, ty.fmt(mode)));
                        }
                        out.push_str("        },\n");
                    }
                }
            }
            out.push_str("    }\n\n");
        }

        out.push_str("}\n");

        fn variant_needs_cow(v: &VariantSpec) -> bool {
            match &v.kind {
                VariantKind::Unit => false,
                VariantKind::Tuple(items) => items.iter().any(TypeSpec::needs_cow),
                VariantKind::Struct(fields) => fields.iter().any(|(_, t)| t.needs_cow()),
            }
        }
    }
}

// Simple LCG to avoid adding RNG dependencies
struct Lcg(u64);

impl Lcg {
    const fn new(seed: u64) -> Self {
        Lcg(seed | 1) // avoid zero cycles
    }

    const fn next_u32(&mut self) -> u32 {
        self.0 = self.0.wrapping_mul(6364136223846793005).wrapping_add(1);
        (self.0 >> 32) as u32
    }

    const fn range(&mut self, min: usize, max: usize) -> usize {
        if max <= min {
            return min;
        }
        min + (self.next_u32() as usize % (max - min + 1))
    }
}
