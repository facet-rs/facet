//! Types for run-v1.json format
//!
//! These types are used for both serialization (in benchmark-analyzer)
//! and deserialization (in perf-index-generator).
//!
//! There are two schema versions:
//! - Old schema: results.{benchmark}.targets.{target}.ops.{operation}.metrics
//! - New schema: results.values.{benchmark}.{operation}.{target}

use facet::Facet;
use indexmap::IndexMap;

// =============================================================================
// Minimal types for metrics extraction (handles both old and new schemas)
// =============================================================================

/// Minimal run.json for metrics extraction - handles both schemas
#[derive(Debug, Clone, Facet)]
pub struct RunJsonMinimal {
    /// Results section
    pub results: ResultsMinimal,
}

/// Results section - handles both old and new schemas
#[derive(Debug, Clone, Facet)]
#[facet(skip_all_unless_truthy)]
pub struct ResultsMinimal {
    /// New schema: results.values.{benchmark}.{operation}.{target}
    pub values: Option<IndexMap<String, BenchmarkOps>>,
    // Old schema fields are handled by flattening - they'll be ignored
}

/// Minimal metrics for extraction
#[derive(Debug, Clone, Default, Facet)]
#[facet(skip_all_unless_truthy)]
pub struct MetricsMinimal {
    pub instructions: Option<u64>,
}

// =============================================================================
// Full types for new schema serialization
// =============================================================================

/// Top-level run.json structure (run-v1 schema)
#[derive(Debug, Clone, Facet)]
#[facet(skip_all_unless_truthy)]
pub struct RunJson {
    /// Schema version identifier (may be absent in old schema)
    pub schema: Option<String>,

    /// Run metadata
    pub run: RunMeta,

    /// Default display settings (may be absent in old schema)
    pub defaults: Option<RunDefaults>,

    /// Catalog of groups, benchmarks, targets, metrics (may be absent in old schema)
    pub catalog: Option<RunCatalog>,

    /// Benchmark results
    pub results: RunResults,
}

/// Run metadata
#[derive(Debug, Clone, Facet)]
#[facet(skip_all_unless_truthy)]
pub struct RunMeta {
    /// Unique run identifier (e.g., "main/3a63f78f")
    pub run_id: String,

    /// URL-safe branch key (e.g., "main", "bench-improvements")
    pub branch_key: String,

    /// Original branch name if different from branch_key
    pub branch_original: Option<String>,

    /// Full commit SHA (new schema)
    pub sha: Option<String>,

    /// Full commit SHA (old schema, for backward compat)
    pub commit: Option<String>,

    /// Short commit SHA (new schema)
    pub short: Option<String>,

    /// Short commit SHA (old schema, for backward compat)
    pub commit_short: Option<String>,

    /// ISO 8601 timestamp (new schema)
    pub timestamp: Option<String>,

    /// ISO 8601 timestamp (old schema, for backward compat)
    pub generated_at: Option<String>,

    /// Unix timestamp
    pub timestamp_unix: Option<i64>,

    /// Commit message
    pub commit_message: String,

    /// PR number if applicable
    pub pr_number: Option<String>,

    /// PR title if applicable
    pub pr_title: Option<String>,

    /// Tool versions used
    pub tool_versions: Option<ToolVersions>,
}

impl RunMeta {
    /// Get the commit SHA (handles both old and new schema)
    #[allow(dead_code)]
    pub fn get_sha(&self) -> Option<&str> {
        self.sha.as_deref().or(self.commit.as_deref())
    }

    /// Get the short commit SHA (handles both old and new schema)
    #[allow(dead_code)]
    pub fn get_short(&self) -> Option<&str> {
        self.short.as_deref().or(self.commit_short.as_deref())
    }

    /// Get the timestamp (handles both old and new schema)
    #[allow(dead_code)]
    pub fn get_timestamp(&self) -> Option<&str> {
        self.timestamp.as_deref().or(self.generated_at.as_deref())
    }
}

/// Tool versions
#[derive(Debug, Clone, Facet)]
pub struct ToolVersions {
    pub divan: String,
    pub gungraun: String,
}

/// Default display settings
#[derive(Debug, Clone, Facet)]
pub struct RunDefaults {
    pub operation: String,
    pub metric: String,
    pub baseline_target: String,
    pub primary_target: String,
    pub comparison_mode: String,
}

/// Catalog of benchmark metadata
#[derive(Debug, Clone, Facet)]
pub struct RunCatalog {
    /// Order of formats (e.g., ["json", "postcard"])
    pub formats_order: Vec<String>,

    /// Format definitions
    pub formats: IndexMap<String, FormatDef>,

    /// Order of groups
    pub groups_order: Vec<String>,

    /// Group definitions (IndexMap preserves insertion order for JSON)
    pub groups: IndexMap<String, GroupDef>,

    /// Benchmark definitions (IndexMap preserves insertion order for JSON)
    pub benchmarks: IndexMap<String, BenchmarkDef>,

    /// Target definitions (IndexMap preserves insertion order for JSON)
    pub targets: IndexMap<String, TargetDef>,

    /// Metric definitions (IndexMap preserves insertion order for JSON)
    pub metrics: IndexMap<String, MetricDef>,
}

/// Format definition (e.g., JSON, Postcard)
#[derive(Debug, Clone, Facet)]
pub struct FormatDef {
    pub key: String,
    pub label: String,
    /// Baseline target for this format (e.g., "serde_json" for JSON)
    pub baseline_target: String,
    /// Primary facet target for this format (e.g., "facet_json_t2" for JSON)
    pub primary_target: String,
}

/// Group definition
#[derive(Debug, Clone, Facet)]
pub struct GroupDef {
    pub label: String,
    pub benchmarks_order: Vec<String>,
}

/// Benchmark definition
#[derive(Debug, Clone, Facet)]
pub struct BenchmarkDef {
    pub key: String,
    pub label: String,
    pub group: String,
    /// Format this benchmark belongs to (e.g., "json", "postcard")
    pub format: String,
    pub targets_order: Vec<String>,
    pub metrics_order: Vec<String>,
}

/// Target definition
#[derive(Debug, Clone, Facet)]
pub struct TargetDef {
    pub key: String,
    pub label: String,
    pub kind: String,
}

/// Metric definition
#[derive(Debug, Clone, Facet)]
pub struct MetricDef {
    pub key: String,
    pub label: String,
    pub unit: String,
    pub better: String,
}

/// Results section
#[derive(Debug, Clone, Facet)]
pub struct RunResults {
    /// Benchmark results: benchmark_name -> BenchmarkOps
    pub values: IndexMap<String, BenchmarkOps>,

    /// Errors section (parse failures, etc.)
    pub errors: RunErrors,
}

/// Operations for a benchmark (deserialize/serialize)
#[derive(Debug, Clone, Facet)]
pub struct BenchmarkOps {
    /// Deserialization results by target
    pub deserialize: IndexMap<String, Option<TargetMetrics>>,

    /// Serialization results by target
    pub serialize: IndexMap<String, Option<TargetMetrics>>,
}

/// Metrics for a single target
#[derive(Debug, Clone, Default, Facet)]
#[facet(skip_all_unless_truthy)]
pub struct TargetMetrics {
    /// Instruction count (primary metric, from gungraun)
    pub instructions: Option<u64>,

    /// Estimated CPU cycles (from gungraun)
    pub estimated_cycles: Option<u64>,

    /// Median time in nanoseconds (from divan)
    pub time_median_ns: Option<f64>,

    /// L1 cache hits (from gungraun)
    pub l1_hits: Option<u64>,

    /// Last-level cache hits (from gungraun)
    pub ll_hits: Option<u64>,

    /// RAM hits (from gungraun)
    pub ram_hits: Option<u64>,

    /// Total read/write operations (from gungraun)
    pub total_read_write: Option<u64>,

    /// JIT tier tracking: Tier-2 attempts (for format+jit2 target)
    pub tier2_attempts: Option<u64>,

    /// JIT tier tracking: Tier-2 successes (for format+jit2 target)
    pub tier2_successes: Option<u64>,

    /// JIT tier tracking: Tier-2 compile unsupported (for format+jit2 target)
    pub tier2_compile_unsupported: Option<u64>,

    /// JIT tier tracking: Tier-2 runtime unsupported (for format+jit2 target)
    pub tier2_runtime_unsupported: Option<u64>,

    /// JIT tier tracking: Tier-2 runtime error (for format+jit2 target)
    pub tier2_runtime_error: Option<u64>,

    /// JIT tier tracking: Tier-1 fallbacks (for format+jit2 target)
    pub tier1_fallbacks: Option<u64>,
}

/// Errors section
#[derive(Debug, Clone, Default, Facet)]
#[facet(skip_all_unless_truthy)]
pub struct RunErrors {
    /// Parse failures grouped by tool
    #[facet(rename = "_parse_failures")]
    pub parse_failures: Option<ParseFailures>,
}

/// Parse failures by tool
#[derive(Debug, Clone, Default, Facet)]
pub struct ParseFailures {
    pub divan: Vec<String>,
    pub gungraun: Vec<String>,
}
