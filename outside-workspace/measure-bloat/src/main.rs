use anyhow::{Context, Result};
use clap::{Parser, Subcommand};
use serde::{Deserialize, Serialize};
use std::cmp::Ordering;
use std::path::PathBuf;
use std::process::Command;
use std::time::Instant;

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
struct LlvmLinesSummary {
    crate_results: Vec<(String, u32)>,
}

#[derive(Debug)]
struct BuildResult {
    target: String,
    variant: String,
    file_size: u64,
    text_section_size: u64,
    build_time_ms: u128,
    top_functions: Vec<BloatFunction>,
    top_crates: Vec<BloatCrate>,
    llvm_lines: Option<u32>,
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
        Commands::Test { component, variant } => test_component(component, variant),
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
    println!("🚧 Full comparison not yet implemented!");
    println!("This will measure all targets across all variants");
    println!("Output directory: {}", output.display());
    println!("Skip serde: {}", skip_serde);
    println!("Skip main: {}", skip_main);

    // TODO: Implement the full comparison logic
    // 1. Setup output directory
    // 2. For each target, measure each variant
    // 3. Generate comparison report

    Ok(())
}

fn test_component(component: String, variant: String) -> Result<()> {
    println!(
        "🧪 Testing component: {} with variant: {}",
        component, variant
    );

    match component.as_str() {
        "json-benchmark" => test_json_benchmark(&variant),
        "pretty-benchmark" => test_pretty_benchmark(&variant),
        "core-benchmark" => test_core_benchmark(&variant),
        _ => {
            println!("❌ Unknown component: {}", component);
            println!("Available components: json-benchmark, pretty-benchmark, core-benchmark");
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

    // TODO: Implement actual measurement
    // 1. Setup appropriate Cargo.toml for variant
    // 2. Run cargo bloat
    // 3. Run cargo llvm-lines
    // 4. Parse outputs
    // 5. Return BuildResult

    match variant {
        "facet-main" => {
            println!("🔄 Would switch to main branch and patch with PR ks-* crates");
        }
        "serde" => {
            println!("📊 Would measure using serde ecosystem");
        }
        "facet-pr" => {
            println!("🚀 Would measure using current facet PR");

            // For now, let's try to actually run cargo bloat on ks-facet
            match run_cargo_bloat("../ks-facet/Cargo.toml", false) {
                Ok(bloat_output) => {
                    println!("✅ cargo-bloat (functions) results:");
                    println!("   File size: {} bytes", bloat_output.file_size);
                    println!(
                        "   Text section size: {} bytes",
                        bloat_output.text_section_size
                    );
                    println!("   Top 5 functions:");
                    for (i, func) in bloat_output.functions.iter().take(5).enumerate() {
                        println!(
                            "   {}. {} ({}): {} bytes",
                            i + 1,
                            func.crate_name,
                            func.name,
                            func.size
                        );
                    }
                }
                Err(e) => {
                    println!("❌ Failed to run cargo-bloat (functions): {}", e);
                }
            }

            // Also run crates analysis
            match run_cargo_bloat("../ks-facet/Cargo.toml", true) {
                Ok(bloat_output) => {
                    println!("\n✅ cargo-bloat (crates) results:");
                    println!("   Top 5 crates:");
                    for (i, crate_info) in bloat_output.crates.iter().take(5).enumerate() {
                        println!(
                            "   {}. {}: {} bytes",
                            i + 1,
                            crate_info.name,
                            crate_info.size
                        );
                    }
                }
                Err(e) => {
                    println!("❌ Failed to run cargo-bloat (crates): {}", e);
                }
            }

            // Run LLVM lines analysis for each target crate
            match run_cargo_llvm_lines_for_crates(&target.facet_crates) {
                Ok(llvm_summary) => {
                    println!("\n✅ cargo-llvm-lines results:");
                    for (crate_name, line_count) in &llvm_summary.crate_results {
                        println!("   {}: {} LLVM IR lines", crate_name, line_count);
                    }
                    let total_lines: u32 = llvm_summary
                        .crate_results
                        .iter()
                        .map(|(_, count)| count)
                        .sum();
                    println!("   Total: {} LLVM IR lines", total_lines);
                }
                Err(e) => {
                    println!("❌ Failed to run cargo-llvm-lines: {}", e);
                }
            }

            // Measure build times
            match measure_build_time("../ks-facet/Cargo.toml") {
                Ok(timing_summary) => {
                    println!("\n✅ Build timing results:");
                    println!("   Total build time: {:.2}s", timing_summary.total_duration);
                    println!("   Top 5 slowest crates:");

                    for (i, (crate_name, duration)) in
                        timing_summary.crate_timings.iter().take(5).enumerate()
                    {
                        println!("   {}. {}: {:.2}s", i + 1, crate_name, duration);
                    }
                }
                Err(e) => {
                    println!("❌ Failed to measure build time: {}", e);
                }
            }
        }
        _ => unreachable!(),
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
        "20",
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

    for crate_name in crate_names {
        // Convert crate name to manifest path
        let manifest_path = format!("../{}/Cargo.toml", crate_name.replace('_', "-"));

        match run_cargo_llvm_lines_single(&manifest_path) {
            Ok(line_count) => {
                crate_results.push((crate_name.clone(), line_count));
            }
            Err(e) => {
                println!(
                    "⚠️  Failed to run cargo-llvm-lines for {}: {}",
                    crate_name, e
                );
                // Continue with other crates instead of failing completely
                crate_results.push((crate_name.clone(), 0));
            }
        }
    }

    Ok(LlvmLinesSummary { crate_results })
}

fn run_cargo_llvm_lines_single(manifest_path: &str) -> Result<u32> {
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

fn parse_llvm_lines_output(output: &str) -> Result<u32> {
    // Parse cargo-llvm-lines output to extract total line count
    // Look for line like "123456 (100.0%, 100.0%) (TOTAL)"

    for line in output.lines() {
        if line.contains("(TOTAL)") {
            let parts: Vec<&str> = line.split_whitespace().collect();
            if let Some(count_str) = parts.first() {
                let count: u32 = count_str
                    .parse()
                    .context("Failed to parse LLVM line count")?;
                return Ok(count);
            }
        }
    }

    anyhow::bail!("Could not find (TOTAL) line in cargo-llvm-lines output");
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
