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
    pub commit_message: String,
    pub pr_title: String,
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

    /// Calculate the ratio of facet-format-json+jit to serde_json instructions
    /// Returns the ratio as a percentage (e.g., 0.85 means facet is 85% of serde)
    pub fn facet_vs_serde_ratio(&self) -> Option<f64> {
        let mut facet_total = 0u64;
        let mut serde_total = 0u64;

        for targets in self.benchmarks.values() {
            for (target_name, &instr) in targets {
                if target_name.contains("facet_format")
                    && target_name.contains("jit")
                    && !target_name.contains("cached")
                {
                    facet_total += instr;
                } else if target_name.contains("serde") {
                    serde_total += instr;
                }
            }
        }

        if serde_total > 0 {
            Some((facet_total as f64) / (serde_total as f64))
        } else {
            None
        }
    }
}
