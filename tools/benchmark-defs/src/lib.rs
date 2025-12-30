//! Shared benchmark definitions for benchmark-generator and benchmark-analyzer.
//!
//! Supports multiple serialization formats (JSON, Postcard, MessagePack, etc.)
//! Each format has its own KDL file defining benchmarks and types.

use facet::Facet;
use facet_args as args;
use facet_kdl as kdl;
use std::collections::HashMap;
use std::path::Path;

/// Arguments for the bench command, shared between xtask and benchmark-analyzer.
#[derive(Facet, Debug, Default)]
pub struct BenchReportArgs {
    /// Filter to run only specific benchmark(s), e.g., "booleans"
    #[facet(args::positional, default)]
    pub filter: Option<String>,

    /// Start HTTP server to view the report after generation
    #[facet(args::named)]
    pub serve: bool,

    /// Skip running benchmarks, reuse previous benchmark data
    #[facet(args::named)]
    pub no_run: bool,

    /// Skip cloning perf.facet.rs and generating index (just export raw data)
    #[facet(args::named)]
    pub no_index: bool,

    /// Push results to perf.facet.rs (refuses if filter is set)
    #[facet(args::named)]
    pub push: bool,
}

/// A complete benchmark suite file (one per format).
#[derive(Debug, Facet)]
pub struct BenchmarkFile {
    /// Format configuration (required, first child)
    #[facet(kdl::child)]
    pub format: FormatConfig,
    /// Benchmark definitions
    #[facet(kdl::children, default)]
    pub benchmarks: Vec<BenchmarkDef>,
    /// Type definitions used by benchmarks
    #[facet(kdl::children, default)]
    pub type_defs: Vec<TypeDef>,
}

/// Format-specific configuration.
#[derive(Debug, Facet, Clone)]
pub struct FormatConfig {
    /// Format identifier (e.g., "json", "postcard", "msgpack")
    #[facet(kdl::child)]
    pub name: FormatName,
    /// Baseline implementation crate (e.g., "serde_json", "postcard")
    #[facet(kdl::child)]
    pub baseline: BaselineCrate,
    /// Facet implementation crate (e.g., "facet_json", "facet_postcard")
    #[facet(kdl::child)]
    pub facet_crate: FacetCrate,
    /// Which JIT tiers to benchmark (default: `[1, 2]` for text formats, `[2]` for binary formats)
    /// T1 = shape-based JIT (works for text formats with structural tokens)
    /// T2 = format-specific JIT (works for all formats with FormatJitParser)
    #[facet(kdl::child, default)]
    pub jit_tiers: Option<JitTiers>,
}

#[derive(Debug, Facet, Clone)]
pub struct JitTiers {
    #[facet(kdl::arguments)]
    pub tiers: Vec<u8>,
}

impl FormatConfig {
    /// Get the JIT tiers to benchmark. Defaults to [1, 2] if not specified.
    pub fn jit_tiers(&self) -> Vec<u8> {
        self.jit_tiers
            .as_ref()
            .map(|t| t.tiers.clone())
            .unwrap_or_else(|| vec![1, 2])
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

#[derive(Debug, Facet, Clone)]
pub struct FormatName {
    #[facet(kdl::argument)]
    pub value: String,
}

#[derive(Debug, Facet, Clone)]
pub struct BaselineCrate {
    #[facet(kdl::argument)]
    pub value: String,
}

#[derive(Debug, Facet, Clone)]
pub struct FacetCrate {
    #[facet(kdl::argument)]
    pub value: String,
}

/// A single benchmark definition.
#[derive(Debug, Facet, Clone)]
pub struct BenchmarkDef {
    #[facet(kdl::property)]
    pub name: String,
    #[facet(kdl::property, rename = "type")]
    pub type_name: String,
    #[facet(kdl::property)]
    pub category: String,
    /// Inline JSON payload
    #[facet(kdl::child, default)]
    pub json: Option<JsonData>,
    /// JSON from file path
    #[facet(kdl::child, default)]
    pub json_file: Option<JsonFile>,
    /// Brotli-compressed JSON from file path
    #[facet(kdl::child, default)]
    pub json_brotli: Option<JsonBrotli>,
    /// Generated payload (format-agnostic generator name)
    #[facet(kdl::child, default)]
    pub generated: Option<Generated>,
    /// Inline binary payload (hex-encoded)
    #[facet(kdl::child, default)]
    pub binary_hex: Option<BinaryHex>,
    /// Inline binary payload (base64-encoded)
    #[facet(kdl::child, default)]
    pub binary_base64: Option<BinaryBase64>,
}

#[derive(Debug, Facet, Clone)]
pub struct JsonData {
    #[facet(kdl::argument)]
    pub content: String,
}

#[derive(Debug, Facet, Clone)]
pub struct JsonFile {
    #[facet(kdl::argument)]
    pub path: String,
}

#[derive(Debug, Facet, Clone)]
pub struct JsonBrotli {
    #[facet(kdl::argument)]
    pub path: String,
}

#[derive(Debug, Facet, Clone)]
pub struct Generated {
    #[facet(kdl::argument)]
    pub generator_name: String,
}

#[derive(Debug, Facet, Clone)]
pub struct BinaryHex {
    #[facet(kdl::argument)]
    pub content: String,
}

#[derive(Debug, Facet, Clone)]
pub struct BinaryBase64 {
    #[facet(kdl::argument)]
    pub content: String,
}

/// A type definition used in benchmarks.
#[derive(Debug, Facet, Clone)]
pub struct TypeDef {
    #[facet(kdl::property)]
    pub name: String,
    #[facet(kdl::child)]
    pub code: CodeBlock,
}

#[derive(Debug, Facet, Clone)]
pub struct CodeBlock {
    #[facet(kdl::argument)]
    pub content: String,
}

/// Parse a single benchmark file from KDL.
pub fn parse_benchmarks(kdl_path: &Path) -> Result<BenchmarkFile, Box<dyn std::error::Error>> {
    let content = std::fs::read_to_string(kdl_path)?;
    let file: BenchmarkFile = facet_kdl::from_str(&content)?;
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
        if path.extension().is_some_and(|ext| ext == "kdl") {
            match parse_benchmarks(&path) {
                Ok(file) => {
                    let format_name = file.format.name.value.clone();
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

/// Load benchmark categories from all KDL files.
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
    let baseline = format.baseline.value.clone();
    let facet = &format.facet_crate.value;
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
format {
    name "json"
    baseline "serde_json"
    facet_crate "facet_json"
}

benchmark name="test" type="String" category="micro" {
    json "hello"
}

type_def name="Foo" {
    code "struct Foo { x: u64 }"
}
"#;

        let file: BenchmarkFile = facet_kdl::from_str(content).expect("should parse");
        assert_eq!(file.format.name.value, "json");
        assert_eq!(file.format.baseline.value, "serde_json");
        assert_eq!(file.format.facet_crate.value, "facet_json");
        assert_eq!(file.benchmarks.len(), 1);
        assert_eq!(file.benchmarks[0].name, "test");
        assert_eq!(file.type_defs.len(), 1);
    }

    #[test]
    fn test_format_targets() {
        let format = FormatConfig {
            name: FormatName {
                value: "json".to_string(),
            },
            baseline: BaselineCrate {
                value: "serde_json".to_string(),
            },
            facet_crate: FacetCrate {
                value: "facet_json".to_string(),
            },
            jit_tiers: None,
        };

        let (baseline, t0, t1, t2) = format_targets(&format);
        assert_eq!(baseline, "serde_json");
        assert_eq!(t0, "facet_json_t0");
        assert_eq!(t1, "facet_json_t1");
        assert_eq!(t2, "facet_json_t2");
    }

    #[test]
    fn test_parse_real_json_kdl() {
        // Test parsing the actual json.kdl file
        let workspace_root = std::env::current_dir().unwrap();
        let kdl_path = workspace_root.join("facet-perf-shootout/benches/json.kdl");

        if kdl_path.exists() {
            let file = parse_benchmarks(&kdl_path).expect("should parse json.kdl");
            assert_eq!(file.format.name.value, "json");
            assert_eq!(file.format.baseline.value, "serde_json");
            assert_eq!(file.format.facet_crate.value, "facet_json");
            assert!(!file.benchmarks.is_empty(), "should have benchmarks");
            assert!(!file.type_defs.is_empty(), "should have type defs");
        }
    }
}
