use anyhow::{Context, Result};
use clap::{Parser, Subcommand};
use regex::Regex;
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::PathBuf;
use std::process::{self, Command};
use std::time::Instant;
use toml_edit::{DocumentMut, Item, Value};

#[derive(Parser)]
#[command(name = "measure-bloat")]
#[command(about = "A utility to measure and compare binary sizes and build times")]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Run the full comparison between serde, facet-pr, and facet-main
    Compare {
        /// Skip serde comparison
        #[arg(long)]
        skip_serde: bool,
        /// Skip facet-main comparison
        #[arg(long)]
        skip_main: bool,
    },
    /// Show the implementation plan
    Plan,
    /// Test individual components
    Test {
        /// Component to test
        component: String,
        /// Variant to test (serde, facet-pr, facet-main)
        variant: String,
    },
}

#[derive(Debug, Clone)]
struct MeasurementTarget {
    name: String,
    facet_crates: Vec<String>,
    serde_crates: Vec<String>,
    binary_crate: String,
}

#[derive(Debug, Serialize, Deserialize)]
struct BloatFunction {
    #[serde(rename = "crate")]
    crate_name: String,
    name: String,
    size: u64,
}

#[derive(Debug, Serialize, Deserialize)]
struct BloatCrate {
    name: String,
    size: u64,
}

#[derive(Debug, Serialize, Deserialize)]
struct BloatOutput {
    #[serde(rename = "file-size")]
    file_size: u64,
    #[serde(rename = "text-section-size")]
    text_section_size: u64,
    #[serde(default)]
    functions: Vec<BloatFunction>,
    #[serde(default)]
    crates: Vec<BloatCrate>,
}

#[derive(Debug, Serialize, Deserialize)]
struct CargoTimingTarget {
    name: String,
}

#[derive(Debug, Serialize, Deserialize)]
struct CargoTimingEntry {
    reason: String,
    package_id: String,
    target: CargoTimingTarget,
    duration: f64,
    #[serde(default)]
    rmeta_time: Option<f64>,
}

#[derive(Debug)]
struct BuildTimingSummary {
    total_duration: f64,
    crate_timings: Vec<(String, f64)>,
}

#[derive(Debug)]
struct LlvmFunction {
    name: String,
    lines: u32,
    copies: u32,
}

#[derive(Debug)]
struct LlvmLinesSummary {
    crate_results: Vec<(String, u32, u32)>, // (name, lines, copies)
    top_functions: Vec<LlvmFunction>,
}

#[derive(Debug)]
struct BuildResult {
    target: String,
    variant: String,
    file_size: u64,
    text_section_size: u64,
    build_time_ms: u64,
    top_functions: Vec<BloatFunction>,
    top_crates: Vec<BloatCrate>,
    llvm_lines: LlvmLinesSummary,
}

fn main() -> Result<()> {
    let cli = Cli::parse();

    match cli.command {
        Commands::Plan => show_plan(),
        Commands::Compare {
            skip_serde,
            skip_main,
        } => run_comparison(skip_serde, skip_main),
        Commands::Test { component, variant } => {
            // Test TOML transformation first
            if component == "debug-toml" {
                test_toml_transformation()?;
                return Ok(());
            }
            test_component(component, variant)
        }
    }
}

fn show_plan() -> Result<()> {
    println!(
        r#"
# MEASURE-BLOAT IMPLEMENTATION PLAN

## Overview
This tool will compare binary sizes and build times across three scenarios:
1. **serde-latest**: Using the latest serde ecosystem
2. **facet-pr**: Using facet from current PR/HEAD
3. **facet-main**: Using facet from main branch (with PR's ks-* crates)

## Measurement Targets

### 1. JSON Read/Write Benchmark
- **Facet crates**: ks-facet, ks-mock, ks-facet-json-read, ks-facet-json-write
- **Serde crates**: ks-serde, ks-mock, ks-serde-json-read, ks-serde-json-write
- **Binary**: Composite benchmark that does JSON read + write operations

### 2. Pretty Printing Benchmark
- **Facet crates**: ks-facet, ks-mock, ks-facet-pretty
- **Serde crates**: ks-serde, ks-mock, ks-debug-print
- **Binary**: Pretty print formatting benchmark

### 3. Core Library Size
- **Facet crates**: ks-facet, ks-mock
- **Serde crates**: ks-serde, ks-mock
- **Binary**: Minimal binary using just core functionality

## Implementation Phases

### Phase 1: Infrastructure (CURRENT)
- [x] Create project structure
- [x] Define measurement targets
- [ ] Implement cargo command execution
- [ ] Implement size parsing (cargo-bloat output)
- [ ] Implement LLVM lines parsing (cargo-llvm-lines output)

### Phase 2: Basic Measurements
- [ ] Implement single-target measurement
- [ ] Test with current ks-facet crates
- [ ] Validate cargo-bloat and cargo-llvm-lines integration

### Phase 3: Multi-Variant Support
- [ ] Implement git branch switching for facet-main comparison
- [ ] Implement [patch.crates-io] for mixing PR ks-* with main facet
- [ ] Handle Cargo.toml manipulation safely

### Phase 4: Serde Integration
- [ ] Implement serde-based variants of ks-* crates
- [ ] Create equivalent benchmarks using serde ecosystem
- [ ] Ensure fair comparison methodology

### Phase 5: Reporting
- [ ] Generate markdown reports
- [ ] Create comparison tables
- [ ] Add diff generation for detailed analysis
- [ ] GitHub Actions integration

## Technical Challenges

### 1. Dependency Management
**Problem**: Need to test facet-main with PR's ks-* crates
**Solution**: Use `[patch.crates-io]` or `[patch."https://github.com/..."]` in Cargo.toml

### 2. Serde Equivalents
**Problem**: ks-serde-* crates are currently stubbed
**Solution**: Implement minimal but equivalent functionality for fair comparison

### 3. Build Isolation
**Problem**: Cargo caches can interfere between builds
**Solution**: Use separate target directories or cargo clean between variants

### 4. Git State Management
**Problem**: Need to switch between branches without losing working changes
**Solution**: Use git stash/unstash or separate worktrees

## Usage Examples

```bash
# Full comparison (will take time!)
measure-bloat compare

# Skip serde comparison during development
measure-bloat compare --skip-serde

# Test individual component
measure-bloat test json-benchmark facet-pr

# Show this plan
measure-bloat plan
```

## Output Format
- Markdown report with size comparisons
- CSV data for further analysis
- Detailed logs for debugging
- GitHub Actions artifact compatibility
"#
    );
    Ok(())
}

fn run_comparison(skip_serde: bool, skip_main: bool) -> Result<()> {
    println!("🚀 Starting full comparison...");
    println!("Skip serde: {}", skip_serde);
    println!("Skip main: {}", skip_main);

    // Setup output directory
    let output = PathBuf::from("bloat-results");
    println!("Output directory: {}", output.display());
    fs::create_dir_all(&output).context("Failed to create output directory")?;

    // Define measurement targets
    let targets = get_measurement_targets();

    let mut all_results = Vec::new();

    // Measure ks-facet with HEAD and main variants
    let facet_target = &targets[0]; // ks-facet target
    if !skip_main {
        // Measure with both HEAD and main
        for &variant in &["facet-pr", "facet-main"] {
            println!("\n🔄 Measuring {} with {}", facet_target.name, variant);
            setup_cargo_patches(variant)?;
            match measure_target_complete(facet_target, variant) {
                Ok(result) => {
                    println!("✅ Measurement complete");
                    all_results.push(result);
                }
                Err(e) => {
                    println!("❌ Measurement failed: {}", e);
                }
            }
            cleanup_cargo_patches()?;
        }
    } else {
        // Measure only HEAD
        let variant = "facet-pr";
        println!("\n🔄 Measuring {} with {}", facet_target.name, variant);
        setup_cargo_patches(variant)?;
        match measure_target_complete(facet_target, variant) {
            Ok(result) => {
                println!("✅ Measurement complete");
                all_results.push(result);
            }
            Err(e) => {
                println!("❌ Measurement failed: {}", e);
            }
        }
        cleanup_cargo_patches()?;
    }

    // Measure ks-serde if not skipped
    if !skip_serde {
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
                println!("❌ Measurement failed: {}", e);
            }
        }
        cleanup_cargo_patches()?;
    }

    // Generate comparison report
    let report_path = output.join("comparison_report.md");
    generate_comparison_report(&all_results, &report_path)?;

    println!("\n🎉 Comparison complete!");
    println!("📄 Report generated: {}", report_path.display());

    Ok(())
}

fn test_component(component: String, variant: String) -> Result<()> {
    println!(
        "🧪 Testing component: {} with variant: {}",
        component, variant
    );

    let targets = get_measurement_targets();

    match component.as_str() {
        "ks-facet" => {
            if variant == "facet-pr" || variant == "facet-main" {
                setup_cargo_patches(&variant)?;
                let result = measure_target(&targets[0], &variant);
                cleanup_cargo_patches()?;
                result
            } else {
                println!(
                    "❌ Invalid variant '{}' for ks-facet. Use 'facet-pr' or 'facet-main'",
                    variant
                );
                Ok(())
            }
        }
        "ks-serde" => {
            if variant == "serde" {
                setup_cargo_patches(&variant)?;
                let result = measure_target(&targets[1], &variant);
                cleanup_cargo_patches()?;
                result
            } else {
                println!("❌ Invalid variant '{}' for ks-serde. Use 'serde'", variant);
                Ok(())
            }
        }
        "json-benchmark" => test_json_benchmark(&variant),
        "pretty-benchmark" => test_pretty_benchmark(&variant),
        "core-benchmark" => test_core_benchmark(&variant),
        _ => {
            println!("❌ Unknown component: {}", component);
            println!(
                "Available components: ks-facet, ks-serde, json-benchmark, pretty-benchmark, core-benchmark"
            );
            Ok(())
        }
    }
}

fn test_json_benchmark(variant: &str) -> Result<()> {
    let target = MeasurementTarget {
        name: "json-benchmark".to_string(),
        facet_crates: vec![
            "ks-facet".to_string(),
            "ks-mock".to_string(),
            "ks-types".to_string(),
            "ks-facet-json-read".to_string(),
            "ks-facet-json-write".to_string(),
        ],
        serde_crates: vec![
            "ks-serde".to_string(),
            "ks-mock".to_string(),
            "ks-types".to_string(),
            "ks-serde-json-read".to_string(),
            "ks-serde-json-write".to_string(),
        ],
        binary_crate: "ks-facet".to_string(), // For now, use ks-facet as the binary
    };

    measure_target(&target, variant)
}

fn test_pretty_benchmark(variant: &str) -> Result<()> {
    let target = MeasurementTarget {
        name: "pretty-benchmark".to_string(),
        facet_crates: vec![
            "ks-facet".to_string(),
            "ks-mock".to_string(),
            "ks-types".to_string(),
            "ks-facet-pretty".to_string(),
        ],
        serde_crates: vec![
            "ks-serde".to_string(),
            "ks-mock".to_string(),
            "ks-types".to_string(),
            "ks-debug".to_string(), // Note: it's ks-debug not ks-debug-print in the directory
        ],
        binary_crate: "ks-facet".to_string(),
    };

    measure_target(&target, variant)
}

fn test_core_benchmark(variant: &str) -> Result<()> {
    let target = MeasurementTarget {
        name: "core-benchmark".to_string(),
        facet_crates: vec![
            "ks-facet".to_string(),
            "ks-mock".to_string(),
            "ks-types".to_string(),
        ],
        serde_crates: vec![
            "ks-serde".to_string(),
            "ks-mock".to_string(),
            "ks-types".to_string(),
        ],
        binary_crate: "ks-facet".to_string(),
    };

    measure_target(&target, variant)
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

    println!("🔍 Original TOML content:\n{}", content);

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
    println!("🔍 Modified TOML content:\n{}", result);
    Ok(result)
}

fn test_toml_transformation() -> Result<()> {
    let test_toml = r#"[package]
name = "ks-facet-json-read"
version = "0.1.0"

[dependencies]
facet-json = { version = "0.24.13", path = "../../facet-json" }
ks-types = { version = "0.1.0", path = "../ks-types", features = ["facet"] }
"#;

    println!("🧪 Testing TOML transformation");
    println!("Original:\n{}", test_toml);

    let result = replace_facet_deps_with_git(test_toml)?;
    println!("Transformed:\n{}", result);

    Ok(())
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

    // Measure binary size
    let bloat_functions = run_cargo_bloat(&manifest_path, false)?;
    let bloat_crates = run_cargo_bloat(&manifest_path, true)?;

    // Measure LLVM lines - crate names stay the same, just in different workspace
    let workspace_path = match variant {
        "facet-main" => {
            let temp_dir = std::env::temp_dir().join(format!("measure-bloat-{}", process::id()));
            temp_dir
                .join("outside-workspace")
                .to_string_lossy()
                .to_string()
        }
        _ => "..".to_string(),
    };
    let llvm_lines = run_cargo_llvm_lines_for_crates(crates_to_use, &workspace_path)?;

    // Measure build time
    let build_timing = measure_build_time(&manifest_path)?;

    let measurement_duration = start.elapsed();
    println!(
        "⏱️  Total measurement time: {:.2}s",
        measurement_duration.as_secs_f64()
    );

    Ok(BuildResult {
        target: target.name.clone(),
        variant: variant.to_string(),
        file_size: bloat_functions.file_size,
        text_section_size: bloat_functions.text_section_size,
        build_time_ms: (build_timing.total_duration * 1000.0) as u64,
        top_functions: bloat_functions.functions.into_iter().take(50).collect(),
        top_crates: bloat_crates.crates.into_iter().take(20).collect(),
        llvm_lines,
    })
}

fn generate_comparison_report(results: &[BuildResult], report_path: &PathBuf) -> Result<()> {
    let mut report = String::new();

    report.push_str("# Facet vs Serde Comparison Report\n\n");
    report.push_str(&format!(
        "Generated on: {}\n\n",
        chrono::Utc::now().format("%Y-%m-%d %H:%M:%S UTC")
    ));

    // Group results by target
    let mut targets: std::collections::HashMap<String, Vec<&BuildResult>> =
        std::collections::HashMap::new();
    for result in results {
        targets
            .entry(result.target.clone())
            .or_default()
            .push(result);
    }

    for (target_name, target_results) in targets {
        report.push_str(&format!("## {}\n\n", target_name));

        // Find facet-pr and facet-main results for diff analysis
        let facet_pr = target_results.iter().find(|r| r.variant == "facet-pr");
        let facet_main = target_results.iter().find(|r| r.variant == "facet-main");

        // Sort results to show facet-main first (baseline), then facet-pr
        let mut sorted_results = target_results.clone();
        sorted_results.sort_by(|a, b| match (a.variant.as_str(), b.variant.as_str()) {
            ("facet-main", _) => std::cmp::Ordering::Less,
            (_, "facet-main") => std::cmp::Ordering::Greater,
            ("facet-pr", _) => std::cmp::Ordering::Less,
            (_, "facet-pr") => std::cmp::Ordering::Greater,
            _ => a.variant.cmp(&b.variant),
        });

        // Summary table with deltas
        if target_results.len() == 1 {
            // Single variant - no deltas
            report.push_str("| Variant | File Size | Text Size | Build Time | LLVM Lines |\n");
            report.push_str("|---------|-----------|-----------|------------|------------|\n");

            for result in &target_results {
                let total_llvm_lines: u32 = result
                    .llvm_lines
                    .crate_results
                    .iter()
                    .map(|(_, lines, _)| lines)
                    .sum();
                report.push_str(&format!(
                    "| {} | {} | {} | {:.2}s | {} |\n",
                    result.variant,
                    format_bytes(result.file_size),
                    format_bytes(result.text_section_size),
                    result.build_time_ms as f64 / 1000.0,
                    format_number(total_llvm_lines)
                ));
            }
        } else {
            // Multiple variants - show deltas
            report.push_str(
                "| Variant | File Size | Δ | Text Size | Δ | Build Time | Δ | LLVM Lines | Δ |\n",
            );
            report.push_str(
                "|---------|-----------|---|-----------|---|------------|---|------------|---|\n",
            );

            let baseline_result = &sorted_results[0];
            let baseline_llvm_total: u32 = baseline_result
                .llvm_lines
                .crate_results
                .iter()
                .map(|(_, lines, _)| lines)
                .sum();

            for (i, result) in sorted_results.iter().enumerate() {
                let total_llvm_lines: u32 = result
                    .llvm_lines
                    .crate_results
                    .iter()
                    .map(|(_, lines, _)| lines)
                    .sum();

                if i == 0 {
                    // First variant - baseline, no deltas
                    report.push_str(&format!(
                        "| {} | {} | - | {} | - | {:.2}s | - | {} | - |\n",
                        result.variant,
                        format_bytes(result.file_size),
                        format_bytes(result.text_section_size),
                        result.build_time_ms as f64 / 1000.0,
                        format_number(total_llvm_lines)
                    ));
                } else {
                    // Calculate deltas from baseline
                    let file_size_delta =
                        result.file_size as i64 - baseline_result.file_size as i64;
                    let text_size_delta =
                        result.text_section_size as i64 - baseline_result.text_section_size as i64;
                    let build_time_delta =
                        result.build_time_ms as i64 - baseline_result.build_time_ms as i64;
                    let llvm_lines_delta = total_llvm_lines as i64 - baseline_llvm_total as i64;

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
                }
            }
        }

        report.push_str("\n");

        // Add diff analysis if we have both facet-pr and facet-main
        if let (Some(pr_result), Some(main_result)) = (facet_pr, facet_main) {
            generate_facet_diff_analysis(&mut report, pr_result, main_result);
        }

        // Detailed breakdown for each variant
        for result in &sorted_results {
            report.push_str(&format!("### {} - {}\n\n", target_name, result.variant));

            report.push_str("**Top Functions by Size:**\n");
            for (i, func) in result.top_functions.iter().take(10).enumerate() {
                report.push_str(&format!(
                    "{}. `{}::{}` - {}\n",
                    i + 1,
                    func.crate_name,
                    func.name,
                    format_bytes(func.size)
                ));
            }

            report.push_str("\n**LLVM Lines by Crate:**\n");
            for (crate_name, lines, copies) in &result.llvm_lines.crate_results {
                report.push_str(&format!(
                    "- `{}`: {} lines ({} copies)\n",
                    crate_name,
                    format_number(*lines),
                    format_number(*copies)
                ));
            }

            report.push_str("\n");
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
        .map(|(_, lines, _)| lines)
        .sum();
    let main_llvm_total: u32 = main_result
        .llvm_lines
        .crate_results
        .iter()
        .map(|(_, lines, _)| lines)
        .sum();
    let llvm_lines_delta = pr_llvm_total as i64 - main_llvm_total as i64;

    // Function-level diff analysis
    generate_function_diff_analysis(report, pr_result, main_result);

    // LLVM crate-level diff analysis
    generate_llvm_crate_diff_analysis(report, pr_result, main_result);

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
        main_funcs.insert(key, func.size);
    }

    let mut pr_funcs: std::collections::HashMap<String, u64> = std::collections::HashMap::new();
    for func in &pr_result.top_functions {
        let key = format!("{}::{}", func.crate_name, func.name);
        pr_funcs.insert(key, func.size);
    }

    // Collect all function changes
    let mut function_changes = Vec::new();

    // Check functions in PR
    for func in &pr_result.top_functions {
        let key = format!("{}::{}", func.crate_name, func.name);
        if let Some(&main_size) = main_funcs.get(&key) {
            let delta = func.size as i64 - main_size as i64;
            if delta != 0 {
                function_changes.push((key, main_size, func.size, delta));
            }
        } else {
            // New function in PR
            function_changes.push((key, 0, func.size, func.size as i64));
        }
    }

    // Check for functions that disappeared in PR
    for func in &main_result.top_functions {
        let key = format!("{}::{}", func.crate_name, func.name);
        if !pr_funcs.contains_key(&key) {
            function_changes.push((key, func.size, 0, -(func.size as i64)));
        }
    }

    // Sort by absolute delta size
    function_changes.sort_by_key(|(_, _, _, delta)| std::cmp::Reverse(delta.abs()));

    if function_changes.is_empty() {
        report.push_str("*No significant function size changes detected.*\n\n");
    } else {
        report.push_str("| Function | Main | PR | Change |\n");
        report.push_str("|----------|------|----|---------|\n");

        for (func_name, main_size, pr_size, delta) in function_changes.iter().take(30) {
            let main_str = if *main_size == 0 {
                "N/A".to_string()
            } else {
                format_bytes(*main_size)
            };
            let pr_str = if *pr_size == 0 {
                "N/A".to_string()
            } else {
                format_bytes(*pr_size)
            };

            let emoji = if *delta > 0 {
                "📈"
            } else if *delta < 0 {
                "📉"
            } else {
                "➖"
            };

            report.push_str(&format!(
                "| `{}` | {} | {} | {}{} |\n",
                func_name,
                main_str,
                pr_str,
                emoji,
                format_signed_bytes(*delta)
            ));
        }
        report.push_str("\n");
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
    for (crate_name, lines, copies) in &main_result.llvm_lines.crate_results {
        main_crates.insert(crate_name.clone(), (*lines, *copies));
    }

    let mut pr_crates: std::collections::HashMap<String, (u32, u32)> =
        std::collections::HashMap::new();
    for (crate_name, lines, copies) in &pr_result.llvm_lines.crate_results {
        pr_crates.insert(crate_name.clone(), (*lines, *copies));
    }

    let mut all_crate_data = Vec::new();

    // Check all crates from both results
    let mut all_crates = std::collections::HashSet::new();
    for (crate_name, _, _) in &main_result.llvm_lines.crate_results {
        all_crates.insert(crate_name.clone());
    }
    for (crate_name, _, _) in &pr_result.llvm_lines.crate_results {
        all_crates.insert(crate_name.clone());
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
        report.push_str("\n");
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

    report.push_str("\n");
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
                .map(|(_, lines, _)| lines)
                .sum();
            println!("   Total LLVM lines: {}", format_number(total_llvm_lines));
        }
        Err(e) => {
            println!("❌ Measurement failed: {}", e);
        }
    }

    Ok(())
}

fn run_cargo_bloat(manifest_path: &str, crates_mode: bool) -> Result<BloatOutput> {
    let start = Instant::now();

    let mut args = vec![
        "bloat",
        "--release",
        "--message-format",
        "json",
        "--manifest-path",
        manifest_path,
        "-n",
        "25",
    ];

    if crates_mode {
        args.push("--crates");
    }

    let output = Command::new("cargo")
        .args(&args)
        .output()
        .context("Failed to execute cargo bloat")?;

    let duration = start.elapsed();

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        anyhow::bail!("cargo bloat failed: {}", stderr);
    }

    let stdout = String::from_utf8_lossy(&output.stdout);
    println!("⏱️  cargo bloat took: {:?}", duration);

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
                crate_results.push((crate_name.clone(), line_count, copy_count));
                all_functions.extend(functions);
            }
            Err(e) => {
                println!(
                    "⚠️  Failed to run cargo-llvm-lines for {}: {}",
                    crate_name, e
                );
                // Continue with other crates instead of failing completely
                crate_results.push((crate_name.clone(), 0, 0));
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
        .args(&["llvm-lines", "--release", "--manifest-path", manifest_path])
        .output()
        .context("Failed to execute cargo llvm-lines")?;

    // If binary target fails, try with --lib for library crates
    if !output.status.success() {
        output = Command::new("cargo")
            .args(&[
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

fn measure_build_time(manifest_path: &str) -> Result<BuildTimingSummary> {
    let start = Instant::now();

    // Use a specific target directory to avoid conflicts
    let target_dir = format!("../target-timing-{}", std::process::id());

    // First, clean to ensure we're measuring a fresh build
    let clean_output = Command::new("cargo")
        .args(&[
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
        .args(&[
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
                    crate_timings.push((crate_name, timing_entry.duration));
                }
            }
        }
    }

    // Sort by duration (descending)
    crate_timings.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));

    Ok(BuildTimingSummary {
        total_duration,
        crate_timings,
    })
}

fn parse_llvm_lines_output(output: &str) -> Result<(u32, u32, Vec<LlvmFunction>)> {
    // Parse cargo-llvm-lines output to extract total line count, copy count, and individual functions
    // Look for line like "1876                94                (TOTAL)"
    // And function lines like "   99 (5.3%,  5.3%)   1 (1.1%,  1.1%)  ks_facet::main"

    let mut total_lines = 0;
    let mut total_copies = 0;
    let mut functions = Vec::new();

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
            .map_or(false, |c| c.is_ascii_digit())
            && line.contains('%')
        {
            // Parse function lines: "   99 (5.3%,  5.3%)   1 (1.1%,  1.1%)  ks_facet::main"
            // Use regex to precisely extract: lines, copies, and function name
            let re = Regex::new(r"^\s*(\d+)\s+\([^)]+\)\s+(\d+)\s+\([^)]+\)\s+(.+)$").unwrap();

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
