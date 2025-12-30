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

/// Tier usage statistics for JIT benchmarks
#[derive(Debug, Clone, Default)]
pub struct TierStats {
    pub tier2_attempts: u64,
    pub tier2_successes: u64,
    pub tier2_compile_unsupported: u64,
    pub tier2_runtime_unsupported: u64,
    pub tier2_runtime_error: u64,
    pub tier1_fallbacks: u64,
}

/// Result from tier stats line
#[derive(Debug, Clone)]
pub struct TierStatsResult {
    pub benchmark: String,
    pub target: String,
    pub operation: Operation,
    pub stats: TierStats,
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
    /// Tier usage stats: benchmark -> operation -> target -> stats
    pub tier_stats: HashMap<String, HashMap<Operation, HashMap<String, TierStats>>>,
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

/// Get the indentation level of a line (count of leading spaces/tree chars before content)
fn get_indent_level(line: &str) -> usize {
    let mut count = 0;
    for c in line.chars() {
        match c {
            ' ' | '│' => count += 1,
            '├' | '╰' => break,
            _ => break,
        }
    }
    count
}

/// Check if a line is a module header (├─ name or ╰─ name) at any indent level
/// Returns (indent_level, name) if it's a module header, None if it has data columns
fn is_module_header(line: &str) -> Option<(usize, &str)> {
    let indent = get_indent_level(line);

    // Find the tree character position
    let tree_pos = line.find('├').or_else(|| line.find('╰'))?;
    let after_tree = &line[tree_pos..];

    // Skip ├─ or ╰─
    let after_dash = after_tree
        .strip_prefix('├')
        .or_else(|| after_tree.strip_prefix('╰'))?
        .strip_prefix('─')?
        .trim_start();

    // Extract the name (word chars only)
    let name_end = after_dash
        .find(|c: char| !c.is_alphanumeric() && c != '_')
        .unwrap_or(after_dash.len());

    if name_end == 0 {
        return None;
    }

    let name = &after_dash[..name_end];

    // Check if there are numeric columns after the name (makes it a result row)
    let rest = &after_dash[name_end..];
    if rest.chars().any(|c| c.is_ascii_digit()) {
        // Has data columns - this is a result row, not a module header
        return None;
    }

    Some((indent, name))
}

/// Parse divan output text
pub fn parse_divan(text: &str) -> ParseResult<DivanResult> {
    let mut results = Vec::new();
    let mut failures = Vec::new();

    // Track nested module path: [(indent_level, name), ...]
    // This allows us to build paths like "json::simple_struct"
    let mut module_stack: Vec<(usize, String)> = Vec::new();

    for line in text.lines() {
        // Check for module header (nested or top-level)
        if let Some((indent, name)) = is_module_header(line) {
            // Pop modules that are at same or higher indent level
            while let Some((stack_indent, _)) = module_stack.last() {
                if *stack_indent >= indent {
                    module_stack.pop();
                } else {
                    break;
                }
            }
            module_stack.push((indent, name.to_string()));
            continue;
        }

        // Check for result row
        if let Some(target_full) = is_result_row(line) {
            // Build full benchmark path from module stack
            let benchmark = module_stack
                .iter()
                .map(|(_, name)| name.as_str())
                .collect::<Vec<_>>()
                .join("::");

            if benchmark.is_empty() {
                failures.push(format!(
                    "divan: result row with no benchmark context: {}",
                    line
                ));
                continue;
            }

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
                        benchmark,
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
    // Order matters: longer/more specific names first to avoid partial matches
    const KNOWN_TARGETS: &[&str] = &[
        // JSON format
        "facet_json_cranelift",
        "facet_json_t2",
        "facet_json_t1",
        "facet_json_t0",
        "facet_json",
        "serde_json",
        // Postcard format
        "facet_postcard_t2",
        "facet_postcard_t1",
        "facet_postcard_t0",
        "facet_postcard",
        "postcard",
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
        // New format: "unified_gungraun::json_short_keys_deser::gungraun_json_short_keys_serde_json_deserialize"
        // Old format: "unified_benchmarks_gungraun::simple_struct_deser::gungraun_simple_struct_facet_json_t1_deserialize"
        if line.contains("unified_gungraun::")
            || line.contains("unified_benchmarks_gungraun::")
            || line.contains("gungraun_jit::")
        {
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
                // name_without_op is like "json_short_keys_serde_json" (new) or "simple_struct_facet_json" (old)
                // We need to extract format, benchmark name, and target
                const KNOWN_FORMATS: &[&str] = &["json", "postcard", "msgpack", "yaml", "toml"];

                let mut found = false;
                for target in KNOWN_TARGETS {
                    if name_without_op.ends_with(target) {
                        let before_target = name_without_op
                            .strip_suffix(target)
                            .unwrap_or(name_without_op)
                            .trim_end_matches('_');

                        if !before_target.is_empty() {
                            // before_target is like "json_short_keys" (new) or "simple_struct" (old)
                            // Check if first segment is a known format
                            let benchmark = if let Some(idx) = before_target.find('_') {
                                let first_segment = &before_target[..idx];
                                if KNOWN_FORMATS.contains(&first_segment) {
                                    // New format: json_short_keys -> json::short_keys
                                    let name = &before_target[idx + 1..];
                                    format!("{}::{}", first_segment, name)
                                } else {
                                    // Old format: simple_struct stays as simple_struct
                                    before_target.to_string()
                                }
                            } else {
                                // Single word - treat whole thing as benchmark
                                before_target.to_string()
                            };

                            current = Some(GungraunParseState {
                                benchmark,
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

/// Parse tier stats from stderr lines
/// Format: [TIER_STATS] benchmark=<name> target=<target> operation=<op> tier2_attempts=<n> tier2_successes=<n> tier1_fallbacks=<n>
pub fn parse_tier_stats(text: &str) -> ParseResult<TierStatsResult> {
    let mut results = Vec::new();
    let mut failures = Vec::new();

    for line in text.lines() {
        let trimmed = line.trim();
        if !trimmed.starts_with("[TIER_STATS]") {
            continue;
        }

        // Parse key=value pairs
        let mut benchmark = None;
        let mut target = None;
        let mut operation = None;
        let mut tier2_attempts = None;
        let mut tier2_successes = None;
        let mut tier2_compile_unsupported = None;
        let mut tier2_runtime_unsupported = None;
        let mut tier2_runtime_error = None;
        let mut tier1_fallbacks = None;

        for part in trimmed.split_whitespace().skip(1) {
            // skip "[TIER_STATS]"
            if let Some((key, value)) = part.split_once('=') {
                match key {
                    "benchmark" => benchmark = Some(value.to_string()),
                    "target" => target = Some(value.to_string()),
                    "operation" => {
                        operation = match value {
                            "deserialize" => Some(Operation::Deserialize),
                            "serialize" => Some(Operation::Serialize),
                            _ => None,
                        }
                    }
                    "tier2_attempts" => tier2_attempts = value.parse().ok(),
                    "tier2_successes" => tier2_successes = value.parse().ok(),
                    "tier2_compile_unsupported" => tier2_compile_unsupported = value.parse().ok(),
                    "tier2_runtime_unsupported" => tier2_runtime_unsupported = value.parse().ok(),
                    "tier2_runtime_error" => tier2_runtime_error = value.parse().ok(),
                    "tier1_fallbacks" => tier1_fallbacks = value.parse().ok(),
                    _ => {}
                }
            }
        }

        // Validate we got all required fields
        if let (
            Some(benchmark),
            Some(target),
            Some(operation),
            Some(tier2_attempts),
            Some(tier2_successes),
            Some(tier1_fallbacks),
        ) = (
            benchmark,
            target,
            operation,
            tier2_attempts,
            tier2_successes,
            tier1_fallbacks,
        ) {
            results.push(TierStatsResult {
                benchmark,
                target,
                operation,
                stats: TierStats {
                    tier2_attempts,
                    tier2_successes,
                    tier2_compile_unsupported: tier2_compile_unsupported.unwrap_or(0),
                    tier2_runtime_unsupported: tier2_runtime_unsupported.unwrap_or(0),
                    tier2_runtime_error: tier2_runtime_error.unwrap_or(0),
                    tier1_fallbacks,
                },
            });
        } else {
            failures.push(format!("tier_stats: incomplete fields in line: {}", line));
        }
    }

    ParseResult { results, failures }
}

/// Combine divan, gungraun, and tier stats results into unified data structure
pub fn combine_results(
    divan: Vec<DivanResult>,
    gungraun: Vec<GungraunResult>,
    tier_stats: Vec<TierStatsResult>,
) -> BenchmarkData {
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

    // Process tier stats (3-level: benchmark -> operation -> target -> stats)
    for r in tier_stats {
        data.tier_stats
            .entry(r.benchmark)
            .or_default()
            .entry(r.operation)
            .or_default()
            .insert(r.target, r.stats);
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
    fn test_is_module_header() {
        // Top-level module headers (no indentation)
        assert_eq!(
            is_module_header("├─ booleans                             │"),
            Some((0, "booleans"))
        );
        assert_eq!(
            is_module_header("╰─ twitter                              │"),
            Some((0, "twitter"))
        );
        // Nested module header (indentation)
        assert_eq!(
            is_module_header("   ╰─ simple_struct                      │"),
            Some((3, "simple_struct"))
        );
        // Result row with data columns should NOT be a module header
        assert_eq!(
            is_module_header("│  ├─ facet_json_t1_deserialize  1.05 µs"),
            None
        );
    }

    #[test]
    fn test_is_result_row() {
        assert_eq!(
            is_result_row("│  ├─ facet_json_t1_deserialize      1.05 µs"),
            Some("facet_json_t1_deserialize")
        );
        assert_eq!(
            is_result_row("   ├─ facet_json_deserialize     2.674 ms"),
            Some("facet_json_deserialize")
        );
        assert_eq!(is_result_row("├─ booleans"), None);
    }

    #[test]
    fn test_parse_divan() {
        let input = r#"
Timer precision: 20 ns
unified_benchmarks_divan                          fastest       │ slowest       │ median        │ mean          │ samples │ iters
├─ booleans                                           │               │               │               │         │
│  ├─ facet_json_deserialize     304.4 µs      │ 400 µs        │ 310.4 µs      │ 311.1 µs      │ 100     │ 100
│  ├─ serde_json_deserialize            5.727 µs      │ 13.42 µs      │ 5.747 µs      │ 5.828 µs      │ 100     │ 100
│  ╰─ serde_json_serialize              1.181 µs      │ 3.264 µs      │ 1.191 µs      │ 1.221 µs      │ 100     │ 200
╰─ twitter                                            │               │               │               │         │
   ├─ facet_json_deserialize     2.674 ms      │ 2.877 ms      │ 2.718 ms      │ 2.723 ms      │ 100     │ 100
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
            .find(|r| r.benchmark == "booleans" && r.target == "facet_json")
            .expect("Should find facet_json result for booleans");
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
unified_benchmarks_gungraun::simple_struct::gungraun_simple_struct_facet_json_t1_deserialize cached:setup_t1()
  Instructions:                        6583|6583                 (No change)
  L1 Hits:                             9950|9950                 (No change)
  LL Hits:                               32|32                   (No change)
  RAM Hits:                               8|8                    (No change)
  Total read+write:                    9975|9975                 (No change)
  Estimated Cycles:                   10375|10375                (No change)
unified_benchmarks_gungraun::simple_struct::gungraun_simple_struct_facet_json_t0_deserialize
  Instructions:                       11811|11811                (No change)
"#;
        let parsed = parse_gungraun(input);
        assert!(
            parsed.failures.is_empty(),
            "Unexpected failures: {:?}",
            parsed.failures
        );
        assert_eq!(parsed.results.len(), 2);

        let t1_result = parsed
            .results
            .iter()
            .find(|r| r.target == "facet_json_t1")
            .expect("Should find facet_json_t1 result");

        assert_eq!(t1_result.benchmark, "simple_struct");
        assert_eq!(t1_result.operation, Operation::Deserialize);
        assert_eq!(t1_result.metrics.instructions, 6583);
        assert_eq!(t1_result.metrics.l1_hits, Some(9950));
        assert_eq!(t1_result.metrics.ll_hits, Some(32));
        assert_eq!(t1_result.metrics.ram_hits, Some(8));
        assert_eq!(t1_result.metrics.total_read_write, Some(9975));
        assert_eq!(t1_result.metrics.estimated_cycles, Some(10375));

        let t0_result = parsed
            .results
            .iter()
            .find(|r| r.target == "facet_json_t0")
            .expect("Should find facet_json_t0 result");
        assert_eq!(t0_result.metrics.instructions, 11811);
        // This result doesn't have other metrics in the input
        assert_eq!(t0_result.metrics.l1_hits, None);
    }

    #[test]
    fn test_parse_gungraun_serialize() {
        let input = r#"
unified_benchmarks_gungraun::canada_ser::gungraun_canada_serde_json_serialize cached:setup_serialize()
  Instructions:                      123456|123456                (No change)
  L1 Hits:                            98765|98765                 (No change)
unified_benchmarks_gungraun::citm_catalog_ser::gungraun_citm_catalog_facet_json_serialize cached:setup_serialize()
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
            .find(|r| r.benchmark == "citm_catalog" && r.target == "facet_json")
            .expect("Should find citm_catalog facet_json result");
        assert_eq!(citm_result.operation, Operation::Serialize);
        assert_eq!(citm_result.metrics.instructions, 54321);
    }

    #[test]
    fn test_parse_tier_stats() {
        let input = r#"
Some benchmark output...
[TIER_STATS] benchmark=booleans target=facet_json_t2 operation=deserialize tier2_attempts=1000 tier2_successes=1000 tier1_fallbacks=0
More output...
[TIER_STATS] benchmark=simple_struct target=facet_json_t2 operation=deserialize tier2_attempts=500 tier2_successes=450 tier1_fallbacks=50
[TIER_STATS] benchmark=canada target=facet_json_t2 operation=serialize tier2_attempts=100 tier2_successes=0 tier1_fallbacks=100
Random line without tier stats
"#;
        let parsed = parse_tier_stats(input);
        assert!(
            parsed.failures.is_empty(),
            "Unexpected failures: {:?}",
            parsed.failures
        );
        assert_eq!(parsed.results.len(), 3, "Should parse 3 tier stats lines");

        // Check booleans result
        let booleans = parsed
            .results
            .iter()
            .find(|r| r.benchmark == "booleans")
            .expect("Should find booleans tier stats");
        assert_eq!(booleans.target, "facet_json_t2");
        assert_eq!(booleans.operation, Operation::Deserialize);
        assert_eq!(booleans.stats.tier2_attempts, 1000);
        assert_eq!(booleans.stats.tier2_successes, 1000);
        assert_eq!(booleans.stats.tier1_fallbacks, 0);

        // Check simple_struct result
        let simple = parsed
            .results
            .iter()
            .find(|r| r.benchmark == "simple_struct")
            .expect("Should find simple_struct tier stats");
        assert_eq!(simple.stats.tier2_attempts, 500);
        assert_eq!(simple.stats.tier2_successes, 450);
        assert_eq!(simple.stats.tier1_fallbacks, 50);

        // Check canada serialize result
        let canada = parsed
            .results
            .iter()
            .find(|r| r.benchmark == "canada")
            .expect("Should find canada tier stats");
        assert_eq!(canada.operation, Operation::Serialize);
        assert_eq!(canada.stats.tier2_attempts, 100);
        assert_eq!(canada.stats.tier2_successes, 0);
        assert_eq!(canada.stats.tier1_fallbacks, 100);
    }
}
