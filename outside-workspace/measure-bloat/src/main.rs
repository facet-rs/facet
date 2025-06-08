use anyhow::{Context, Result};
use clap::{Parser, Subcommand};
use owo_colors::OwoColorize;
use regex::Regex;
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::PathBuf;
use std::process::{self, Command};
use std::time::Instant;
use toml_edit::{DocumentMut, Item, Value};

/// Command-line interface for the measure-bloat utility
/// Used for: Parsing command line arguments and routing to appropriate functionality
#[derive(Parser)]
#[command(name = "measure-bloat")]
#[command(about = "A utility to measure and compare binary sizes and build times")]
struct Cli {
    /// The subcommand to execute
    /// Used for: Determining whether to run full comparison or test individual components
    #[command(subcommand)]
    command: Commands,
}

/// Available commands for the measure-bloat utility
#[derive(Subcommand)]
enum Commands {
    /// Run the full comparison between serde, facet-pr, and facet-main
    /// Used for: Complete performance analysis across all variants
    Compare,
}

/// Configuration for measuring a specific comparison target
/// Used to define what crates to include when comparing different serialization implementations
#[derive(Debug, Clone)]
struct MeasurementTarget {
    /// Display name for this measurement target
    /// Example: "ks-facet", "json-benchmark"
    /// Used for: Report generation and logging
    name: String,

    /// List of crates to include when measuring with facet variants
    /// Example: ["ks-facet", "ks-mock", "ks-types", "ks-facet-json-read"]
    /// Obtained from: Manual configuration based on project structure
    /// Used for: LLVM lines analysis and determining what to measure
    facet_crates: Vec<String>,

    /// List of crates to include when measuring with serde variant
    /// Example: ["ks-serde", "ks-mock", "ks-types", "ks-serde-json-read"]
    /// Obtained from: Manual configuration based on project structure
    /// Used for: LLVM lines analysis and determining what to measure
    serde_crates: Vec<String>,

    /// The main binary crate to compile and measure
    /// Example: "ks-facet", "ks-serde"
    /// Used for: cargo bloat and build time measurements
    binary_crate: String,
}

#[derive(Debug, Serialize, Deserialize)]
struct BloatFunction {
    #[serde(default, rename = "crate")]
    crate_name: String,
    name: String,
    size: u64,
}

/// Crate-level size information from cargo-bloat
/// Represents one crate's total contribution to binary size
/// Crate-level size information from cargo-bloat analysis
#[derive(Debug, Serialize, Deserialize)]
struct BloatCrate {
    /// Name of the crate
    /// Example: "ks_facet", "serde", "tokio"
    /// Obtained from: cargo-bloat --crates JSON output
    /// Used for: Understanding which crates contribute most to binary size
    name: String,

    /// Total size contributed by this crate in bytes
    /// Example: 50000, 1024000
    /// Obtained from: cargo-bloat --crates JSON output
    /// Used for: High-level crate size comparison
    size: u64,
}

/// Complete output from cargo-bloat command
/// Contains both high-level metrics and detailed breakdowns
/// Complete output from cargo-bloat analysis containing size information
#[derive(Debug, Serialize, Deserialize)]
struct BloatOutput {
    /// Total binary file size in bytes
    /// Example: 2097152, 1548576
    /// Obtained from: cargo-bloat JSON output
    /// Used for: Overall binary size comparison between variants
    #[serde(rename = "file-size")]
    file_size: u64,

    /// Size of the text (code) section in bytes
    /// Example: 1500000, 3500000
    /// Obtained from: cargo-bloat JSON output
    /// Used for: Measuring actual executable code size
    #[serde(rename = "text-section-size")]
    text_section_size: u64,

    /// List of functions and their sizes (only present in function mode)
    /// Obtained from: cargo-bloat function analysis
    /// Used for: Detailed function-level size breakdown
    #[serde(default)]
    functions: Vec<BloatFunction>,

    /// List of crates and their sizes (only present in crates mode)
    /// Obtained from: cargo-bloat --crates analysis
    /// Used for: High-level crate size breakdown
    #[serde(default)]
    crates: Vec<BloatCrate>,
}

/// Target information from cargo timing output
#[derive(Debug, Serialize, Deserialize)]
struct CargoTimingTarget {
    /// Name of the compilation target (usually the crate name)
    /// Example: "ks-facet", "serde"
    /// Obtained from: cargo build --timings JSON output
    /// Used for: Identifying which crate the timing data belongs to
    name: String,
}

/// Individual timing entry from cargo build timing output
#[derive(Debug, Serialize, Deserialize)]
struct CargoTimingEntry {
    /// Type of timing event
    /// Example: "timing-info", "build-start"
    /// Obtained from: cargo build --timings JSON output
    /// Used for: Filtering relevant timing events
    reason: String,

    /// Full package identifier with version
    /// Example: "ks-facet 0.1.0", "serde 1.0.152"
    /// Obtained from: cargo build --timings JSON output
    /// Used for: Package identification and version tracking
    package_id: String,

    /// Target information for this timing entry
    /// Obtained from: cargo build --timings JSON output
    /// Used for: Associating timing with specific build targets
    target: CargoTimingTarget,

    /// Compilation duration in seconds
    /// Example: 2.5, 0.8, 12.3
    /// Obtained from: cargo build timing measurements
    /// Used for: Measuring and comparing build performance
    duration: f64,

    /// Time spent generating .rmeta files (optional)
    /// Example: Some(1.2), None
    /// Obtained from: cargo build timing measurements
    /// Used for: Advanced build timing analysis
    #[serde(default)]
    rmeta_time: Option<f64>,
}

/// Build timing information for a single crate
#[derive(Debug)]
struct CrateTiming {
    /// Name of the crate
    /// Example: "ks_facet", "serde", "tokio"
    /// Obtained from: cargo build --timings output, with hyphens converted to underscores
    /// Used for: Identifying which crate took how long to build
    name: String,

    /// Time taken to compile this crate in seconds
    /// Example: 2.5, 0.8, 12.3
    /// Obtained from: cargo build timing measurements
    /// Used for: Comparing build performance between crates and variants
    duration: f64,
}

/// Summary of build timing information for all crates
#[derive(Debug)]
struct BuildTimingSummary {
    /// Total time for the entire build process in seconds
    /// Example: 15.7, 45.2, 120.8
    /// Obtained from: Measuring elapsed time during cargo build
    /// Used for: Overall build performance comparison between variants
    total_duration: f64,

    /// Per-crate timing information, sorted by duration (descending)
    /// Obtained from: cargo build --timings JSON output
    /// Used for: Identifying which crates are slowest to build
    crate_timings: Vec<CrateTiming>,
}

/// LLVM IR function information from cargo llvm-lines analysis
#[derive(Debug)]
struct LlvmFunction {
    /// Function name (may be mangled)
    /// Example: "ks_facet::serialize_data", "_ZN8ks_facet9serialize17h123456789abcdefE"
    /// Obtained from: cargo llvm-lines output parsing
    /// Used for: Identifying functions that generate most LLVM IR
    name: String,

    /// Number of LLVM IR lines for this function
    /// Example: 150, 89, 2341
    /// Obtained from: cargo llvm-lines analysis of compiled output
    /// Used for: Measuring code complexity and compilation overhead
    lines: u32,

    /// Number of copies/instances of this function
    /// Example: 1, 5, 23
    /// Obtained from: cargo llvm-lines analysis (monomorphization count)
    /// Used for: Identifying functions with high monomorphization overhead
    copies: u32,
}

/// LLVM IR lines summary for a single crate
#[derive(Debug)]
struct CrateLlvmLines {
    /// Name of the crate
    /// Example: "ks_facet", "serde", "std"
    /// Obtained from: cargo llvm-lines analysis per crate
    /// Used for: Grouping LLVM complexity by crate
    name: String,

    /// Total LLVM IR lines generated by this crate
    /// Example: 1250, 890, 15600
    /// Obtained from: Aggregating cargo llvm-lines output for each crate
    /// Used for: Measuring per-crate code complexity
    lines: u32,

    /// Total function copies/instances in this crate
    /// Example: 45, 23, 156
    /// Obtained from: Aggregating cargo llvm-lines monomorphization data
    /// Used for: Measuring per-crate monomorphization overhead
    copies: u32,
}

/// Complete LLVM lines analysis summary across all measured crates
#[derive(Debug)]
struct LlvmLinesSummary {
    /// LLVM IR summary for each crate
    /// Obtained from: Running cargo llvm-lines on each crate individually
    /// Used for: Per-crate complexity comparison and total complexity calculation
    crate_results: Vec<CrateLlvmLines>,

    /// Top functions by LLVM IR line count across all crates
    /// Obtained from: Aggregating and sorting all functions from cargo llvm-lines
    /// Used for: Identifying the most complex individual functions
    top_functions: Vec<LlvmFunction>,
}

/// Complete measurement results for a single target/variant combination
#[derive(Debug)]
struct BuildResult {
    /// Name of the measurement target
    /// Example: "ks-facet", "ks-serde", "json-benchmark"
    /// Obtained from: MeasurementTarget configuration
    /// Used for: Grouping results and generating reports
    target: String,

    /// Variant being measured
    /// Example: "facet-pr", "facet-main", "serde"
    /// Obtained from: Command line argument or comparison setup
    /// Used for: Comparing different versions/implementations
    variant: String,

    /// Total binary file size in bytes
    /// Example: 2097152, 1548576, 3145728
    /// Obtained from: cargo bloat file-size measurement
    /// Used for: High-level binary size comparison
    file_size: u64,

    /// Size of executable code section in bytes
    /// Example: 1048576, 987234, 1234567
    /// Obtained from: cargo bloat text-section-size measurement
    /// Used for: Measuring actual code size excluding metadata
    text_section_size: u64,

    /// Total build time in milliseconds
    /// Example: 15700, 45200, 120800
    /// Obtained from: Measuring elapsed time during cargo build
    /// Used for: Build performance comparison between variants
    build_time_ms: u64,

    /// Top functions contributing to binary size
    /// Obtained from: cargo bloat function analysis
    /// Used for: Detailed function-level size analysis in reports
    top_functions: Vec<BloatFunction>,

    /// Top crates contributing to binary size
    /// Obtained from: cargo bloat --crates analysis
    /// Used for: High-level crate size comparison in reports
    top_crates: Vec<BloatCrate>,

    /// Complete LLVM IR complexity analysis
    /// Obtained from: cargo llvm-lines analysis across all relevant crates
    /// Used for: Code complexity comparison and monomorphization analysis
    llvm_lines: LlvmLinesSummary,
}

fn main() -> Result<()> {
    let cli = Cli::parse();

    match cli.command {
        Commands::Compare => run_comparison(),
    }
}

fn run_comparison() -> Result<()> {
    println!("🚀 Starting full comparison...");

    // Setup output directory
    let output = PathBuf::from("bloat-results");
    println!("Output directory: {}", output.display());
    fs::create_dir_all(&output).context("Failed to create output directory")?;

    // Define measurement targets
    let targets = get_measurement_targets();

    let mut all_results = Vec::new();

    // Measure ks-facet with both PR and main variants
    let facet_target = &targets[0]; // ks-facet target
    for &variant in &["facet-pr", "facet-main"] {
        println!("\n🔄 Measuring {} with {}", facet_target.name, variant);
        setup_cargo_patches(variant)?;
        match measure_target_complete(facet_target, variant) {
            Ok(result) => {
                println!("✅ Measurement complete");
                all_results.push(result);
            }
            Err(e) => {
                println!("❌ Measurement failed: {:?}", e);
                std::process::exit(1);
            }
        }
        cleanup_cargo_patches()?;
    }

    // Measure ks-serde
    let serde_target = &targets[1]; // ks-serde target
    let variant = "serde";
    println!("\n🔄 Measuring {} with {}", serde_target.name, variant);
    setup_cargo_patches(variant)?;
    match measure_target_complete(serde_target, variant) {
        Ok(result) => {
            println!("✅ Measurement complete");
            all_results.push(result);
        }
        Err(e) => {
            println!("❌ Measurement failed: {:?}", e);
            std::process::exit(1);
        }
    }
    cleanup_cargo_patches()?;

    let report_path = output.join("comparison_report.md");
    generate_comparison_report(&all_results, &report_path)?;

    println!("\n🎉 Comparison complete!");
    println!("📄 Report generated: {}", report_path.display());

    Ok(())
}

fn get_measurement_targets() -> Vec<MeasurementTarget> {
    vec![
        // ks-facet target (measured with facet-pr and facet-main variants)
        MeasurementTarget {
            name: "ks-facet".to_string(),
            facet_crates: vec![
                "ks-facet".to_string(),
                "ks-mock".to_string(),
                "ks-types".to_string(),
                "ks-facet-json-read".to_string(),
                "ks-facet-json-write".to_string(),
                "ks-facet-pretty".to_string(),
            ],
            serde_crates: vec![], // Not used for this target
            binary_crate: "ks-facet".to_string(),
        },
        // ks-serde target (measured with serde variant)
        MeasurementTarget {
            name: "ks-serde".to_string(),
            facet_crates: vec![], // Not used for this target
            serde_crates: vec![
                "ks-serde".to_string(),
                "ks-mock".to_string(),
                "ks-types".to_string(),
                "ks-serde-json-read".to_string(),
                "ks-serde-json-write".to_string(),
                "ks-debug".to_string(),
            ],
            binary_crate: "ks-serde".to_string(),
        },
    ]
}

fn setup_cargo_patches(variant: &str) -> Result<()> {
    match variant {
        "facet-pr" => {
            // No changes needed - use current state
            println!("✅ Using current facet PR state (no changes needed)");
        }
        "facet-main" => {
            // Modify Cargo.toml files to use git dependencies from main branch
            modify_cargo_tomls_for_main_branch()?;
            println!("✅ Modified Cargo.toml files for facet-main variant");
        }
        "serde" => {
            // Serde variant uses current state - no changes needed
            println!("✅ Using current serde implementation (no changes needed)");
        }
        _ => {
            anyhow::bail!("Unknown variant: {}", variant);
        }
    }
    Ok(())
}

fn modify_cargo_tomls_for_main_branch() -> Result<()> {
    println!("✅ Creating temp workspace for facet-main");

    // Create unique temp directory outside source tree
    let temp_dir = std::env::temp_dir().join(format!("measure-bloat-{}", process::id()));
    let source_workspace = PathBuf::from("..");
    let temp_workspace = temp_dir.join("outside-workspace");

    // Remove existing temp directory if it exists
    if temp_dir.exists() {
        fs::remove_dir_all(&temp_dir)
            .context(format!("Failed to remove existing {}", temp_dir.display()))?;
    }

    // Create temp directory
    fs::create_dir_all(&temp_dir).context(format!(
        "Failed to create temp directory {}",
        temp_dir.display()
    ))?;

    // Copy entire outside-workspace to temp location
    copy_dir_recursive(&source_workspace, &temp_workspace)?;

    // Store temp workspace path for later use
    let temp_workspace_path = temp_workspace.to_string_lossy().to_string();
    println!("📁 Temp workspace created at: {}", temp_workspace_path);

    // Patch all Cargo.toml files in the temp workspace
    let crates_to_patch = vec![
        "ks-facet-json-read",
        "ks-facet-json-write",
        "ks-facet-pretty",
        "ks-mock",
        "ks-types",
        "ks-facet",
    ];

    for crate_name in crates_to_patch {
        let cargo_toml_path = temp_workspace.join(crate_name).join("Cargo.toml");

        if cargo_toml_path.exists() {
            // Read original content
            let original_content = fs::read_to_string(&cargo_toml_path)
                .context(format!("Failed to read {}", cargo_toml_path.display()))?;

            // Replace local facet dependencies with git dependencies
            let modified_content = replace_facet_deps_with_git(&original_content)?;

            // Write modified content back
            fs::write(&cargo_toml_path, modified_content)
                .context(format!("Failed to write {}", cargo_toml_path.display()))?;
        }
    }

    Ok(())
}

fn copy_dir_recursive(src: &PathBuf, dst: &PathBuf) -> Result<()> {
    if !src.exists() {
        return Err(anyhow::anyhow!(
            "Source directory does not exist: {}",
            src.display()
        ));
    }

    fs::create_dir_all(dst).context(format!("Failed to create directory {}", dst.display()))?;

    for entry in fs::read_dir(src).context(format!("Failed to read directory {}", src.display()))? {
        let entry = entry.context("Failed to read directory entry")?;
        let src_path = entry.path();
        let dst_path = dst.join(entry.file_name());

        if src_path.is_dir() {
            copy_dir_recursive(&src_path, &dst_path)?;
        } else {
            fs::copy(&src_path, &dst_path).context(format!(
                "Failed to copy {} to {}",
                src_path.display(),
                dst_path.display()
            ))?;
        }
    }

    Ok(())
}

fn replace_facet_deps_with_git(content: &str) -> Result<String> {
    let mut doc = content
        .parse::<DocumentMut>()
        .context("Failed to parse TOML")?;

    println!("🔍 Original TOML content:\n{}", content.dimmed());

    // List of facet crates to replace
    let facet_crates = vec![
        "facet",
        "facet-core",
        "facet-reflect",
        "facet-macros",
        "facet-deserialize",
        "facet-serialize",
        "facet-json",
        "facet-yaml",
        "facet-pretty",
    ];

    // Check dependencies section
    if let Some(deps) = doc
        .get_mut("dependencies")
        .and_then(|item| item.as_table_mut())
    {
        println!("🔍 Found dependencies section");
        for crate_name in &facet_crates {
            if let Some(dep) = deps.get_mut(crate_name) {
                println!("🔍 Found dependency: {}", crate_name);
                if let Some(dep_table) = dep.as_inline_table_mut() {
                    // If it has a path dependency, replace with git
                    if dep_table.contains_key("path") {
                        println!("✅ Replacing inline table dependency: {}", crate_name);
                        // Remove path and version, add git and branch
                        dep_table.remove("path");
                        dep_table.remove("version");
                        dep_table.insert("git", Value::from("https://github.com/facet-rs/facet"));
                        dep_table.insert("branch", Value::from("main"));
                    }
                } else if let Some(dep_table) = dep.as_table_mut() {
                    // Handle table format dependencies
                    if dep_table.contains_key("path") {
                        println!("✅ Replacing table dependency: {}", crate_name);
                        dep_table.remove("path");
                        dep_table.remove("version");
                        dep_table.insert(
                            "git",
                            Item::Value(Value::from("https://github.com/facet-rs/facet")),
                        );
                        dep_table.insert("branch", Item::Value(Value::from("main")));
                    }
                }
            }
        }
    }

    // Check dev-dependencies section
    if let Some(deps) = doc
        .get_mut("dev-dependencies")
        .and_then(|item| item.as_table_mut())
    {
        for crate_name in &facet_crates {
            if let Some(dep) = deps.get_mut(crate_name) {
                if let Some(dep_table) = dep.as_inline_table_mut() {
                    if dep_table.contains_key("path") {
                        dep_table.remove("path");
                        dep_table.remove("version");
                        dep_table.insert("git", Value::from("https://github.com/facet-rs/facet"));
                        dep_table.insert("branch", Value::from("main"));
                    }
                } else if let Some(dep_table) = dep.as_table_mut() {
                    if dep_table.contains_key("path") {
                        dep_table.remove("path");
                        dep_table.remove("version");
                        dep_table.insert(
                            "git",
                            Item::Value(Value::from("https://github.com/facet-rs/facet")),
                        );
                        dep_table.insert("branch", Item::Value(Value::from("main")));
                    }
                }
            }
        }
    }

    let result = doc.to_string();
    println!("🔍 Modified TOML content:\n{}", result.dimmed());
    Ok(result)
}

fn cleanup_cargo_patches() -> Result<()> {
    // Remove temp directory
    let temp_dir = std::env::temp_dir().join(format!("measure-bloat-{}", process::id()));

    if temp_dir.exists() {
        fs::remove_dir_all(&temp_dir).context(format!(
            "Failed to remove temp directory {}",
            temp_dir.display()
        ))?;
    }

    println!("🧹 Cleaned up temp workspace");
    Ok(())
}

fn measure_target_complete(target: &MeasurementTarget, variant: &str) -> Result<BuildResult> {
    let crates_to_use = match variant {
        "serde" => &target.serde_crates,
        "facet-pr" | "facet-main" => &target.facet_crates,
        _ => {
            anyhow::bail!("Unknown variant: {}", variant);
        }
    };

    // Use the binary crate as the measurement target, with temp workspace for facet-main
    let manifest_path = match variant {
        "facet-main" => {
            let temp_dir = std::env::temp_dir().join(format!("measure-bloat-{}", process::id()));
            format!(
                "{}/outside-workspace/{}/Cargo.toml",
                temp_dir.to_string_lossy(),
                target.binary_crate
            )
        }
        _ => format!("../{}/Cargo.toml", target.binary_crate),
    };

    // Run measurements
    let start = Instant::now();

    // Define a persistent target directory to be used for all builds in this function
    let persistent_target_dir = PathBuf::from("../target-measure-bloat");
    let _ = fs::remove_dir_all(&persistent_target_dir);
    if fs::exists(&persistent_target_dir).unwrap() {
        panic!(
            "Failed to remove persistent target directory: {}",
            persistent_target_dir.display()
        );
    }
    fs::create_dir_all(&persistent_target_dir).context(format!(
        "Failed to create persistent target directory: {}",
        persistent_target_dir.display()
    ))?;
    let persistent_target_dir_str = persistent_target_dir.to_string_lossy().to_string();

    let mut consistent_env_vars = std::collections::HashMap::new();
    consistent_env_vars.insert("RUSTC_BOOTSTRAP".to_string(), "1".to_string());
    consistent_env_vars.insert("RUSTFLAGS".to_string(), "--emit=llvm-ir".to_string());

    // --- Build with LLVM IR emission and timing ---
    println!("🔨 Building with LLVM IR emission...");
    let llvm_ir_opts = BuildWithLllvmIrOpts {
        manifest_path: manifest_path.clone(),
        target_dir: persistent_target_dir_str.clone(), // Use persistent target dir
        env_vars: consistent_env_vars.clone(),         // See comment above.
    };
    let build_output = build_with_llvm_ir(&llvm_ir_opts)?; // This build uses persistent_target_dir_str

    // --- Analyze LLVM files from the build ---
    println!("📊 Analyzing LLVM lines...");
    // `build_output.target_dir` is the `persistent_target_dir_str`
    let llvm_lines = analyze_llvm_files(&build_output.target_dir, crates_to_use)?;

    // --- Measure binary size with cargo-bloat ---
    println!("📏 Measuring binary size...");

    // For `cargo bloat` to reuse artifacts and avoid rebuilds, its internal `cargo build`
    // (if triggered) must see the same RUSTFLAGS and other relevant environment variables
    // that `build_with_llvm_ir` effectively used.
    // `build_with_llvm_ir` effectively uses: RUSTC_BOOTSTRAP="1", RUSTFLAGS="--emit=llvm-ir".

    let bloat_functions_opts = CargoBloatOpts {
        manifest_path: manifest_path.clone(),
        target_dir: Some(persistent_target_dir_str.clone()), // Use persistent target dir
        mode: CargoBloatMode::Functions,
        env_vars: consistent_env_vars.clone(),
    };
    let bloat_functions = run_cargo_bloat(&bloat_functions_opts)?;

    let bloat_crates_opts = CargoBloatOpts {
        manifest_path: manifest_path.clone(),
        target_dir: Some(persistent_target_dir_str.clone()), // Use persistent target dir
        mode: CargoBloatMode::Crates,
        env_vars: consistent_env_vars,
    };
    let bloat_crates = run_cargo_bloat(&bloat_crates_opts)?;

    // --- Do NOT clean up the persistent build directory ---
    // The previous lines that removed build_output.target_dir (which was temporary)
    // and target_dir_bloat are no longer needed as we use a persistent directory.

    let measurement_duration = start.elapsed();
    println!(
        "⏰ Total measurement time: {:.2}s",
        measurement_duration.as_secs_f64()
    );

    Ok(BuildResult {
        target: target.name.clone(),
        variant: variant.to_string(),
        file_size: bloat_functions.file_size,
        text_section_size: bloat_functions.text_section_size,
        build_time_ms: (build_output.timing_summary.total_duration * 1000.0) as u64,
        top_functions: bloat_functions.functions,
        top_crates: bloat_crates.crates,
        llvm_lines,
    })
}

fn aggregate_and_sort_functions(bloat_functions: &[BloatFunction]) -> Vec<BloatFunction> {
    let mut aggregated_map: std::collections::HashMap<(String, String), u64> =
        std::collections::HashMap::new();
    for func in bloat_functions {
        *aggregated_map
            .entry((func.crate_name.clone(), func.name.clone()))
            .or_insert(0) += func.size;
    }
    let mut aggregated_list: Vec<BloatFunction> = aggregated_map
        .into_iter()
        .map(|((crate_name, name), size)| BloatFunction {
            crate_name,
            name,
            size,
        })
        .collect();
    aggregated_list.sort_by(|a, b| b.size.cmp(&a.size)); // Sort by size descending
    aggregated_list
}

fn generate_comparison_report(results: &[BuildResult], report_path: &PathBuf) -> Result<()> {
    let mut report = String::new();

    report.push_str("# Facet vs Serde Comparison Report\n\n");
    report.push_str(&format!(
        "Generated on: {}\n\n",
        chrono::Utc::now().format("%Y-%m-%d %H:%M:%S UTC")
    ));

    // Group results by target, but we'll create a unified table
    let mut targets: std::collections::HashMap<String, Vec<&BuildResult>> =
        std::collections::HashMap::new();
    for result in results {
        targets
            .entry(result.target.clone())
            .or_default()
            .push(result);
    }

    // Find all variants across targets for unified table
    let empty_facet = vec![];
    let empty_serde = vec![];
    let facet_results = targets.get("ks-facet").unwrap_or(&empty_facet);
    let serde_results = targets.get("ks-serde").unwrap_or(&empty_serde);

    // Create unified comparison - use ks-facet as the main target name
    let main_target_name = "ks-facet";
    report.push_str(&format!("## {}\n\n", main_target_name));

    // Find facet results for diff analysis
    let facet_pr = facet_results.iter().find(|r| r.variant == "facet-pr");
    let facet_main = facet_results.iter().find(|r| r.variant == "facet-main");
    let serde_result = serde_results.iter().find(|r| r.variant == "serde");

    // Create unified sorted results: facet-main, facet-pr, then serde
    let mut all_results = Vec::new();
    if let Some(main) = facet_main {
        all_results.push(*main);
    }
    if let Some(pr) = facet_pr {
        all_results.push(*pr);
    }
    if let Some(serde) = serde_result {
        all_results.push(*serde);
    }

    // Summary table - show deltas for facet variants, no deltas for serde
    report.push_str(
        "| Variant | File Size | Δ | Text Size | Δ | Build Time | Δ | LLVM Lines | Δ |\n",
    );
    report.push_str(
        "|---------|-----------|---|-----------|---|------------|---|------------|---|\n",
    );

    // Find facet-main as baseline for deltas
    let baseline_result = all_results.iter().find(|r| r.variant == "facet-main");
    let baseline_llvm_total: Option<u32> = baseline_result.map(|result| {
        result
            .llvm_lines
            .crate_results
            .iter()
            .map(|crate_llvm| crate_llvm.lines)
            .sum()
    });

    for result in all_results.iter() {
        let total_llvm_lines: u32 = result
            .llvm_lines
            .crate_results
            .iter()
            .map(|crate_llvm| crate_llvm.lines)
            .sum();

        if result.variant == "facet-main" {
            // Baseline - no deltas
            report.push_str(&format!(
                "| {} | {} | - | {} | - | {:.2}s | - | {} | - |\n",
                result.variant,
                format_bytes(result.file_size),
                format_bytes(result.text_section_size),
                result.build_time_ms as f64 / 1000.0,
                format_number(total_llvm_lines)
            ));
        } else if result.variant == "facet-pr" && baseline_result.is_some() {
            // Calculate deltas from facet-main baseline
            let baseline = baseline_result.unwrap();
            let file_size_delta = result.file_size as i64 - baseline.file_size as i64;
            let text_size_delta =
                result.text_section_size as i64 - baseline.text_section_size as i64;
            let build_time_delta = result.build_time_ms as i64 - baseline.build_time_ms as i64;
            let llvm_lines_delta = total_llvm_lines as i64 - baseline_llvm_total.unwrap() as i64;

            let file_emoji = if file_size_delta > 0 {
                "📈"
            } else if file_size_delta < 0 {
                "📉"
            } else {
                "➖"
            };
            let text_emoji = if text_size_delta > 0 {
                "📈"
            } else if text_size_delta < 0 {
                "📉"
            } else {
                "➖"
            };
            let time_emoji = if build_time_delta > 0 {
                "📈"
            } else if build_time_delta < 0 {
                "📉"
            } else {
                "➖"
            };
            let llvm_emoji = if llvm_lines_delta > 0 {
                "📈"
            } else if llvm_lines_delta < 0 {
                "📉"
            } else {
                "➖"
            };

            report.push_str(&format!(
                "| {} | {} | {}{} | {} | {}{} | {:.2}s | {}{:.2}s | {} | {}{} |\n",
                result.variant,
                format_bytes(result.file_size),
                file_emoji,
                format_signed_bytes(file_size_delta),
                format_bytes(result.text_section_size),
                text_emoji,
                format_signed_bytes(text_size_delta),
                result.build_time_ms as f64 / 1000.0,
                time_emoji,
                build_time_delta as f64 / 1000.0,
                format_number(total_llvm_lines),
                llvm_emoji,
                llvm_lines_delta
            ));
        } else {
            // Serde or other variants - no deltas
            report.push_str(&format!(
                "| {} | {} | - | {} | - | {:.2}s | - | {} | - |\n",
                result.variant,
                format_bytes(result.file_size),
                format_bytes(result.text_section_size),
                result.build_time_ms as f64 / 1000.0,
                format_number(total_llvm_lines)
            ));
        }
    }

    report.push('\n');

    // Add diff analysis if we have both facet-pr and facet-main
    if let (Some(pr_result), Some(main_result)) = (facet_pr, facet_main) {
        generate_facet_diff_analysis(&mut report, pr_result, main_result);
    }

    // Detailed breakdown for each target and variant
    for (target_name, target_results) in targets {
        // Sort results for consistent ordering
        let mut sorted_results = target_results.clone();
        sorted_results.sort_by(|a, b| match (a.variant.as_str(), b.variant.as_str()) {
            ("facet-main", _) => std::cmp::Ordering::Less,
            (_, "facet-main") => std::cmp::Ordering::Greater,
            ("facet-pr", _) => std::cmp::Ordering::Less,
            (_, "facet-pr") => std::cmp::Ordering::Greater,
            _ => a.variant.cmp(&b.variant),
        });

        for result in &sorted_results {
            report.push_str(&format!("### {} - {}\n\n", target_name, result.variant));

            report.push_str("**Top Functions by Size:**\n");
            let aggregated_functions = aggregate_and_sort_functions(&result.top_functions);
            for (i, func) in aggregated_functions.iter().take(10).enumerate() {
                report.push_str(&format!(
                    "{}. `{}::{}` - {}\n",
                    i + 1,
                    func.crate_name,
                    func.name,
                    format_bytes(func.size)
                ));
            }

            report.push_str("\n**LLVM Lines by Crate:**\n");
            for crate_llvm in &result.llvm_lines.crate_results {
                report.push_str(&format!(
                    "- `{}`: {} lines ({} copies)\n",
                    crate_llvm.name,
                    format_number(crate_llvm.lines),
                    format_number(crate_llvm.copies)
                ));
            }

            report.push('\n');
        }
    }

    fs::write(report_path, report).context("Failed to write comparison report")?;
    Ok(())
}

fn generate_facet_diff_analysis(
    report: &mut String,
    pr_result: &BuildResult,
    main_result: &BuildResult,
) {
    report.push_str("### 🔍 PR vs Main Branch Analysis\n\n");

    // Calculate deltas for Key Findings section
    let file_size_delta = pr_result.file_size as i64 - main_result.file_size as i64;
    let text_size_delta = pr_result.text_section_size as i64 - main_result.text_section_size as i64;
    let build_time_delta = pr_result.build_time_ms as i64 - main_result.build_time_ms as i64;

    let pr_llvm_total: u32 = pr_result
        .llvm_lines
        .crate_results
        .iter()
        .map(|crate_llvm| crate_llvm.lines)
        .sum();
    let main_llvm_total: u32 = main_result
        .llvm_lines
        .crate_results
        .iter()
        .map(|crate_llvm| crate_llvm.lines)
        .sum();
    let llvm_lines_delta = pr_llvm_total as i64 - main_llvm_total as i64;

    // Function-level diff analysis
    generate_function_diff_analysis(report, pr_result, main_result);

    // LLVM crate-level diff analysis
    generate_llvm_crate_diff_analysis(report, pr_result, main_result);

    // bloat analysis
    generate_crate_diff_analysis(report, pr_result, main_result);

    // Regression/improvement highlights
    generate_highlights(
        report,
        file_size_delta,
        text_size_delta,
        build_time_delta,
        llvm_lines_delta,
    );
}

fn generate_function_diff_analysis(
    report: &mut String,
    pr_result: &BuildResult,
    main_result: &BuildResult,
) {
    report.push_str("**Function Size Changes (Top 30):**\n\n");

    // Create maps for easy lookup
    let mut main_funcs: std::collections::HashMap<String, u64> = std::collections::HashMap::new();
    for func in &main_result.top_functions {
        let key = format!("{}::{}", func.crate_name, func.name);
        let mut size = func.size;
        if let Some(old) = main_funcs.remove(&key) {
            size += old;
        }
        main_funcs.insert(key, size);
    }

    let mut pr_funcs: std::collections::HashMap<String, u64> = std::collections::HashMap::new();
    for func in &pr_result.top_functions {
        let key = format!("{}::{}", func.crate_name, func.name);
        let mut size = func.size;
        if let Some(old) = pr_funcs.remove(&key) {
            size += old;
        }
        pr_funcs.insert(key, size);
    }

    let mut top_pr_funcs: Vec<(String, u64)> = pr_funcs
        .iter()
        .map(|(name, size)| (name.clone(), *size))
        .collect();
    top_pr_funcs.sort_by(|a, b| b.1.cmp(&a.1));

    let mut top_main_funcs: Vec<(String, u64)> = main_funcs
        .iter()
        .map(|(name, size)| (name.clone(), *size))
        .collect();
    top_main_funcs.sort_by(|a, b| b.1.cmp(&a.1));

    struct FunctionChange {
        key: String,
        main_size: u64,
        pr_size: u64,
        delta: i64,
    }

    // Collect all function changes
    let mut function_changes: Vec<FunctionChange> = Vec::new();

    // Check functions in PR
    for func in &pr_result.top_functions {
        let key = format!("{}::{}", func.crate_name, func.name);
        let pr_size = pr_funcs.get(&key).copied().unwrap_or(0); // Use aggregated size from pr_funcs
        let main_size = main_funcs.get(&key).copied().unwrap_or(0);
        let delta = pr_size as i64 - main_size as i64;

        if delta != 0 {
            // Add or update entry. Since we iterate pr_result.top_functions,
            // we are effectively creating entries for all functions present in PR.
            // If a function is only in PR, main_size will be 0.
            // If a function is in both, delta will be calculated correctly.
            function_changes.push(FunctionChange {
                key: key.clone(),
                main_size,
                pr_size,
                delta,
            });
        } else if main_size == 0 && pr_size > 0 {
            // Function new in PR, but delta was 0 (this case might be redundant if pr_size > 0 implies delta != 0 if main_size is 0)
            // This ensures new functions are captured even if their size is 0 (unlikely but possible)
            // More robustly: if it's in pr_funcs but not main_funcs.
            // However, the current pr_funcs iteration covers this.
            // Let's refine the logic for adding:
            // Add if it's in PR. The delta calculation will handle new/changed/unchanged.
        }
    }

    // More robust way to collect changes:
    let mut processed_keys = std::collections::HashSet::new();
    function_changes.clear(); // Clear previous attempt, start fresh

    // Iterate over all functions in PR result
    for (key, &pr_size) in &pr_funcs {
        let main_size = main_funcs.get(key).copied().unwrap_or(0);
        let delta = pr_size as i64 - main_size as i64;
        function_changes.push(FunctionChange {
            key: key.clone(),
            main_size,
            pr_size,
            delta,
        });
        processed_keys.insert(key.clone());
    }

    // Iterate over all functions in Main result to find those not in PR (disappeared)
    for (key, &main_size) in &main_funcs {
        if !processed_keys.contains(key) {
            // This function was in main but not in PR
            function_changes.push(FunctionChange {
                key: key.clone(),
                main_size,
                pr_size: 0,
                delta: -(main_size as i64),
            });
        }
    }

    // Sort by absolute delta size
    function_changes.sort_by_key(|fc| std::cmp::Reverse(fc.delta.abs()));

    if function_changes.is_empty() {
        report.push_str("*No function size changes detected.*\n\n");
    } else {
        report.push_str("| Function | Main | PR | Change |\n");
        report.push_str("|----------|------|----|---------|\n");

        for change in function_changes.iter().filter(|fc| fc.delta != 0).take(30) {
            let main_str = if change.main_size == 0 {
                "N/A".to_string()
            } else {
                format_bytes(change.main_size)
            };
            let pr_str = if change.pr_size == 0 {
                "N/A".to_string()
            } else {
                format_bytes(change.pr_size)
            };

            let emoji = if change.delta > 0 {
                "📈"
            } else if change.delta < 0 {
                "📉"
            } else {
                // This case should be filtered out by .filter(|fc| fc.delta != 0)
                "➖"
            };

            report.push_str(&format!(
                "| `{}` | {} | {} | {}{} |\n",
                change.key,
                main_str,
                pr_str,
                emoji,
                format_signed_bytes(change.delta)
            ));
        }
        report.push('\n');
    }
}

fn generate_llvm_crate_diff_analysis(
    report: &mut String,
    pr_result: &BuildResult,
    main_result: &BuildResult,
) {
    report.push_str("**LLVM Lines Changes by Crate (All Crates):**\n\n");

    // Create maps for crate LLVM lines
    let mut main_crates: std::collections::HashMap<String, (u32, u32)> =
        std::collections::HashMap::new();
    for crate_llvm in &main_result.llvm_lines.crate_results {
        main_crates.insert(
            crate_llvm.name.clone(),
            (crate_llvm.lines, crate_llvm.copies),
        );
    }

    let mut pr_crates: std::collections::HashMap<String, (u32, u32)> =
        std::collections::HashMap::new();
    for crate_llvm in &pr_result.llvm_lines.crate_results {
        pr_crates.insert(
            crate_llvm.name.clone(),
            (crate_llvm.lines, crate_llvm.copies),
        );
    }

    let mut all_crate_data = Vec::new();

    // Check all crates from both results
    let mut all_crates = std::collections::HashSet::new();
    for crate_llvm in &main_result.llvm_lines.crate_results {
        all_crates.insert(crate_llvm.name.clone());
    }
    for crate_llvm in &pr_result.llvm_lines.crate_results {
        all_crates.insert(crate_llvm.name.clone());
    }

    for crate_name in all_crates {
        let (main_lines, main_copies) = main_crates.get(&crate_name).unwrap_or(&(0, 0));
        let (pr_lines, pr_copies) = pr_crates.get(&crate_name).unwrap_or(&(0, 0));

        let lines_delta = *pr_lines as i64 - *main_lines as i64;
        let copies_delta = *pr_copies as i64 - *main_copies as i64;

        all_crate_data.push((
            crate_name,
            (*main_lines, *main_copies),
            (*pr_lines, *pr_copies),
            lines_delta,
            copies_delta,
        ));
    }

    // Sort by absolute lines delta (largest changes first), then by crate name
    all_crate_data.sort_by(|a, b| {
        let delta_cmp = b.3.abs().cmp(&a.3.abs());
        if delta_cmp == std::cmp::Ordering::Equal {
            a.0.cmp(&b.0)
        } else {
            delta_cmp
        }
    });

    if all_crate_data.is_empty() {
        report.push_str("*No crates found.*\n\n");
    } else {
        report.push_str("| Crate | Main Lines | PR Lines | Lines Δ | Copies Δ |\n");
        report.push_str("|-------|------------|----------|---------|----------|\n");

        for (
            crate_name,
            (main_lines, main_copies),
            (pr_lines, pr_copies),
            lines_delta,
            copies_delta,
        ) in all_crate_data
        {
            let main_lines_str = if main_lines == 0 {
                "N/A".to_string()
            } else {
                format_number(main_lines)
            };
            let pr_lines_str = if pr_lines == 0 {
                "N/A".to_string()
            } else {
                format_number(pr_lines)
            };

            let lines_emoji = if lines_delta > 0 {
                "📈"
            } else if lines_delta < 0 {
                "📉"
            } else {
                "➖"
            };

            let copies_emoji = if copies_delta > 0 {
                "📈"
            } else if copies_delta < 0 {
                "📉"
            } else {
                "➖"
            };

            let lines_delta_str = if lines_delta == 0 {
                "0".to_string()
            } else {
                format!("{:+}", lines_delta)
            };

            let copies_delta_str = if copies_delta == 0 {
                "0".to_string()
            } else {
                format!("{:+}", copies_delta)
            };

            report.push_str(&format!(
                "| `{}` | {} ({}) | {} ({}) | {}{} | {}{} |\n",
                crate_name,
                main_lines_str,
                format_number(main_copies),
                pr_lines_str,
                format_number(pr_copies),
                lines_emoji,
                lines_delta_str,
                copies_emoji,
                copies_delta_str
            ));
        }
        report.push('\n');
    }
}

struct CrateSizeChange {
    name: String,
    main_size: u64,
    pr_size: u64,
    delta: i64,
}

fn generate_crate_diff_analysis(
    report: &mut String,
    pr_result: &BuildResult,
    main_result: &BuildResult,
) {
    report.push_str("**Crate Size Changes (PR vs Main):**\n\n");

    let mut main_crates_map: std::collections::HashMap<String, u64> =
        std::collections::HashMap::new();
    for crate_data in &main_result.top_crates {
        main_crates_map.insert(crate_data.name.clone(), crate_data.size);
    }

    let mut pr_crates_map: std::collections::HashMap<String, u64> =
        std::collections::HashMap::new();
    for crate_data in &pr_result.top_crates {
        pr_crates_map.insert(crate_data.name.clone(), crate_data.size);
    }

    let mut all_crate_names = std::collections::HashSet::new();
    for crate_data in &main_result.top_crates {
        all_crate_names.insert(crate_data.name.clone());
    }
    for crate_data in &pr_result.top_crates {
        all_crate_names.insert(crate_data.name.clone());
    }

    let mut crate_size_changes = Vec::new();

    for crate_name in all_crate_names {
        let main_size = *main_crates_map.get(&crate_name).unwrap_or(&0);
        let pr_size = *pr_crates_map.get(&crate_name).unwrap_or(&0);
        let delta = pr_size as i64 - main_size as i64;

        crate_size_changes.push(CrateSizeChange {
            name: crate_name,
            main_size,
            pr_size,
            delta,
        });
    }

    // Sort by absolute delta (largest changes first), then by crate name
    crate_size_changes.sort_by(|a, b| {
        let delta_cmp = b.delta.abs().cmp(&a.delta.abs());
        if delta_cmp == std::cmp::Ordering::Equal {
            a.name.cmp(&b.name)
        } else {
            delta_cmp
        }
    });

    if crate_size_changes.is_empty() {
        report.push_str("*No crate data found for comparison.*\n\n");
    } else {
        report.push_str("| Crate | Main Size | PR Size | Δ Size |\n");
        report.push_str("|-------|-----------|---------|--------|\n");

        for change in crate_size_changes {
            let main_size_str = if change.main_size == 0 && change.pr_size > 0 {
                "N/A (New)".to_string()
            } else if change.main_size == 0 {
                "N/A".to_string()
            } else {
                format_bytes(change.main_size)
            };

            let pr_size_str = if change.pr_size == 0 && change.main_size > 0 {
                "N/A (Removed)".to_string()
            } else if change.pr_size == 0 {
                "N/A".to_string()
            } else {
                format_bytes(change.pr_size)
            };

            let emoji = if change.main_size == 0 && change.pr_size > 0 {
                "🆕" // New
            } else if change.pr_size == 0 && change.main_size > 0 {
                "🗑️" // Removed
            } else if change.delta > 0 {
                "📈"
            } else if change.delta < 0 {
                "📉"
            } else {
                "➖"
            };

            report.push_str(&format!(
                "| `{}` | {} | {} | {}{} |\n",
                change.name,
                main_size_str,
                pr_size_str,
                emoji,
                format_signed_bytes(change.delta)
            ));
        }
        report.push('\n');
    }
}

fn generate_highlights(
    report: &mut String,
    file_size_delta: i64,
    _text_size_delta: i64,
    build_time_delta: i64,
    llvm_lines_delta: i64,
) {
    report.push_str("**Key Findings:**\n\n");

    let mut findings = Vec::new();

    // File size analysis
    if file_size_delta.abs() > 1024 {
        // More than 1KB change
        if file_size_delta > 0 {
            findings.push(format!(
                "⚠️  **Binary size increased** by {}",
                format_bytes(file_size_delta as u64)
            ));
        } else {
            findings.push(format!(
                "✅ **Binary size reduced** by {}",
                format_bytes((-file_size_delta) as u64)
            ));
        }
    }

    // Build time analysis
    if build_time_delta.abs() > 100 {
        // More than 100ms change
        if build_time_delta > 0 {
            findings.push(format!(
                "⚠️  **Build time increased** by {:.2}s",
                build_time_delta as f64 / 1000.0
            ));
        } else {
            findings.push(format!(
                "✅ **Build time improved** by {:.2}s",
                (-build_time_delta) as f64 / 1000.0
            ));
        }
    }

    // LLVM complexity analysis
    if llvm_lines_delta.abs() > 1000 {
        // More than 1K lines change
        if llvm_lines_delta > 0 {
            findings.push(format!(
                "⚠️  **Code complexity increased** by {} LLVM lines",
                format_number(llvm_lines_delta as u32)
            ));
        } else {
            findings.push(format!(
                "✅ **Code complexity reduced** by {} LLVM lines",
                format_number((-llvm_lines_delta) as u32)
            ));
        }
    }

    if findings.is_empty() {
        findings.push(
            "📊 **No significant changes detected** - metrics are within normal variance"
                .to_string(),
        );
    }

    for finding in findings {
        report.push_str(&format!("- {}\n", finding));
    }

    report.push('\n');
}

fn format_signed_bytes(bytes: i64) -> String {
    if bytes >= 0 {
        format!("+{}", format_bytes(bytes as u64))
    } else {
        format!("-{}", format_bytes((-bytes) as u64))
    }
}

fn measure_target(target: &MeasurementTarget, variant: &str) -> Result<()> {
    println!(
        "📏 Measuring target: {} with variant: {}",
        target.name, variant
    );

    let crates_to_use = match variant {
        "serde" => &target.serde_crates,
        "facet-pr" | "facet-main" => &target.facet_crates,
        _ => {
            println!("❌ Unknown variant: {}", variant);
            println!("Available variants: serde, facet-pr, facet-main");
            return Ok(());
        }
    };

    println!("📦 Crates to measure: {:?}", crates_to_use);

    // Run the complete measurement
    match measure_target_complete(target, variant) {
        Ok(result) => {
            println!(
                "✅ Measurement complete for {} ({})",
                result.target, result.variant
            );

            // Display summary
            println!("📊 Summary:");
            println!("   File size: {}", format_bytes(result.file_size));
            println!(
                "   Text section size: {}",
                format_bytes(result.text_section_size)
            );
            println!(
                "   Build time: {:.2}s",
                result.build_time_ms as f64 / 1000.0
            );

            let total_llvm_lines: u32 = result
                .llvm_lines
                .crate_results
                .iter()
                .map(|crate_llvm| crate_llvm.lines)
                .sum();
            println!("   Total LLVM lines: {}", format_number(total_llvm_lines));
        }
        Err(e) => {
            println!("❌ Measurement failed: {:?}", e);
            std::process::exit(1);
        }
    }

    Ok(())
}

/// Modes for cargo-bloat: function-level or crate-level analysis
#[derive(Debug, Clone, Copy)]
enum CargoBloatMode {
    Functions,
    Crates,
}

/// Options for running cargo-bloat.
/// Used to configure the cargo-bloat execution.
#[derive(Debug, Clone)]
struct CargoBloatOpts {
    /// Path to the Cargo.toml manifest file for the crate to analyze.
    /// Example: "../ks-facet/Cargo.toml"
    /// Obtained from: MeasurementTarget configuration or manual specification.
    /// Used for: Specifying which crate to analyze.
    manifest_path: String,

    /// Target directory for the build artifacts. If None, cargo's default is used.
    /// Example: Some("../target-bloat-12345")
    /// Obtained from: Dynamically generated or set to None.
    /// Used for: Isolating build outputs if necessary.
    target_dir: Option<String>,

    /// Mode for cargo-bloat: function-level or crate-level analysis.
    /// Obtained from: Internal logic deciding which analysis to run.
    /// Used for: Controlling the type of bloat report generated.
    mode: CargoBloatMode,

    /// Environment variables to set for the cargo-bloat command.
    /// Example: `HashMap::from([("RUSTFLAGS".to_string(), "-Zunstable-options".to_string())])`
    /// Obtained from: Specific measurement requirements.
    /// Used for: Customizing the build environment for cargo-bloat.
    env_vars: std::collections::HashMap<String, String>,
}

fn run_cargo_bloat(opts: &CargoBloatOpts) -> Result<BloatOutput> {
    let pb = indicatif::ProgressBar::new_spinner();
    let style = indicatif::ProgressStyle::default_spinner()
        .tick_chars("⠁⠂⠄⡀⢀⠠⠐⠈ ")
        .template("{spinner:.green} {msg}")
        .expect("BUG: Invalid indicatif template"); // template() on default_spinner should not fail with valid str
    pb.set_style(style);
    pb.set_message(format!("Running cargo bloat ({:?})...", opts.mode));
    pb.enable_steady_tick(std::time::Duration::from_millis(100));

    let start = Instant::now();

    let mut args = vec![
        "bloat".to_string(),
        "--release".to_string(),
        "--message-format".to_string(),
        "json".to_string(),
        "--manifest-path".to_string(),
        opts.manifest_path.clone(),
        "-n".to_string(),
        "500".to_string(), // Keep default -n 500
    ];

    if let Some(target_dir) = &opts.target_dir {
        args.push("--target-dir".to_string());
        args.push(target_dir.clone());
    }

    match opts.mode {
        CargoBloatMode::Crates => args.push("--crates".to_string()),
        CargoBloatMode::Functions => {} // No additional args needed
    }

    let mut command = Command::new("cargo");
    command.args(&args);

    for (key, value) in &opts.env_vars {
        command.env(key, value);
    }

    let output = command.output().context("Failed to execute cargo bloat")?;

    let duration = start.elapsed();
    pb.disable_steady_tick(); // Stop animation before printing final message

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        pb.finish_with_message(format!("❌ cargo bloat ({:?}) failed", opts.mode));
        anyhow::bail!("cargo bloat failed: {}", stderr);
    }

    pb.finish_with_message(format!(
        "✅ cargo bloat ({:?}) completed in {:.2}s",
        opts.mode,
        duration.as_secs_f64()
    ));

    let stdout = String::from_utf8_lossy(&output.stdout);

    let bloat_output: BloatOutput =
        serde_json::from_str(&stdout).context("Failed to parse cargo bloat JSON output")?;

    Ok(bloat_output)
}

fn run_cargo_llvm_lines_for_crates(
    crate_names: &[String],
    workspace_path: &str,
) -> Result<LlvmLinesSummary> {
    let mut crate_results = Vec::new();
    let mut all_functions = Vec::new();

    for crate_name in crate_names {
        // Convert crate name to manifest path with workspace prefix
        let manifest_path = format!(
            "{}/{}/Cargo.toml",
            workspace_path,
            crate_name.replace('_', "-")
        );

        match run_cargo_llvm_lines_single(&manifest_path) {
            Ok((line_count, copy_count, functions)) => {
                crate_results.push(CrateLlvmLines {
                    name: crate_name.clone(),
                    lines: line_count,
                    copies: copy_count,
                });
                all_functions.extend(functions);
            }
            Err(e) => {
                println!(
                    "⚠️  Failed to run cargo-llvm-lines for {}: {}",
                    crate_name, e
                );
                // Continue with other crates instead of failing completely
                crate_results.push(CrateLlvmLines {
                    name: crate_name.clone(),
                    lines: 0,
                    copies: 0,
                });
            }
        }
    }

    // Sort all functions by line count (descending)
    all_functions.sort_by(|a, b| b.lines.cmp(&a.lines));

    Ok(LlvmLinesSummary {
        crate_results,
        top_functions: all_functions,
    })
}

fn run_cargo_llvm_lines_single(manifest_path: &str) -> Result<(u32, u32, Vec<LlvmFunction>)> {
    let start = Instant::now();

    // First try with binary target
    let mut output = Command::new("cargo")
        .args(["llvm-lines", "--release", "--manifest-path", manifest_path])
        .output()
        .context("Failed to execute cargo llvm-lines")?;

    // If binary target fails, try with --lib for library crates
    if !output.status.success() {
        output = Command::new("cargo")
            .args([
                "llvm-lines",
                "--release",
                "--lib",
                "--manifest-path",
                manifest_path,
            ])
            .output()
            .context("Failed to execute cargo llvm-lines with --lib")?;
    }

    let duration = start.elapsed();

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        anyhow::bail!("cargo llvm-lines failed: {}", stderr);
    }

    let stdout = String::from_utf8_lossy(&output.stdout);
    println!("⏱️  cargo llvm-lines took: {:?}", duration);

    parse_llvm_lines_output(&stdout)
}

fn format_bytes(bytes: u64) -> String {
    const UNITS: &[&str] = &["B", "KiB", "MiB", "GiB"];
    const THRESHOLD: f64 = 1024.0;

    let mut size = bytes as f64;
    let mut unit_index = 0;

    while size >= THRESHOLD && unit_index < UNITS.len() - 1 {
        size /= THRESHOLD;
        unit_index += 1;
    }

    if unit_index == 0 {
        format!("{} B", bytes)
    } else {
        format!("{:.2} {}", size, UNITS[unit_index])
    }
}

fn format_number(num: u32) -> String {
    // Add thousands separators for readability
    let num_str = num.to_string();
    let mut result = String::new();

    for (i, c) in num_str.chars().rev().enumerate() {
        if i > 0 && i % 3 == 0 {
            result.push(',');
        }
        result.push(c);
    }

    result.chars().rev().collect()
}

/// Output from the LLVM IR build process
#[derive(Debug)]
struct LlvmBuildOutput {
    /// Directory where LLVM IR files and build artifacts are stored
    /// Example: "../target-llvm-12345"
    /// Obtained from: build_with_llvm_ir function, dynamically generated
    /// Used for: Locating .ll files for analysis and cleanup
    target_dir: String,

    /// Summary of build timing information from this build
    /// Obtained from: Parsing cargo build --timings output
    /// Used for: Populating build time metrics in BuildResult
    timing_summary: BuildTimingSummary,
}

/// Options for building with LLVM IR emission.
/// Used to configure the build process for LLVM analysis.
#[derive(Debug, Clone)]
struct BuildWithLllvmIrOpts {
    /// Path to the Cargo.toml manifest file for the crate to build.
    /// Example: "../ks-facet/Cargo.toml"
    /// Obtained from: MeasurementTarget configuration or manual specification.
    /// Used for: Specifying which crate to build.
    manifest_path: String,

    /// Target directory for the build artifacts.
    /// Example: "../target-llvm-12345"
    /// Obtained from: Dynamically generated to avoid conflicts.
    /// Used for: Isolating build outputs and locating .ll files.
    target_dir: String,

    /// Environment variables to set for the cargo-bloat command.
    /// Example: `HashMap::from([("RUSTFLAGS".to_string(), "-Zunstable-options".to_string())])`
    /// Obtained from: Specific measurement requirements.
    /// Used for: Customizing the build environment for cargo-bloat.
    env_vars: std::collections::HashMap<String, String>,
}

/// Build the project with LLVM IR emission and timing information
fn build_with_llvm_ir(opts: &BuildWithLllvmIrOpts) -> Result<LlvmBuildOutput> {
    let pb = indicatif::ProgressBar::new_spinner();
    let style = indicatif::ProgressStyle::default_spinner()
        .tick_chars("⠁⠂⠄⡀⢀⠠⠐⠈ ")
        .template("{spinner:.green} {msg}")
        .expect("BUG: Invalid indicatif template"); // template() on default_spinner should not fail with valid str
    pb.set_style(style);
    pb.set_message("Building with LLVM IR emission and timing...");
    pb.enable_steady_tick(std::time::Duration::from_millis(100));

    let start = Instant::now();

    // Build with LLVM IR emission and timing information
    let mut command = Command::new("cargo");
    command.args([
        "build",
        "--release",
        "--manifest-path",
        &opts.manifest_path,
        "--target-dir",
        &opts.target_dir,
        "--timings=json",
        "-Zunstable-options", // This -Z flag is for cargo's --timings, not rustc
    ]);

    // Apply environment variables from opts
    for (key, value) in &opts.env_vars {
        command.env(key, value);
    }

    let output = command
        .output()
        .context("Failed to execute cargo build with LLVM IR")?;

    let total_duration = start.elapsed().as_secs_f64();
    pb.disable_steady_tick(); // Stop animation before printing final message

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        pb.finish_with_message("❌ Cargo build (LLVM IR) failed");
        anyhow::bail!(
            "cargo build failed: {}\nSTDOUT:\n{}\nSTDERR:\n{}",
            output.status,
            String::from_utf8_lossy(&output.stdout),
            stderr
        );
    }

    pb.finish_with_message(format!(
        "✅ Cargo build (LLVM IR) completed in {:.2}s",
        total_duration
    ));

    let stdout = String::from_utf8_lossy(&output.stdout);
    let mut crate_timings = Vec::new();

    // Parse each JSON line for timing info
    for line in stdout.lines() {
        if line.trim().starts_with('{') && line.contains("timing-info") {
            if let Ok(timing_entry) = serde_json::from_str::<CargoTimingEntry>(line.trim()) {
                if timing_entry.reason == "timing-info" {
                    // Use the target name which is the actual crate name
                    let crate_name = timing_entry.target.name.replace('-', "_");
                    crate_timings.push(CrateTiming {
                        name: crate_name,
                        duration: timing_entry.duration,
                    });
                }
            }
        }
    }

    // Sort by duration (descending)
    crate_timings.sort_by(|a, b| {
        b.duration
            .partial_cmp(&a.duration)
            .unwrap_or(std::cmp::Ordering::Equal)
    });

    Ok(LlvmBuildOutput {
        target_dir: opts.target_dir.clone(),
        timing_summary: BuildTimingSummary {
            total_duration,
            crate_timings,
        },
    })
}

fn measure_build_time(manifest_path: &str) -> Result<BuildTimingSummary> {
    let start = Instant::now();

    // Use a specific target directory to avoid conflicts
    let target_dir = format!("../target-timing-{}", std::process::id());

    // First, clean to ensure we're measuring a fresh build
    let clean_output = Command::new("cargo")
        .args([
            "clean",
            "--manifest-path",
            manifest_path,
            "--target-dir",
            &target_dir,
        ])
        .env("RUSTC_BOOTSTRAP", "1")
        .output()
        .context("Failed to run cargo clean")?;

    if !clean_output.status.success() {
        let stderr = String::from_utf8_lossy(&clean_output.stderr);
        anyhow::bail!("cargo clean failed: {}", stderr);
    }

    // Now build with timing information
    let output = Command::new("cargo")
        .args([
            "build",
            "--release",
            "--manifest-path",
            manifest_path,
            "--target-dir",
            &target_dir,
            "--timings=json",
            "-Zunstable-options",
        ])
        .env("RUSTC_BOOTSTRAP", "1")
        .output()
        .context("Failed to execute cargo build with timings")?;

    let total_duration = start.elapsed().as_secs_f64();
    println!("⏱️  cargo build (with timing) took: {:.2}s", total_duration);

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        anyhow::bail!("cargo build failed: {}", stderr);
    }

    // Clean up the temporary target directory
    let _ = std::fs::remove_dir_all(&target_dir);

    let stdout = String::from_utf8_lossy(&output.stdout);
    let mut crate_timings = Vec::new();

    // Parse each JSON line for timing info
    for line in stdout.lines() {
        if line.trim().starts_with('{') && line.contains("timing-info") {
            if let Ok(timing_entry) = serde_json::from_str::<CargoTimingEntry>(line.trim()) {
                if timing_entry.reason == "timing-info" {
                    // Use the target name which is the actual crate name
                    let crate_name = timing_entry.target.name.replace('-', "_");
                    crate_timings.push(CrateTiming {
                        name: crate_name,
                        duration: timing_entry.duration,
                    });
                }
            }
        }
    }

    // Sort by duration (descending)
    crate_timings.sort_by(|a, b| {
        b.duration
            .partial_cmp(&a.duration)
            .unwrap_or(std::cmp::Ordering::Equal)
    });

    Ok(BuildTimingSummary {
        total_duration,
        crate_timings,
    })
}

/// Analyze LLVM IR files to get lines of code information
fn analyze_llvm_files(target_dir: &str, crate_names: &[String]) -> Result<LlvmLinesSummary> {
    use std::fs;
    use std::path::Path;

    let deps_dir = Path::new(target_dir).join("release").join("deps");

    if !deps_dir.exists() {
        anyhow::bail!("Target deps directory does not exist: {:?}", deps_dir);
    }

    let mut crate_results = Vec::new();
    let mut all_functions = Vec::new();
    // Collect all relevant .ll files from the deps_dir first
    let mut all_ll_files = Vec::new();
    for entry in fs::read_dir(&deps_dir)? {
        let entry = entry?;
        let path = entry.path();
        let file_name = entry.file_name();
        let file_name_str = file_name.to_string_lossy();

        if path.is_file() && file_name_str.ends_with(".ll") {
            // Skip build script artifacts
            if !file_name_str.contains("build_script") {
                all_ll_files.push(path);
            }
        }
    }

    // For each crate, find its .ll file from the collected list and analyze it
    for crate_name in crate_names {
        // Convert crate name to file prefix (hyphens to underscores)
        let file_prefix = crate_name.replace('-', "_");

        // Find the .ll file for this crate from the pre-filtered list
        // Match files like "crate_name-hash.ll" or "crate_name.ll" (if no hash)
        let ll_file_path = all_ll_files.iter().find(|path| {
            path.file_name()
                .is_some_and(|name| name.to_string_lossy().starts_with(&file_prefix))
        });

        if ll_file_path.is_none() {
            // It's possible some crates don't produce .ll files (e.g. proc-macros, or if they are empty)
            // Or if the build command didn't actually build them with --emit=llvm-ir
            // For now, we will panic as this was the previous behavior, but this could be a warning.
            // Consider if all `crate_names` are *expected* to have an .ll file.
            // If `cargo llvm-lines` is run directly, it might handle missing .ll for some crates in a workspace build.
            // Here, since we are specifically looking for .ll files from a `cargo build --emit=llvm-ir`,
            // it's more likely an issue if one is missing for a crate we expect to analyze.
            panic!(
                "⚠️  No .ll files found for crate: {} in {:?}. Searched prefix: {}",
                crate_name, deps_dir, file_prefix
            );
        }

        let ll_file = ll_file_path.unwrap();
        println!(
            "📊 Analyzing LLVM IR for {}: {:?}",
            crate_name,
            ll_file.file_name().unwrap_or_default()
        );

        // Run cargo llvm-lines with --files option
        let output = Command::new("cargo")
            .args(["llvm-lines", "--files", &ll_file.to_string_lossy()])
            .output()
            .context(format!(
                "Failed to execute cargo llvm-lines for file: {}",
                ll_file.display()
            ))?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            println!(
                "⚠️  cargo llvm-lines failed for crate {}: {}",
                crate_name, stderr
            );
            crate_results.push(CrateLlvmLines {
                name: crate_name.clone(),
                lines: 0,
                copies: 0,
            });
            continue;
        }

        let stdout = String::from_utf8_lossy(&output.stdout);
        match parse_llvm_lines_output(&stdout) {
            Ok((line_count, copy_count, functions)) => {
                crate_results.push(CrateLlvmLines {
                    name: crate_name.clone(),
                    lines: line_count,
                    copies: copy_count,
                });
                all_functions.extend(functions);
            }
            Err(e) => {
                println!(
                    "⚠️  Failed to parse llvm-lines output for crate {}: {}",
                    crate_name, e
                );
                crate_results.push(CrateLlvmLines {
                    name: crate_name.clone(),
                    lines: 0,
                    copies: 0,
                });
            }
        }
    }

    // Sort all functions by line count (descending)
    all_functions.sort_by(|a, b| b.lines.cmp(&a.lines));

    Ok(LlvmLinesSummary {
        crate_results,
        top_functions: all_functions,
    })
}

fn parse_llvm_lines_output(output: &str) -> Result<(u32, u32, Vec<LlvmFunction>)> {
    // Parse cargo-llvm-lines output to extract total line count, copy count, and individual functions
    // Look for line like "1876                94                (TOTAL)"
    // And function lines like "   99 (5.3%,  5.3%)   1 (1.1%,  1.1%)  ks_facet::main"

    let mut total_lines = 0;
    let mut total_copies = 0;
    let mut functions = Vec::new();

    // Parse function lines: "   99 (5.3%,  5.3%)   1 (1.1%,  1.1%)  ks_facet::main"
    // Use regex to precisely extract: lines, copies, and function name
    let re = Regex::new(r"^\s*(\d+)\s+\([^)]+\)\s+(\d+)\s+\([^)]+\)\s+(.+)$").unwrap();

    for line in output.lines() {
        if line.contains("(TOTAL)") {
            let parts: Vec<&str> = line.split_whitespace().collect();
            if parts.len() >= 3 {
                total_lines = parts[0]
                    .parse()
                    .context("Failed to parse LLVM line count")?;
                total_copies = parts[1]
                    .parse()
                    .context("Failed to parse LLVM copy count")?;
            }
        } else if line
            .trim_start()
            .chars()
            .next()
            .is_some_and(|c| c.is_ascii_digit())
            && line.contains('%')
        {
            if let Some(captures) = re.captures(line) {
                if let (Ok(lines), Ok(copies)) =
                    (captures[1].parse::<u32>(), captures[2].parse::<u32>())
                {
                    let function_name = captures[3].trim().to_string();
                    functions.push(LlvmFunction {
                        name: function_name,
                        lines,
                        copies,
                    });
                }
            }
        }
    }

    if total_lines == 0 && total_copies == 0 {
        // Handle case where there are no functions (like ks-types)
        return Ok((0, 0, functions));
    }

    if total_lines == 0 {
        anyhow::bail!("Could not find (TOTAL) line in cargo-llvm-lines output");
    }

    Ok((total_lines, total_copies, functions))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_format_bytes() {
        assert_eq!(format_bytes(1024), "1.00 KiB");
        assert_eq!(format_bytes(1536), "1.50 KiB");
        assert_eq!(format_bytes(2097152), "2.00 MiB");
        assert_eq!(format_bytes(500), "500 B");
    }
}
