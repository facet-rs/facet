// measure-bloat/src/types.rs
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::PathBuf;
use std::time::Duration;

// --- Build Process Types ---

/// Options for building a Rust project with LLVM IR output enabled.
#[derive(Debug, Clone)]
pub struct BuildWithLllvmIrOpts {
    /// Path to the Cargo.toml manifest file for the crate or workspace.
    pub manifest_path: String,
    /// Target directory for the build artifacts.
    pub target_dir: Option<PathBuf>,
    /// Environment variables to set for the cargo build command.
    pub env_vars: HashMap<String, String>,
}

/// Output of a build process that also generated LLVM IR files.
#[derive(Debug)]
pub struct LlvmBuildOutput {
    /// The target directory used for this build. LLVM IR files (.ll)
    /// will be located in `target_dir/release/deps/*.ll`.
    pub target_dir: PathBuf,
    /// Summary of build timings gathered during the LLVM IR generation build.
    pub timing_summary: BuildTimingSummary,
}

// --- Build Timing Types ---

/// Represents timing information for a specific item in cargo's build process.
/// Part of the structure parsed from `cargo build --timings=json` output.
#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct CargoTimingTarget {
    pub name: String,
    // Cargo's JSON output for `kind` is an array of strings e.g. `["bin"]` or `["lib", "rlib"]`
    // pub kind: Vec<String>,
    // pub src_path: String,
}

/// Represents a single timing entry from cargo's build timing report.
/// Parsed from `cargo build --timings=json` output lines.
#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct CargoTimingEntry {
    /// Reason for the timing entry, e.g., "compiler-artifact", "build-script-executed".
    pub reason: String,
    pub package_id: String,
    pub target: CargoTimingTarget,
    /// Duration of this specific entry in seconds.
    pub duration: f64,
    /// Time spent generating rmeta files in seconds (optional).
    #[serde(default, rename = "rmeta_time")]
    pub rmeta_time: Option<f64>,
}

/// Summarized timing information for a single crate.
#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct CrateTiming {
    pub name: String,
    /// Total duration for this crate in seconds.
    pub duration: f64,
}

/// Summary of build timings for all crates and the total build time.
#[derive(Debug, Serialize, Deserialize, Clone, Default)]
pub struct BuildTimingSummary {
    /// Total build duration.
    pub total_duration: Duration,
    /// List of individual crate timings, sorted by duration descending.
    pub crate_timings: Vec<CrateTiming>,
}

// --- LLVM Lines Analysis Types ---

/// Represents a single function's contribution to LLVM lines.
#[derive(Debug, Serialize, Deserialize, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct LlvmFunction {
    pub name: String,
    pub lines: u64,
    pub copies: u64,
    // pub crate_name: Option<String>, // Potentially add if easily available from parser
}

/// LLVM line count summary for a single crate.
#[derive(Debug, Serialize, Deserialize, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct CrateLlvmLines {
    pub name: String,
    pub lines: u64,
    pub copies: u64,
}

/// Summary of LLVM lines analysis, including top crates and functions.
#[derive(Debug, Serialize, Deserialize, Clone, Default)]
pub struct LlvmLinesSummary {
    /// LLVM line results per crate, sorted by line count descending.
    pub crate_results: Vec<CrateLlvmLines>,
    /// Top functions by LLVM IR line count, sorted descending.
    pub top_functions: Vec<LlvmFunction>,
}

// --- Size Measurement Types ---

/// Represents the size of a compiled .rlib artifact for a crate.
#[derive(Debug, Serialize, Deserialize, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct CrateRlibSize {
    pub name: String,
    /// Size of the .rlib file in bytes.
    pub size: u64,
}

// --- Consolidated Build Result ---

/// Consolidated result of a single build and measurement pass for a target and variant.
#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct BuildResult {
    /// Name of the measurement target (e.g. "json-serialization-test").
    pub target_name: String,
    /// Variant of the build ("head-facet", "main-facet", "serde").
    pub variant_name: String,
    /// Total file size of the main binary artifact in bytes (optional).
    pub main_executable_size: Option<u64>,
    // pub text_section_size: Option<u64>, // Currently no tool to populate this easily
    /// Total build time in milliseconds.
    pub build_time_ms: u128,
    /// Sizes of .rlib files for tracked dependencies.
    pub rlib_sizes: Vec<CrateRlibSize>,
    /// Summary of LLVM lines analysis (optional).
    pub llvm_lines: Option<LlvmLinesSummary>,
    /// Detailed build timing summary.
    pub build_timing_summary: BuildTimingSummary,
}

// --- Reporting Types ---

/// Represents a change in size for a crate (used for .rlib, LLVM lines, etc.).
#[derive(Debug, Clone)]
pub struct CrateSizeChange {
    pub name: String,
    /// Size in the baseline (e.g., main branch or serde).
    pub base_size: u64,
    /// Size in the current version being compared (e.g., HEAD).
    pub current_size: u64,
    pub delta: i64,
}

/// Represents the difference in LLVM metrics for a single crate between two variants.
#[derive(Debug, Clone)]
pub struct LlvmCrateDiff {
    pub crate_name: String,
    pub base_lines: u64,
    pub current_lines: u64,
    pub base_copies: u64,
    pub current_copies: u64,
    pub delta_lines: i64,
    pub delta_copies: i64,
}

// Potentially add more report-specific intermediate structs if needed
