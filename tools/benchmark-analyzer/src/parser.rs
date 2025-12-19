//! Parse divan and gungraun benchmark output.

use std::collections::HashMap;

/// Operation type: deserialize or serialize
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Operation {
    Deserialize,
    Serialize,
}

impl std::fmt::Display for Operation {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Operation::Deserialize => write!(f, "Deserialize"),
            Operation::Serialize => write!(f, "Serialize"),
        }
    }
}

/// Result from divan benchmark (wall-clock timing)
#[derive(Debug, Clone)]
pub struct DivanResult {
    pub benchmark: String,
    pub target: String,
    pub operation: Operation,
    pub median_ns: f64,
}

/// Parse result with success/failure tracking
#[derive(Debug)]
pub struct ParseResult<T> {
    pub results: Vec<T>,
    pub failures: Vec<String>,
}

/// All metrics from a gungraun benchmark run
#[derive(Debug, Clone, Default)]
pub struct GungraunMetrics {
    pub instructions: u64,
    pub l1_hits: Option<u64>,
    pub ll_hits: Option<u64>,
    pub ram_hits: Option<u64>,
    pub total_read_write: Option<u64>,
    pub estimated_cycles: Option<u64>,
}

/// Result from gungraun benchmark (instruction count + cache metrics)
#[derive(Debug, Clone)]
pub struct GungraunResult {
    pub benchmark: String,
    pub target: String,
    pub operation: Operation,
    pub metrics: GungraunMetrics,
}

/// Combined benchmark data
#[derive(Debug, Default)]
pub struct BenchmarkData {
    /// Wall-clock results: benchmark -> operation -> target -> median_ns
    pub divan: HashMap<String, HashMap<Operation, HashMap<String, f64>>>,
    /// Instruction counts + cache metrics: benchmark -> operation -> target -> metrics
    pub gungraun: HashMap<String, HashMap<Operation, HashMap<String, GungraunMetrics>>>,
}

/// Parse a time value with unit into nanoseconds
fn parse_time(s: &str) -> Option<f64> {
    let s = s.trim();
    if s.is_empty() {
        return None;
    }

    // Find where the number ends and unit begins
    let mut num_end = 0;
    for (i, c) in s.char_indices() {
        if c.is_ascii_digit() || c == '.' {
            num_end = i + c.len_utf8();
        } else if !c.is_whitespace() {
            break;
        }
    }

    let num_str = s[..num_end].trim();
    let unit_str = s[num_end..].trim();

    let value: f64 = num_str.parse().ok()?;

    let multiplier = match unit_str {
        "ns" => 1.0,
        "µs" | "us" => 1_000.0,
        "ms" => 1_000_000.0,
        "s" => 1_000_000_000.0,
        _ => return None,
    };

    Some(value * multiplier)
}

/// Check if a line starts a benchmark group (├─ name or ╰─ name at column 0)
fn is_benchmark_header(line: &str) -> Option<&str> {
    let trimmed = line.trim_start();
    if !line.starts_with('├') && !line.starts_with('╰') {
        return None;
    }

    // Skip the tree char and dash
    let after_tree = trimmed
        .strip_prefix('├')
        .or_else(|| trimmed.strip_prefix('╰'))?;
    let after_dash = after_tree.strip_prefix('─')?.trim_start();

    // Extract the benchmark name (word chars only, stop at whitespace)
    let name_end = after_dash
        .find(|c: char| !c.is_alphanumeric() && c != '_')
        .unwrap_or(after_dash.len());

    if name_end == 0 {
        return None;
    }

    Some(&after_dash[..name_end])
}

/// Check if a line is a result row (indented with │ or spaces, then ├─ or ╰─)
fn is_result_row(line: &str) -> Option<&str> {
    // Must start with │ or space (indentation)
    if !line.starts_with('│') && !line.starts_with(' ') {
        return None;
    }

    // Find the tree character after the indentation
    let tree_pos = line.find('├').or_else(|| line.find('╰'))?;

    // Check that before the tree char is only │ and spaces
    let prefix = &line[..tree_pos];
    if !prefix.chars().all(|c| c == '│' || c.is_whitespace()) {
        return None;
    }

    // Extract target name after ├─ or ╰─
    let after_tree = &line[tree_pos..];
    let after_dash = after_tree
        .strip_prefix('├')
        .or_else(|| after_tree.strip_prefix('╰'))?
        .strip_prefix('─')?
        .trim_start();

    // Extract the target name (word chars only)
    let name_end = after_dash
        .find(|c: char| !c.is_alphanumeric() && c != '_')
        .unwrap_or(after_dash.len());

    if name_end == 0 {
        return None;
    }

    Some(&after_dash[..name_end])
}

/// Extract columns from a result line by splitting on │
fn extract_columns(line: &str) -> Vec<&str> {
    // Find the target name first to know where data columns start
    let tree_pos = line.find('├').or_else(|| line.find('╰'));
    if tree_pos.is_none() {
        return vec![];
    }

    // Split the rest of the line by │
    let after_tree = &line[tree_pos.unwrap()..];

    // Skip past ├─ or ╰─ and the target name to get to the data
    let data_start = after_tree
        .find(|c: char| c.is_ascii_digit())
        .unwrap_or(after_tree.len());

    let data_part = &after_tree[data_start..];

    // Now split by │ to get columns
    // The format is: fastest │ slowest │ median │ mean │ samples │ iters
    // But the first value (fastest) comes before the first │
    let mut columns = vec![];

    // Get the first column (before first │)
    let first_sep = data_part.find('│').unwrap_or(data_part.len());
    columns.push(data_part[..first_sep].trim());

    // Get remaining columns
    let mut rest = &data_part[first_sep..];
    while let Some(sep_pos) = rest.find('│') {
        rest = &rest[sep_pos + '│'.len_utf8()..];
        let next_sep = rest.find('│').unwrap_or(rest.len());
        columns.push(rest[..next_sep].trim());
    }

    columns
}

/// Parse divan output text
pub fn parse_divan(text: &str) -> ParseResult<DivanResult> {
    let mut results = Vec::new();
    let mut failures = Vec::new();
    let mut current_benchmark: Option<String> = None;

    for line in text.lines() {
        // Check for benchmark header
        if let Some(name) = is_benchmark_header(line) {
            current_benchmark = Some(name.to_string());
            continue;
        }

        // Check for result row
        if let Some(bench) = &current_benchmark
            && let Some(target_full) = is_result_row(line)
        {
            let columns = extract_columns(line);

            // We need at least 3 columns: fastest, slowest, median
            if columns.len() >= 3 {
                // Median is the 3rd column (index 2)
                if let Some(median_ns) = parse_time(columns[2]) {
                    // Determine operation and strip suffix
                    let (target, operation) = if target_full.ends_with("_deserialize") {
                        (
                            target_full
                                .strip_suffix("_deserialize")
                                .unwrap()
                                .to_string(),
                            Operation::Deserialize,
                        )
                    } else if target_full.ends_with("_serialize") {
                        (
                            target_full.strip_suffix("_serialize").unwrap().to_string(),
                            Operation::Serialize,
                        )
                    } else {
                        (target_full.to_string(), Operation::Deserialize)
                    };

                    results.push(DivanResult {
                        benchmark: bench.clone(),
                        target,
                        operation,
                        median_ns,
                    });
                } else {
                    // Looked like a result row but couldn't parse time
                    failures.push(format!(
                        "divan: couldn't parse time from column '{}' in line: {}",
                        columns[2], line
                    ));
                }
            } else {
                // Looked like a result row but not enough columns
                failures.push(format!(
                    "divan: expected ≥3 columns, got {} in line: {}",
                    columns.len(),
                    line
                ));
            }
        }
    }

    ParseResult { results, failures }
}

/// State for parsing a single gungraun benchmark
struct GungraunParseState {
    benchmark: String,
    target: String,
    operation: Operation,
    header_line: String,
    metrics: GungraunMetrics,
}

/// Parse a gungraun metric value from a line like "  Instructions:  6589|6589  (No change)"
fn parse_gungraun_metric(line: &str, label: &str) -> Option<u64> {
    let trimmed = line.trim();
    if !trimmed.starts_with(label) {
        return None;
    }

    let after_label = trimmed.strip_prefix(label)?.strip_prefix(':')?.trim();

    // Handle "value|baseline" format and commas
    let value_str = after_label
        .split('|')
        .next()
        .unwrap_or(after_label)
        .replace(',', "");

    // Parse just the numeric part
    let num_str: String = value_str
        .chars()
        .take_while(|c| c.is_ascii_digit())
        .collect();

    num_str.parse().ok()
}

/// Parse gungraun output text
pub fn parse_gungraun(text: &str) -> ParseResult<GungraunResult> {
    let mut results = Vec::new();
    let mut failures = Vec::new();
    let mut current: Option<GungraunParseState> = None;

    // Known targets to look for - must match benchmark function names
    const KNOWN_TARGETS: &[&str] = &[
        "serde_json",
        "facet_format_json",
        "facet_format_jit_t1",
        "facet_format_jit_t2",
    ];

    // Helper to finalize current benchmark
    let finalize_current = |current: &mut Option<GungraunParseState>,
                            results: &mut Vec<GungraunResult>,
                            failures: &mut Vec<String>| {
        if let Some(state) = current.take() {
            if state.metrics.instructions > 0 {
                results.push(GungraunResult {
                    benchmark: state.benchmark,
                    target: state.target,
                    operation: state.operation,
                    metrics: state.metrics,
                });
            } else {
                failures.push(format!(
                    "gungraun: no Instructions line found for {}/{} (header: {})",
                    state.benchmark, state.target, state.header_line
                ));
            }
        }
    };

    for line in text.lines() {
        // Check for benchmark path like:
        // "unified_benchmarks_gungraun::simple_struct_deser::gungraun_simple_struct_facet_format_jit_deserialize"
        if line.contains("unified_benchmarks_gungraun::") || line.contains("gungraun_jit::") {
            // Finalize any previous benchmark
            finalize_current(&mut current, &mut results, &mut failures);

            // Extract the function name (last part after ::)
            if let Some(last_part) = line.split("::").last() {
                // Remove trailing stuff like " cached:setup_jit()"
                let func_name = last_part.split_whitespace().next().unwrap_or(last_part);

                // Strip "gungraun_" prefix
                let after_prefix = func_name.strip_prefix("gungraun_").unwrap_or(func_name);

                // Determine operation from suffix
                let (name_without_op, operation) = if after_prefix.ends_with("_deserialize") {
                    (
                        after_prefix.strip_suffix("_deserialize").unwrap(),
                        Operation::Deserialize,
                    )
                } else if after_prefix.ends_with("_serialize") {
                    (
                        after_prefix.strip_suffix("_serialize").unwrap(),
                        Operation::Serialize,
                    )
                } else {
                    // Default to deserialize for old-style benchmarks without suffix
                    (after_prefix, Operation::Deserialize)
                };

                // Find which target is in the name
                let mut found = false;
                for target in KNOWN_TARGETS {
                    if name_without_op.ends_with(target) {
                        let bench = name_without_op
                            .strip_suffix(target)
                            .unwrap_or(name_without_op)
                            .trim_end_matches('_');
                        if !bench.is_empty() {
                            current = Some(GungraunParseState {
                                benchmark: bench.to_string(),
                                target: target.to_string(),
                                operation,
                                header_line: line.to_string(),
                                metrics: GungraunMetrics::default(),
                            });
                            found = true;
                            break;
                        }
                    }
                }
                if !found {
                    failures.push(format!(
                        "gungraun: couldn't extract benchmark/target from line: {}",
                        line
                    ));
                }
            }
            continue;
        }

        // Parse metric lines if we're in a benchmark
        if let Some(ref mut state) = current {
            let trimmed = line.trim();

            if let Some(v) = parse_gungraun_metric(trimmed, "Instructions") {
                state.metrics.instructions = v;
            } else if let Some(v) = parse_gungraun_metric(trimmed, "L1 Hits") {
                state.metrics.l1_hits = Some(v);
            } else if let Some(v) = parse_gungraun_metric(trimmed, "LL Hits") {
                state.metrics.ll_hits = Some(v);
            } else if let Some(v) = parse_gungraun_metric(trimmed, "RAM Hits") {
                state.metrics.ram_hits = Some(v);
            } else if let Some(v) = parse_gungraun_metric(trimmed, "Total read+write") {
                state.metrics.total_read_write = Some(v);
            } else if let Some(v) = parse_gungraun_metric(trimmed, "Estimated Cycles") {
                state.metrics.estimated_cycles = Some(v);
            }
        }
    }

    // Finalize trailing benchmark
    finalize_current(&mut current, &mut results, &mut failures);

    ParseResult { results, failures }
}

/// Combine divan and gungraun results into unified data structure
pub fn combine_results(divan: Vec<DivanResult>, gungraun: Vec<GungraunResult>) -> BenchmarkData {
    let mut data = BenchmarkData::default();

    // Process divan results
    for r in divan {
        data.divan
            .entry(r.benchmark)
            .or_default()
            .entry(r.operation)
            .or_default()
            .insert(r.target, r.median_ns);
    }

    // Process gungraun results (3-level: benchmark -> operation -> target -> metrics)
    for r in gungraun {
        data.gungraun
            .entry(r.benchmark)
            .or_default()
            .entry(r.operation)
            .or_default()
            .insert(r.target, r.metrics);
    }

    data
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_time() {
        assert!((parse_time("1.05 µs").unwrap() - 1050.0).abs() < 0.1);
        assert!((parse_time("310.4 µs").unwrap() - 310_400.0).abs() < 0.1);
        assert!((parse_time("57.94 ms").unwrap() - 57_940_000.0).abs() < 0.1);
        assert!((parse_time("20 ns").unwrap() - 20.0).abs() < 0.1);
    }

    #[test]
    fn test_is_benchmark_header() {
        assert_eq!(
            is_benchmark_header("├─ booleans                             │"),
            Some("booleans")
        );
        assert_eq!(
            is_benchmark_header("╰─ twitter                              │"),
            Some("twitter")
        );
        assert_eq!(
            is_benchmark_header("│  ├─ facet_format_jit_deserialize"),
            None
        );
    }

    #[test]
    fn test_is_result_row() {
        assert_eq!(
            is_result_row("│  ├─ facet_format_jit_deserialize      1.05 µs"),
            Some("facet_format_jit_deserialize")
        );
        assert_eq!(
            is_result_row("   ├─ facet_format_json_deserialize     2.674 ms"),
            Some("facet_format_json_deserialize")
        );
        assert_eq!(is_result_row("├─ booleans"), None);
    }

    #[test]
    fn test_parse_divan() {
        let input = r#"
Timer precision: 20 ns
unified_benchmarks_divan                          fastest       │ slowest       │ median        │ mean          │ samples │ iters
├─ booleans                                           │               │               │               │         │
│  ├─ facet_format_json_deserialize     304.4 µs      │ 400 µs        │ 310.4 µs      │ 311.1 µs      │ 100     │ 100
│  ├─ serde_json_deserialize            5.727 µs      │ 13.42 µs      │ 5.747 µs      │ 5.828 µs      │ 100     │ 100
│  ╰─ serde_json_serialize              1.181 µs      │ 3.264 µs      │ 1.191 µs      │ 1.221 µs      │ 100     │ 200
╰─ twitter                                            │               │               │               │         │
   ├─ facet_format_json_deserialize     2.674 ms      │ 2.877 ms      │ 2.718 ms      │ 2.723 ms      │ 100     │ 100
   ╰─ serde_json_deserialize            394.5 µs      │ 500.1 µs      │ 399.4 µs      │ 404.6 µs      │ 100     │ 100
"#;
        let parsed = parse_divan(input);
        assert!(
            parsed.failures.is_empty(),
            "Unexpected failures: {:?}",
            parsed.failures
        );

        // Check booleans results
        let facet_result = parsed
            .results
            .iter()
            .find(|r| r.benchmark == "booleans" && r.target == "facet_format_json")
            .expect("Should find facet_format_json result for booleans");
        assert_eq!(facet_result.operation, Operation::Deserialize);
        assert!((facet_result.median_ns - 310_400.0).abs() < 1.0);

        // Check twitter results (the last benchmark with different indentation)
        let twitter_result = parsed
            .results
            .iter()
            .find(|r| r.benchmark == "twitter" && r.target == "serde_json")
            .expect("Should find serde_json result for twitter");
        assert_eq!(twitter_result.operation, Operation::Deserialize);
        assert!((twitter_result.median_ns - 399_400.0).abs() < 100.0);
    }

    #[test]
    fn test_parse_gungraun() {
        let input = r#"
unified_benchmarks_gungraun::simple_struct::gungraun_simple_struct_facet_format_jit_deserialize cached:setup_jit()
  Instructions:                        6583|6583                 (No change)
  L1 Hits:                             9950|9950                 (No change)
  LL Hits:                               32|32                   (No change)
  RAM Hits:                               8|8                    (No change)
  Total read+write:                    9975|9975                 (No change)
  Estimated Cycles:                   10375|10375                (No change)
unified_benchmarks_gungraun::simple_struct::gungraun_simple_struct_facet_format_json_deserialize
  Instructions:                       11811|11811                (No change)
"#;
        let parsed = parse_gungraun(input);
        assert!(
            parsed.failures.is_empty(),
            "Unexpected failures: {:?}",
            parsed.failures
        );
        assert_eq!(parsed.results.len(), 2);

        let jit_result = parsed
            .results
            .iter()
            .find(|r| r.target == "facet_format_jit")
            .expect("Should find facet_format_jit result");

        assert_eq!(jit_result.benchmark, "simple_struct");
        assert_eq!(jit_result.operation, Operation::Deserialize);
        assert_eq!(jit_result.metrics.instructions, 6583);
        assert_eq!(jit_result.metrics.l1_hits, Some(9950));
        assert_eq!(jit_result.metrics.ll_hits, Some(32));
        assert_eq!(jit_result.metrics.ram_hits, Some(8));
        assert_eq!(jit_result.metrics.total_read_write, Some(9975));
        assert_eq!(jit_result.metrics.estimated_cycles, Some(10375));

        let json_result = parsed
            .results
            .iter()
            .find(|r| r.target == "facet_format_json")
            .expect("Should find facet_format_json result");
        assert_eq!(json_result.metrics.instructions, 11811);
        // This result doesn't have other metrics in the input
        assert_eq!(json_result.metrics.l1_hits, None);
    }

    #[test]
    fn test_parse_gungraun_serialize() {
        let input = r#"
unified_benchmarks_gungraun::canada_ser::gungraun_canada_serde_json_serialize cached:setup_serialize()
  Instructions:                      123456|123456                (No change)
  L1 Hits:                            98765|98765                 (No change)
unified_benchmarks_gungraun::citm_catalog_ser::gungraun_citm_catalog_facet_format_json_serialize cached:setup_serialize()
  Instructions:                       54321|54321                 (No change)
"#;
        let parsed = parse_gungraun(input);
        assert!(
            parsed.failures.is_empty(),
            "Unexpected failures: {:?}",
            parsed.failures
        );
        assert_eq!(parsed.results.len(), 2, "Should parse 2 serialize results");

        let canada_result = parsed
            .results
            .iter()
            .find(|r| r.benchmark == "canada" && r.target == "serde_json")
            .expect("Should find canada serde_json result");
        assert_eq!(canada_result.operation, Operation::Serialize);
        assert_eq!(canada_result.metrics.instructions, 123456);
        assert_eq!(canada_result.metrics.l1_hits, Some(98765));

        let citm_result = parsed
            .results
            .iter()
            .find(|r| r.benchmark == "citm_catalog" && r.target == "facet_format_json")
            .expect("Should find citm_catalog facet_format_json result");
        assert_eq!(citm_result.operation, Operation::Serialize);
        assert_eq!(citm_result.metrics.instructions, 54321);
    }
}
