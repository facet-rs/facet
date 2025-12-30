//! Data types for metadata and performance data
//!
//! This module contains types for:
//! - Legacy perf-data.json format (PerfDataFile, CommitMetadata)
//! - New index-v2.json format (IndexV2, BranchInfo, Commit, etc.)

use facet::Facet;
use std::collections::HashMap;

/// Metadata from metadata.json
#[derive(Debug, Clone, Facet)]
pub struct CommitMetadata {
    pub commit: String,
    pub commit_short: String,
    pub branch: String,
    pub branch_original: String,
    pub pr_number: Option<String>, // facet handles Option automatically
    pub timestamp: String,
    pub timestamp_display: String,
    pub commit_message: String,
    pub pr_title: String,
}

/// Performance data from perf-data.json
#[derive(Debug, Clone, Facet)]
pub struct PerfDataFile {
    pub timestamp: String,
    pub benchmarks: HashMap<String, HashMap<String, u64>>,
}

// Note: PerfDataFile methods removed - no longer used with index-v2 format

// =============================================================================
// Index V2 Types (new commit-centric format)
// =============================================================================

/// Top-level index structure (index-v2.json)
#[derive(Debug, Clone, Facet)]
pub struct IndexV2 {
    /// Format version (always 2)
    pub version: u32,

    /// ISO 8601 timestamp when this index was generated
    pub generated_at: String,

    /// Repository name (e.g., "facet-rs/facet")
    pub repo: String,

    /// Metric specifications for UI rendering
    pub metric_specs: HashMap<String, MetricSpec>,

    /// Default metrics for display
    pub defaults: IndexDefaults,

    /// Baseline run reference (usually latest main)
    pub baseline: Option<BaselineInfo>,

    /// Branch metadata: branch_key -> BranchInfo
    pub branches: HashMap<String, BranchInfo>,

    /// Per-branch commit lists (newest-first): branch_key -> [BranchCommitRef]
    pub branch_commits: HashMap<String, Vec<BranchCommitRef>>,

    /// All commits: sha -> Commit
    pub commits: HashMap<String, Commit>,
}

/// Metric specification for UI rendering
#[derive(Debug, Clone, Facet)]
pub struct MetricSpec {
    /// Display label
    pub label: String,
    /// Unit type: "count", "ns", "x", "pct"
    pub unit: String,
    /// Whether lower is better: "lower" or "higher"
    pub better: String,
    /// Format type: "int", "ratio", "percent"
    pub format: String,
    /// Data source: "divan", "gungraun", "derived"
    pub source: Option<String>,
    /// Additional notes
    pub notes: Option<String>,
}

/// Default index display settings
#[derive(Debug, Clone, Facet)]
pub struct IndexDefaults {
    /// Default metric to display (e.g., "instructions")
    pub index_metric: String,
    /// Default operation: "deserialize" or "serialize"
    pub index_operation: String,
    /// Baseline target for comparisons (e.g., "serde_json")
    pub baseline_target: String,
    /// Headline target to highlight (e.g., "facet_json_t2")
    pub headline_target: String,
    /// Ratio display mode: "speedup" or "relative_cost"
    pub ratio_mode: String,
    /// Max commits to show by default
    pub max_commits_default: Option<u32>,
}

/// Baseline run information
#[derive(Debug, Clone, Facet)]
pub struct BaselineInfo {
    /// Display name (e.g., "main tip")
    pub name: String,
    /// Branch key
    pub branch_key: String,
    /// Full commit SHA
    pub commit_sha: String,
    /// Short commit SHA (8 chars)
    pub commit_short: String,
    /// Operation for comparison
    pub operation: String,
    /// Metric for comparison
    pub metric: String,
    /// Baseline target (e.g., "serde_json")
    pub baseline_target: String,
    /// Headline target (e.g., "facet_json_t2")
    pub headline_target: String,
    /// ISO 8601 timestamp
    pub timestamp: String,
    /// URL to baseline run.json
    pub run_json_url: String,
}

/// Branch metadata
#[derive(Debug, Clone, Facet)]
pub struct BranchInfo {
    /// URL-safe branch key
    pub key: String,
    /// Display name
    pub display: String,
    /// Branch kind: "main", "pr", "queue", "renovate", "feature"
    pub kind: String,
    /// Original VCS branch name
    pub branch_original: Option<String>,
    /// PR number if applicable
    pub pr_number: Option<String>,
    /// ISO 8601 timestamp of latest commit
    pub last_timestamp: String,
}

/// Commit reference within a branch (for commit picker)
#[derive(Debug, Clone, Facet)]
pub struct BranchCommitRef {
    /// Full commit SHA
    pub sha: String,
    /// Short commit SHA (8 chars)
    pub short: String,
    /// Unix timestamp
    pub timestamp_unix: i64,
    /// Parent commit SHA (for "vs parent" comparisons)
    pub parent_sha: Option<String>,
    /// URL to run.json for this commit
    pub run_json_url: Option<String>,
}

/// Commit data (canonical, deduplicated by SHA)
#[derive(Debug, Clone, Facet)]
pub struct Commit {
    /// Full commit SHA
    pub sha: String,
    /// Short commit SHA (8 chars)
    pub short: String,
    /// First line of PR title or commit message
    pub subject: String,
    /// Unix timestamp (canonical timestamp for sorting)
    pub timestamp_unix: i64,
    /// Branch keys where this commit has runs
    pub branches_present: Vec<String>,
    /// Default branch selection for deduped timeline
    pub primary_default: PrimaryDefault,
    /// Runs keyed by branch: branch_key -> CommitRunIndexEntry
    pub runs: HashMap<String, CommitRunIndexEntry>,
}

/// Primary default selection for a commit
#[derive(Debug, Clone, Facet)]
pub struct PrimaryDefault {
    /// Default branch key (usually "main" if present)
    pub branch_key: String,
}

/// Index entry for a run within a commit
#[derive(Debug, Clone, Facet)]
pub struct CommitRunIndexEntry {
    /// Branch key
    pub branch_key: String,
    /// Original branch name
    pub branch_original: Option<String>,
    /// PR number if applicable
    pub pr_number: Option<String>,
    /// PR title if applicable
    pub pr_title: Option<String>,
    /// ISO 8601 timestamp
    pub timestamp: String,
    /// Full commit message
    pub commit_message: String,
    /// URL to run.json
    pub run_json_url: String,
}
