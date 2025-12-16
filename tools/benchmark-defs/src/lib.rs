//! Shared benchmark definitions for benchmark-generator and benchmark-analyzer.

use facet::Facet;
use facet_args as args;
use facet_kdl as kdl; // Make kdl:: paths work in attributes
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
}

#[derive(Debug, Facet)]
pub struct BenchmarkFile {
    #[facet(kdl::children, default)]
    pub benchmarks: Vec<BenchmarkDef>,
    #[facet(kdl::children, default)]
    pub type_defs: Vec<TypeDef>,
}

#[derive(Debug, Facet, Clone)]
pub struct BenchmarkDef {
    #[facet(kdl::property)]
    pub name: String,
    #[facet(kdl::property, rename = "type")]
    pub type_name: String,
    #[facet(kdl::property)]
    pub category: String,
    #[facet(kdl::child, default)]
    pub json: Option<JsonData>,
    #[facet(kdl::child, default)]
    pub json_file: Option<JsonFile>,
    #[facet(kdl::child, default)]
    pub json_brotli: Option<JsonBrotli>,
    #[facet(kdl::child, default)]
    pub generated: Option<Generated>,
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

/// Parse benchmark definitions from a KDL file.
pub fn parse_benchmarks(kdl_path: &Path) -> Result<BenchmarkFile, Box<dyn std::error::Error>> {
    let content = std::fs::read_to_string(kdl_path)?;
    let file: BenchmarkFile = facet_kdl::from_str(&content)?;
    Ok(file)
}

/// Load benchmark categories from benchmarks.kdl, returning a map of name -> category.
pub fn load_categories(workspace_root: &Path) -> HashMap<String, String> {
    let kdl_path = workspace_root.join("facet-json/benches/benchmarks.kdl");
    match parse_benchmarks(&kdl_path) {
        Ok(file) => file
            .benchmarks
            .into_iter()
            .map(|b| (b.name, b.category))
            .collect(),
        Err(_) => HashMap::new(),
    }
}
