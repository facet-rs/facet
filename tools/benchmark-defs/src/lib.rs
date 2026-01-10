//! Shared benchmark definitions for benchmark-generator and benchmark-analyzer.
//!
//! Supports multiple serialization formats (JSON, Postcard, MessagePack, etc.)
//! Each format has its own YAML file defining benchmarks and types.

use facet::Facet;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::Path;

/// Arguments for the bench command, shared between xtask and benchmark-analyzer.
#[derive(Facet, Debug, Default, Serialize, Deserialize)]
pub struct BenchReportArgs {
    /// Filter to run only specific benchmark(s), e.g., "booleans"
    pub filter: Option<String>,

    /// Start HTTP server to view the report after generation
    pub serve: bool,

    /// Skip running benchmarks, reuse previous benchmark data
    pub no_run: bool,

    /// Skip cloning perf.facet.rs and generating index (just export raw data)
    pub no_index: bool,

    /// Push results to perf.facet.rs (refuses if filter is set)
    pub push: bool,
}

/// A complete benchmark suite file (one per format).
#[derive(Debug, Serialize, Deserialize)]
pub struct BenchmarkFile {
    /// Format configuration (required)
    pub format: FormatConfig,
    /// Benchmark definitions
    #[serde(default)]
    pub benchmarks: Vec<BenchmarkDef>,
    /// Type definitions used by benchmarks
    #[serde(default)]
    pub type_defs: Vec<TypeDef>,
}

/// Format-specific configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FormatConfig {
    /// Format identifier (e.g., "json", "postcard", "msgpack")
    pub name: String,
    /// Baseline implementation crate (e.g., "serde_json", "postcard")
    pub baseline: String,
    /// Facet implementation crate (e.g., "facet_json", "facet_postcard")
    pub facet_crate: String,
    /// Which JIT tiers to benchmark (default: `[1, 2]` for text formats, `[2]` for binary formats)
    /// T1 = shape-based JIT (works for text formats with structural tokens)
    /// T2 = format-specific JIT (works for all formats with FormatJitParser)
    #[serde(default)]
    pub jit_tiers: Option<u8>,
}

impl FormatConfig {
    /// Get the JIT tiers to benchmark. Defaults to [1, 2] if not specified.
    pub fn jit_tiers(&self) -> Vec<u8> {
        match self.jit_tiers {
            Some(2) => vec![2],
            Some(1) => vec![1],
            None => vec![1, 2],
            Some(n) => vec![n],
        }
    }

    /// Check if T1 benchmarks should be generated.
    pub fn has_t1(&self) -> bool {
        self.jit_tiers().contains(&1)
    }

    /// Check if T2 benchmarks should be generated.
    pub fn has_t2(&self) -> bool {
        self.jit_tiers().contains(&2)
    }
}

/// A single benchmark definition.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BenchmarkDef {
    pub name: String,
    #[serde(rename = "type")]
    pub type_name: String,
    pub category: String,
    /// Inline JSON payload
    #[serde(default)]
    pub json: Option<String>,
    /// JSON from file path
    #[serde(default)]
    pub json_file: Option<String>,
    /// Brotli-compressed JSON from file path
    #[serde(default)]
    pub json_brotli: Option<String>,
    /// Generated payload (format-agnostic generator name)
    #[serde(default)]
    pub generated: Option<String>,
    /// Inline binary payload (hex-encoded)
    #[serde(default)]
    pub binary_hex: Option<String>,
    /// Inline binary payload (base64-encoded)
    #[serde(default)]
    pub binary_base64: Option<String>,
}

/// A type definition used in benchmarks.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TypeDef {
    pub name: String,
    pub code: String,
}

/// Parse a single benchmark file from YAML.
pub fn parse_benchmarks(yaml_path: &Path) -> Result<BenchmarkFile, Box<dyn std::error::Error>> {
    let content = std::fs::read_to_string(yaml_path)?;
    let file: BenchmarkFile = serde_yaml::from_str(&content)?;
    Ok(file)
}

/// Discover and parse all benchmark files in a directory.
/// Returns a map of format_name -> BenchmarkFile.
pub fn discover_benchmark_files(
    benches_dir: &Path,
) -> Result<HashMap<String, BenchmarkFile>, Box<dyn std::error::Error>> {
    let mut files = HashMap::new();

    for entry in std::fs::read_dir(benches_dir)? {
        let entry = entry?;
        let path = entry.path();
        if path
            .extension()
            .is_some_and(|ext| ext == "yaml" || ext == "yml")
        {
            match parse_benchmarks(&path) {
                Ok(file) => {
                    let format_name = file.format.name.clone();
                    files.insert(format_name, file);
                }
                Err(e) => {
                    eprintln!("Warning: Failed to parse {}: {}", path.display(), e);
                }
            }
        }
    }

    Ok(files)
}

/// Load benchmark categories from all YAML files.
/// Returns a map of (format, benchmark_name) -> category.
pub fn load_categories(workspace_root: &Path) -> HashMap<(String, String), String> {
    let benches_dir = workspace_root.join("facet-perf-shootout/benches");
    let mut categories = HashMap::new();

    if let Ok(files) = discover_benchmark_files(&benches_dir) {
        for (format_name, file) in files {
            for bench in file.benchmarks {
                categories.insert((format_name.clone(), bench.name), bench.category);
            }
        }
    }

    categories
}

/// Per-format benchmark ordering: (section_order, benchmarks_by_section)
pub type FormatBenchmarkOrder = (Vec<String>, HashMap<String, Vec<String>>);

/// Ordered benchmark groups per format.
///
/// Returns HashMap<format_name, FormatBenchmarkOrder>
pub fn load_ordered_benchmarks(workspace_root: &Path) -> HashMap<String, FormatBenchmarkOrder> {
    let benches_dir = workspace_root.join("facet-perf-shootout/benches");

    // Canonical section order
    let section_order = vec![
        "micro".to_string(),
        "synthetic".to_string(),
        "realistic".to_string(),
    ];

    let mut result = HashMap::new();

    if let Ok(files) = discover_benchmark_files(&benches_dir) {
        for (format_name, file) in files {
            let mut benchmarks_by_section: HashMap<String, Vec<String>> = HashMap::new();
            for section in &section_order {
                benchmarks_by_section.insert(section.clone(), Vec::new());
            }

            for bench in file.benchmarks {
                if let Some(list) = benchmarks_by_section.get_mut(&bench.category) {
                    list.push(bench.name);
                }
            }

            result.insert(format_name, (section_order.clone(), benchmarks_by_section));
        }
    }

    result
}

/// Section display labels
pub fn section_label(section: &str) -> &'static str {
    match section {
        "micro" => "Micro Benchmarks",
        "synthetic" => "Synthetic Benchmarks",
        "realistic" => "Realistic Benchmarks",
        _ => "Other",
    }
}

/// Format display labels
pub fn format_label(format: &str) -> &'static str {
    match format {
        "json" => "JSON",
        "postcard" => "Postcard",
        "msgpack" => "MessagePack",
        "yaml" => "YAML",
        "toml" => "TOML",
        _ => "Other",
    }
}

/// Get target names for a format.
/// Returns (baseline, t0, t1, t2) target names.
pub fn format_targets(format: &FormatConfig) -> (String, String, String, String) {
    let baseline = format.baseline.clone();
    let facet = &format.facet_crate;
    (
        baseline,
        format!("{}_t0", facet),
        format!("{}_t1", facet),
        format!("{}_t2", facet),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_format_block() {
        let content = r#"
format:
  name: json
  baseline: serde_json
  facet_crate: facet_json

benchmarks:
  - name: test
    type: String
    category: micro
    json: "hello"

type_defs:
  - name: Foo
    code: "struct Foo { x: u64 }"
"#;

        let file: BenchmarkFile = serde_yaml::from_str(content).expect("should parse");
        assert_eq!(file.format.name, "json");
        assert_eq!(file.format.baseline, "serde_json");
        assert_eq!(file.format.facet_crate, "facet_json");
        assert_eq!(file.benchmarks.len(), 1);
        assert_eq!(file.benchmarks[0].name, "test");
        assert_eq!(file.type_defs.len(), 1);
    }

    #[test]
    fn test_format_targets() {
        let format = FormatConfig {
            name: "json".to_string(),
            baseline: "serde_json".to_string(),
            facet_crate: "facet_json".to_string(),
            jit_tiers: None,
        };

        let (baseline, t0, t1, t2) = format_targets(&format);
        assert_eq!(baseline, "serde_json");
        assert_eq!(t0, "facet_json_t0");
        assert_eq!(t1, "facet_json_t1");
        assert_eq!(t2, "facet_json_t2");
    }

    #[test]
    fn test_parse_real_json_yaml() {
        // Test parsing the actual json.yaml file
        let workspace_root = std::env::current_dir().unwrap();
        let yaml_path = workspace_root.join("facet-perf-shootout/benches/json.yaml");

        if yaml_path.exists() {
            let file = parse_benchmarks(&yaml_path).expect("should parse json.yaml");
            assert_eq!(file.format.name, "json");
            assert_eq!(file.format.baseline, "serde_json");
            assert_eq!(file.format.facet_crate, "facet_json");
            assert!(!file.benchmarks.is_empty(), "should have benchmarks");
            assert!(!file.type_defs.is_empty(), "should have type defs");
        }
    }
}
