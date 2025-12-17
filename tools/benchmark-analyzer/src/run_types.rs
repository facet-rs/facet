//! Types for run-v1.json format
//!
//! This module defines the data structures for per-run benchmark data,
//! following the run-v1.json spec.
//!
//! Note: These types are currently used for documentation purposes.
//! The actual JSON serialization is done manually in export_run_json().

#![allow(dead_code)]

use facet::Facet;
use std::collections::HashMap;

/// Top-level run data structure (run-v1.json)
///
/// Note: This struct does not derive Facet because it contains
/// CaseResults which has a non-Facet enum. JSON serialization
/// is done manually in export_run_json().
#[derive(Debug, Clone)]
pub struct RunData {
    /// Format version (always 1)
    pub version: u32,

    /// Run metadata
    pub run: RunMeta,

    /// Schema information (targets, metrics, etc.)
    pub schema: RunSchema,

    /// Benchmark groups for sidebar navigation
    pub groups: Vec<Group>,

    /// Results: case_id -> CaseResults
    pub results: HashMap<String, CaseResults>,

    /// Parse failures and warnings
    pub diagnostics: Diagnostics,
}

/// Run metadata
#[derive(Debug, Clone, Facet)]
pub struct RunMeta {
    /// Repository name (e.g., "facet-rs/facet")
    pub repo: String,

    /// Unique run identifier (e.g., "main:3a63f78f")
    pub run_id: String,

    /// URL-safe branch key (e.g., "main", "bench-improvements")
    pub branch_key: String,

    /// Original branch name (e.g., "gh-readonly-queue/main/pr-1315-...")
    #[facet(rename = "branch_original")]
    pub branch_original: Option<String>,

    /// Full commit SHA
    pub commit: String,

    /// Short commit SHA (8 chars)
    pub commit_short: String,

    /// ISO 8601 timestamp when this run was generated
    pub generated_at: String,

    /// Tooling information
    pub tooling: Option<Tooling>,

    /// Environment information
    pub env: Option<RunEnv>,
}

/// Tooling versions used for benchmarks
#[derive(Debug, Clone, Facet)]
pub struct Tooling {
    /// Divan timing benchmark info
    pub divan: Option<ToolInfo>,
    /// Gungraun instruction count benchmark info
    pub gungraun: Option<ToolInfo>,
}

/// Info about a benchmark tool
#[derive(Debug, Clone, Facet)]
pub struct ToolInfo {
    /// Whether this tool was used
    pub present: bool,
    /// Tool version (if known)
    pub version: Option<String>,
}

/// Environment information for the benchmark run
#[derive(Debug, Clone, Facet)]
pub struct RunEnv {
    /// CI runner name
    pub runner: Option<String>,
    /// Operating system
    pub os: Option<String>,
    /// CPU info
    pub cpu: Option<String>,
    /// Additional notes
    pub notes: Option<String>,
}

/// Schema information for interpreting results
#[derive(Debug, Clone, Facet)]
pub struct RunSchema {
    /// Available operations (e.g., ["deserialize", "serialize"])
    pub operations: Vec<String>,

    /// Available targets with metadata
    pub targets: Vec<TargetInfo>,

    /// Available metrics with metadata
    pub metrics: Vec<MetricInfo>,

    /// Default selections
    pub defaults: SchemaDefaults,

    /// Optional case label overrides
    pub case_labels: Option<HashMap<String, String>>,
}

/// Target (implementation) information
#[derive(Debug, Clone, Facet)]
pub struct TargetInfo {
    /// Stable target identifier (e.g., "serde_json", "facet_format_jit")
    pub id: String,
    /// Display label (e.g., "serde_json", "facet-format+jit")
    pub label: String,
    /// Target kind for styling
    pub kind: Option<String>, // "baseline" | "facet" | "other"
}

/// Metric information
#[derive(Debug, Clone, Facet)]
pub struct MetricInfo {
    /// Stable metric identifier (e.g., "time_median_ns", "instructions")
    pub id: String,
    /// Display label
    pub label: String,
    /// Unit (e.g., "ns", "count")
    pub unit: String,
    /// Whether lower values are better
    pub better: String, // "lower" | "higher"
    /// Data source
    pub source: String, // "divan" | "gungraun" | "derived"
}

/// Default schema selections
#[derive(Debug, Clone, Facet)]
pub struct SchemaDefaults {
    /// Default baseline target for comparisons
    pub baseline_target: String,
    /// Default primary metric for display
    pub primary_metric: String,
}

/// A group of benchmark cases (for sidebar navigation)
#[derive(Debug, Clone, Facet)]
pub struct Group {
    /// Stable group identifier
    pub group_id: String,
    /// Display label
    pub label: String,
    /// Optional description
    pub description: Option<String>,
    /// Cases in this group
    pub cases: Vec<CaseInfo>,
}

/// Basic case info for groups
#[derive(Debug, Clone, Facet)]
pub struct CaseInfo {
    /// Case identifier
    pub case_id: String,
    /// Display label
    pub label: String,
}

/// Results for a single benchmark case
#[derive(Debug, Clone)]
pub struct CaseResults {
    /// Results per target: target_id -> TargetResults
    pub targets: HashMap<String, TargetResults>,
}

/// Results for a target within a case
#[derive(Debug, Clone)]
pub struct TargetResults {
    /// Results per operation: "deserialize" | "serialize" -> TargetOpResult
    pub ops: HashMap<String, TargetOpResult>,
}

/// Result for a specific (case, target, operation) combination
///
/// In JSON output, this is serialized as:
/// - Success: { "ok": true, "metrics": { ... } }
/// - Error: { "ok": false, "error": { ... } }
#[derive(Debug, Clone)]
pub enum TargetOpResult {
    /// Successful benchmark result
    Ok { metrics: MetricValues },
    /// Failed benchmark result
    Err { error: BenchmarkError },
}

impl TargetOpResult {
    /// Create a successful result
    pub fn ok(metrics: MetricValues) -> Self {
        Self::Ok { metrics }
    }

    /// Create an error result
    pub fn err(error: BenchmarkError) -> Self {
        Self::Err { error }
    }

    /// Check if this is a successful result
    pub fn is_ok(&self) -> bool {
        matches!(self, Self::Ok { .. })
    }
}

/// Metric values for a successful benchmark
#[derive(Debug, Clone, Default, Facet)]
pub struct MetricValues {
    /// Wall-clock median time in nanoseconds (from divan)
    pub time_median_ns: Option<f64>,
    /// Instruction count (from gungraun)
    pub instructions: Option<u64>,
    /// L1 cache hits (from gungraun)
    pub l1_hits: Option<u64>,
    /// Last-level cache hits (from gungraun)
    pub ll_hits: Option<u64>,
    /// RAM hits (from gungraun)
    pub ram_hits: Option<u64>,
    /// Total read+write operations (from gungraun)
    pub total_read_write: Option<u64>,
    /// Estimated CPU cycles (from gungraun)
    pub estimated_cycles: Option<u64>,
}

/// Error information for a failed benchmark
#[derive(Debug, Clone, Facet)]
pub struct BenchmarkError {
    /// Error kind (e.g., "compile", "runtime", "timeout")
    pub kind: String,
    /// Error message
    pub message: String,
    /// Additional details
    pub details: Option<String>,
}

/// Diagnostic information from parsing/running benchmarks
#[derive(Debug, Clone, Default, Facet)]
pub struct Diagnostics {
    /// Parse failures by tool
    pub parse_failures: Option<ParseFailures>,
    /// General notes
    pub notes: Option<Vec<String>>,
}

/// Parse failures grouped by tool
#[derive(Debug, Clone, Default, Facet)]
pub struct ParseFailures {
    /// Divan parse failures
    pub divan: Option<Vec<String>>,
    /// Gungraun parse failures
    pub gungraun: Option<Vec<String>>,
}
