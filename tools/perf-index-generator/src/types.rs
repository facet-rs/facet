//! Data types for metadata and performance data

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
}

/// Performance data from perf-data.json
#[derive(Debug, Clone, Facet)]
pub struct PerfDataFile {
    pub timestamp: String,
    pub benchmarks: HashMap<String, HashMap<String, u64>>,
}

impl PerfDataFile {
    /// Calculate total instruction count across all benchmarks
    pub fn total_instructions(&self) -> u64 {
        let mut total = 0u64;
        for targets in self.benchmarks.values() {
            for &instr in targets.values() {
                total += instr;
            }
        }
        total
    }
}
