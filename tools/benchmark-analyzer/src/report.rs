//! Report types and utilities for benchmark-analyzer.
//!
//! HTML report generation has been removed in favor of the unified SPA
//! that renders reports client-side from run.json data.

// Re-export from benchmark_defs
pub use benchmark_defs::load_categories;

/// Git information for the benchmark run
pub struct GitInfo {
    pub commit: String,
    pub commit_short: String,
    pub branch: String,
    /// Full commit message
    pub commit_message: String,
    /// PR number (from CI environment or git)
    pub pr_number: Option<String>,
    /// PR title (from CI environment)
    pub pr_title: Option<String>,
}
