//! Parse divan and gungraun benchmark output.

use regex::Regex;
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

/// Result from gungraun benchmark (instruction count)
#[derive(Debug, Clone)]
pub struct GungraunResult {
    pub benchmark: String,
    pub target: String,
    pub instructions: u64,
}

/// Combined benchmark data
#[derive(Debug, Default)]
pub struct BenchmarkData {
    /// Wall-clock results: benchmark -> operation -> target -> median_ns
    pub divan: HashMap<String, HashMap<Operation, HashMap<String, f64>>>,
    /// Instruction counts: (benchmark, target) -> instructions
    pub gungraun: HashMap<(String, String), u64>,
}

/// Parse divan output text
pub fn parse_divan(text: &str) -> Vec<DivanResult> {
    let mut results = Vec::new();
    let mut current_benchmark: Option<String> = None;

    // Match benchmark module names: "├─ booleans" (at start of line, may have more content after)
    // The benchmark name is followed by spaces and then column headers
    let module_re = Regex::new(r"^[├╰]─\s+(\w+)\s").unwrap();

    // Match result lines: "│  ├─ facet_format_jit_deserialize      1.05 µs"
    // We want the median column (3rd numeric column after target name)
    // Format: target  fastest  │  slowest  │  median  │  mean  │  samples  │  iters
    // Need to extract median which is the 3rd time value
    let result_re = Regex::new(
        r"^│\s*[├╰]─\s+([\w_]+)\s+([\d.]+)\s+(ns|µs|ms)\s+│\s+([\d.]+)\s+(ns|µs|ms)\s+│\s+([\d.]+)\s+(ns|µs|ms)"
    ).unwrap();

    for line in text.lines() {
        // Check for benchmark module (line starts with ├─ or ╰─)
        if let Some(caps) = module_re.captures(line) {
            current_benchmark = Some(caps[1].to_string());
            continue;
        }

        // Check for result line (line starts with │)
        if let Some(bench) = &current_benchmark
            && let Some(caps) = result_re.captures(line)
        {
            let target_full = &caps[1];
            // caps[2] and caps[3] are fastest
            // caps[4] and caps[5] are slowest
            // caps[6] and caps[7] are median (what we want)
            let value: f64 = caps[6].parse().unwrap_or(0.0);
            let unit = &caps[7];

            // Convert to nanoseconds
            let ns = match unit {
                "ns" => value,
                "µs" => value * 1_000.0,
                "ms" => value * 1_000_000.0,
                _ => value,
            };

            // Determine operation and strip suffix
            let (target, operation) = if target_full.ends_with("_deserialize") {
                (
                    target_full.trim_end_matches("_deserialize").to_string(),
                    Operation::Deserialize,
                )
            } else if target_full.ends_with("_serialize") {
                (
                    target_full.trim_end_matches("_serialize").to_string(),
                    Operation::Serialize,
                )
            } else {
                // Default to deserialize for targets without suffix
                (target_full.to_string(), Operation::Deserialize)
            };

            results.push(DivanResult {
                benchmark: bench.clone(),
                target,
                operation,
                median_ns: ns,
            });
        }
    }

    results
}

/// Parse gungraun output text
pub fn parse_gungraun(text: &str) -> Vec<GungraunResult> {
    let mut results = Vec::new();
    let mut current_benchmark: Option<String> = None;
    let mut current_target: Option<String> = None;

    // Match benchmark path: "gungraun_jit::jit_benchmarks::simple_struct_facet_format_jit"
    // or "unified_benchmarks_gungraun::simple_struct::facet_format_jit"
    let bench_re =
        Regex::new(r"(?:gungraun_jit|unified_benchmarks_gungraun)::[\w_]+::([\w_]+)").unwrap();

    // Match instructions: "  Instructions:  6583|6583"
    let instr_re = Regex::new(r"^\s+Instructions:\s+([\d,]+)").unwrap();

    for line in text.lines() {
        // Check for benchmark name
        if let Some(caps) = bench_re.captures(line) {
            let full_name = &caps[1];

            // Parse out benchmark and target from name like "simple_struct_facet_format_jit"
            // or "gungraun_simple_struct_facet_format_jit_deserialize"
            let name = full_name
                .trim_start_matches("gungraun_")
                .trim_end_matches("_deserialize");

            // Known targets to look for
            let known_targets = [
                "facet_format_jit",
                "facet_format_json",
                "facet_json",
                "facet_json_cranelift",
                "serde_json",
            ];

            // Find which target is in the name
            for target in &known_targets {
                if name.ends_with(target) {
                    let bench = name.trim_end_matches(target).trim_end_matches('_');
                    current_benchmark = Some(bench.to_string());
                    current_target = Some(target.to_string());
                    break;
                }
            }
            continue;
        }

        // Check for instructions
        if let (Some(bench), Some(target)) = (&current_benchmark, &current_target)
            && let Some(caps) = instr_re.captures(line)
        {
            let value_str = caps[1].replace(',', "");
            // Handle "value|baseline" format
            let value_str = value_str.split('|').next().unwrap_or(&value_str);
            if let Ok(instructions) = value_str.parse() {
                results.push(GungraunResult {
                    benchmark: bench.clone(),
                    target: target.clone(),
                    instructions,
                });
                current_benchmark = None;
                current_target = None;
            }
        }
    }

    results
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

    // Process gungraun results
    for r in gungraun {
        data.gungraun
            .insert((r.benchmark, r.target), r.instructions);
    }

    data
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_divan() {
        let input = r#"
Timer precision: 20 ns
vs_format_json                          fastest       │ slowest       │ median        │ mean          │ samples │ iters
├─ booleans                                           │               │               │               │         │
│  ├─ facet_format_json_deserialize     304.4 µs      │ 400 µs        │ 310.4 µs      │ 311.1 µs      │ 100     │ 100
│  ├─ serde_json_deserialize            5.727 µs      │ 13.42 µs      │ 5.747 µs      │ 5.828 µs      │ 100     │ 100
│  ╰─ serde_json_serialize              1.181 µs      │ 3.264 µs      │ 1.191 µs      │ 1.221 µs      │ 100     │ 200
├─ simple_struct                                      │               │               │               │         │
│  ├─ facet_format_jit_deserialize      1.05 µs       │ 549.2 µs      │ 1.05 µs       │ 6.574 µs      │ 100     │ 100
"#;
        let results = parse_divan(input);
        assert!(!results.is_empty(), "Should parse some results");

        // Find the facet_format_json result
        let facet_result = results
            .iter()
            .find(|r| r.benchmark == "booleans" && r.target == "facet_format_json")
            .expect("Should find facet_format_json result");

        assert_eq!(facet_result.operation, Operation::Deserialize);
        // 310.4 µs = 310400 ns
        assert!((facet_result.median_ns - 310_400.0).abs() < 1.0);
    }

    #[test]
    fn test_parse_gungraun() {
        let input = r#"
gungraun_jit::jit_benchmarks::simple_struct_facet_format_jit cached:setup_simple_jit()
  Instructions:                        6583|6583                 (No change)
  L1 Hits:                             9950|9950                 (No change)
gungraun_jit::jit_benchmarks::simple_struct_facet_format_json
  Instructions:                       11811|11811                (No change)
"#;
        let results = parse_gungraun(input);
        assert!(!results.is_empty());

        let jit_result = results
            .iter()
            .find(|r| r.target == "facet_format_jit")
            .expect("Should find facet_format_jit result");

        assert_eq!(jit_result.benchmark, "simple_struct");
        assert_eq!(jit_result.instructions, 6583);
    }
}
