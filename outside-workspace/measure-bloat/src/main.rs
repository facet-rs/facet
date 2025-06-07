use anyhow::{Context, Result};
use clap::{Parser, Subcommand};
use regex::Regex;
use serde::{Deserialize, Serialize};
use std::cmp::Ordering;
use std::collections::HashMap;
use std::fs;
use std::path::PathBuf;
use std::process::Command;
use std::time::Instant;
use toml_edit::{Document, Item, Table, Value};

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
        /// Output directory for results
        #[arg(short, long, default_value = "bloat-results")]
        output: PathBuf,
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
            output,
            skip_serde,
            skip_main,
        } => run_comparison(output, skip_serde, skip_main),
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

fn run_comparison(output: PathBuf, skip_serde: bool, skip_main: bool) -> Result<()> {
    println!("🚀 Starting full comparison...");
    println!("Output directory: {}", output.display());
    println!("Skip serde: {}", skip_serde);
    println!("Skip main: {}", skip_main);

    // Setup output directory
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
                "ks-serde-pretty".to_string(),
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
            // For serde variant, we'd need different Cargo.toml files
            // For now, just note this needs implementation
            println!("🚧 Serde variant setup not yet implemented");
        }
        _ => {
            anyhow::bail!("Unknown variant: {}", variant);
        }
    }
    Ok(())
}

fn modify_cargo_tomls_for_main_branch() -> Result<()> {
    // Store original Cargo.toml files and replace facet dependencies with git refs
    let crates_to_modify = vec![
        "../ks-facet-json-read",
        "../ks-facet-json-write",
        "../ks-facet-pretty",
        "../ks-mock",
    ];

    for crate_path in crates_to_modify {
        let cargo_toml_path = PathBuf::from(crate_path).join("Cargo.toml");
        let backup_path = PathBuf::from(crate_path).join("Cargo.toml.backup");

        // Read original content
        let original_content = fs::read_to_string(&cargo_toml_path)
            .context(format!("Failed to read {}", cargo_toml_path.display()))?;

        // Backup original
        fs::write(&backup_path, &original_content).context("Failed to create backup")?;

        // Replace local facet dependencies with git dependencies
        let modified_content = replace_facet_deps_with_git(&original_content)?;

        // Write modified content
        fs::write(&cargo_toml_path, modified_content).context(format!(
            "Failed to write modified {}",
            cargo_toml_path.display()
        ))?;
    }

    Ok(())
}

fn replace_facet_deps_with_git(content: &str) -> Result<String> {
    let mut doc = content
        .parse::<Document>()
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
    // Restore original Cargo.toml files from backups
    let crates_to_restore = vec![
        "../ks-facet-json-read",
        "../ks-facet-json-write",
        "../ks-facet-pretty",
        "../ks-mock",
    ];

    for crate_path in crates_to_restore {
        let cargo_toml_path = PathBuf::from(crate_path).join("Cargo.toml");
        let backup_path = PathBuf::from(crate_path).join("Cargo.toml.backup");

        if backup_path.exists() {
            // Restore original content
            let original_content =
                fs::read_to_string(&backup_path).context("Failed to read backup file")?;
            fs::write(&cargo_toml_path, original_content)
                .context("Failed to restore original Cargo.toml")?;

            // Remove backup file
            fs::remove_file(&backup_path).context("Failed to remove backup file")?;
        }
    }

    println!("🧹 Cleaned up Cargo.toml modifications");
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

    // Use the binary crate as the measurement target
    let manifest_path = format!("../{}/Cargo.toml", target.binary_crate);

    // Run measurements
    let start = Instant::now();

    // Measure binary size
    let bloat_functions = run_cargo_bloat(&manifest_path, false)?;
    let bloat_crates = run_cargo_bloat(&manifest_path, true)?;

    // Measure LLVM lines
    let llvm_lines = run_cargo_llvm_lines_for_crates(crates_to_use)?;

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
        top_functions: bloat_functions.functions.into_iter().take(10).collect(),
        top_crates: bloat_crates.crates.into_iter().take(5).collect(),
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

        // Summary table
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

        report.push_str("\n");

        // Detailed breakdown for each variant
        for result in &target_results {
            report.push_str(&format!("### {} - {}\n\n", target_name, result.variant));

            report.push_str("**Top Functions by Size:**\n");
            for (i, func) in result.top_functions.iter().take(5).enumerate() {
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

fn run_cargo_llvm_lines_for_crates(crate_names: &[String]) -> Result<LlvmLinesSummary> {
    let mut crate_results = Vec::new();
    let mut all_functions = Vec::new();

    for crate_name in crate_names {
        // Convert crate name to manifest path
        let manifest_path = format!("../{}/Cargo.toml", crate_name.replace('_', "-"));

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
