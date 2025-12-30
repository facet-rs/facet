//! Benchmark analyzer: run benchmarks, parse output, generate HTML reports.

mod parser;
mod perf_index;
mod report;
mod run_types;
mod server;

use chrono::Local;
use console::Term;
use facet_args as args;
use miette::Report;
use owo_colors::OwoColorize;
use std::collections::VecDeque;
use std::fs;
use std::io::{BufRead, BufReader};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::mpsc;
use std::time::Duration;

/// Format a URL as a clickable terminal hyperlink using OSC 8 escape sequences.
/// Falls back to plain URL if the terminal doesn't support hyperlinks.
pub fn hyperlink(url: &str) -> String {
    format!("\x1b]8;;{url}\x07{url}\x1b]8;;\x07")
}

/// Format a file path as a clickable terminal hyperlink.
fn file_hyperlink(path: &Path) -> String {
    let url = format!("file://{}", path.display());
    let text = path.display().to_string();
    format!("\x1b]8;;{url}\x07{text}\x1b]8;;\x07")
}

use benchmark_defs::BenchReportArgs as Args;

/// Export performance data as JSON for delta tracking
/// Includes all targets so ratios can be computed on the frontend
fn export_perf_json(data: &parser::BenchmarkData, report_dir: &Path, timestamp: &str) {
    use std::collections::BTreeMap;

    // We want a structure like:
    // {
    //   "timestamp": "...",
    //   "benchmarks": {
    //     "simple_struct": { "facet_json_t2": 6567, "serde_json": 8000, ... },
    //     ...
    //   }
    // }

    let mut json_data = BTreeMap::new();
    json_data.insert("timestamp".to_string(), timestamp.to_string());

    let mut benchmarks: BTreeMap<String, BTreeMap<String, u64>> = BTreeMap::new();

    // Extract instruction counts from gungraun data - ALL targets
    // Structure: benchmark -> operation -> target -> metrics
    // For legacy perf-data.json, we export only deserialize operation instructions
    for (benchmark, ops) in &data.gungraun {
        for (operation, targets) in ops {
            // Only export deserialize for legacy format compatibility
            if *operation != parser::Operation::Deserialize {
                continue;
            }
            for (target, metrics) in targets {
                benchmarks
                    .entry(benchmark.clone())
                    .or_default()
                    .insert(target.clone(), metrics.instructions);
            }
        }
    }

    // Build JSON manually (avoiding serde dependency per project guidelines)
    let mut json = String::from("{\n");
    json.push_str(&format!("  \"timestamp\": \"{}\",\n", timestamp));
    json.push_str("  \"benchmarks\": {\n");

    let benchmark_count = benchmarks.len();
    for (idx, (benchmark, targets)) in benchmarks.iter().enumerate() {
        json.push_str(&format!("    \"{}\": {{\n", benchmark));

        let target_count = targets.len();
        for (tidx, (target, instructions)) in targets.iter().enumerate() {
            json.push_str(&format!("      \"{}\": {}", target, instructions));
            if tidx < target_count - 1 {
                json.push(',');
            }
            json.push('\n');
        }

        json.push_str("    }");
        if idx < benchmark_count - 1 {
            json.push(',');
        }
        json.push('\n');
    }

    json.push_str("  }\n");
    json.push_str("}\n");

    // Write to file
    let perf_json_file = report_dir.join(format!("perf-data-{}.json", timestamp));
    fs::write(&perf_json_file, &json).expect("Failed to write perf-data JSON");

    // Also write a "latest" symlink/copy for easy access
    let latest_perf_json = report_dir.join("perf-data.json");
    let _ = fs::remove_file(&latest_perf_json);
    #[cfg(unix)]
    {
        use std::os::unix::fs::symlink;
        let _ = symlink(format!("perf-data-{}.json", timestamp), &latest_perf_json);
    }
    #[cfg(windows)]
    {
        let _ = fs::copy(&perf_json_file, &latest_perf_json);
    }

    println!("   Exported performance data to perf-data.json");
}

/// Export a comprehensive Markdown report of the latest benchmark results
/// This is useful for LLMs to read (they can't interact with HTML SPAs)
fn export_markdown_report(
    data: &parser::BenchmarkData,
    ordered_benchmarks: &(Vec<String>, std::collections::HashMap<String, Vec<String>>),
    git_info: &report::GitInfo,
    report_dir: &Path,
    timestamp: &str,
) {
    use parser::Operation;

    let mut md = String::new();

    // Header
    md.push_str("# Facet Benchmark Results\n\n");
    md.push_str(&format!("**Generated:** {}\n\n", timestamp));
    md.push_str(&format!(
        "**Commit:** {} (`{}`)\n\n",
        git_info.commit_short, git_info.branch
    ));
    if !git_info.commit_message.is_empty() {
        md.push_str(&format!("**Message:** {}\n\n", git_info.commit_message));
    }

    // Target definitions (order: baseline → best → good → reflection)
    md.push_str("## Targets\n\n");
    md.push_str("| Target | Description |\n");
    md.push_str("|--------|-------------|\n");
    md.push_str("| `serde_json` | Baseline (serde_json crate) |\n");
    md.push_str(
        "| `format+jit2` | Tier-2 JIT (format-specific, direct byte parsing via Cranelift) |\n",
    );
    md.push_str("| `format+jit1` | Tier-1 JIT (shape-based, ParseEvent stream) |\n");
    md.push_str("| `format` | facet-json without JIT (reflection only) |\n");
    md.push('\n');

    // Canonical target order for tables: baseline → best → good → reflection
    let targets_order = [
        ("serde_json", "serde_json"),
        ("facet_json_t2", "facet+jit2"),
        ("facet_json_t1", "facet+jit1"),
        ("facet_json_t0", "facet"),
    ];

    let (section_order, benchmarks_by_section) = ordered_benchmarks;

    // Group labels
    let group_labels: std::collections::HashMap<&str, &str> = [
        ("micro", "Micro Benchmarks"),
        ("synthetic", "Synthetic Benchmarks"),
        ("realistic", "Realistic Benchmarks"),
        ("other", "Other Benchmarks"),
    ]
    .into_iter()
    .collect();

    // Process each section
    for group_id in section_order {
        let default_label: &str = group_id.as_str();
        let label = group_labels
            .get(group_id.as_str())
            .unwrap_or(&default_label);
        let benches = benchmarks_by_section
            .get(group_id)
            .cloned()
            .unwrap_or_default();

        if benches.is_empty() {
            continue;
        }

        md.push_str(&format!("## {}\n\n", label));

        for bench in &benches {
            md.push_str(&format!("### {}\n\n", bench));

            // Deserialize table
            md.push_str("**Deserialize:**\n\n");
            md.push_str("| Target | Time (median) | Instructions | vs serde_json |\n");
            md.push_str("|--------|---------------|--------------|---------------|\n");

            // Get baseline values for ratio calculation
            let baseline_time = data
                .divan
                .get(bench)
                .and_then(|o| o.get(&Operation::Deserialize))
                .and_then(|t| t.get("serde_json"))
                .copied();

            let baseline_instr = data
                .gungraun
                .get(bench)
                .and_then(|o| o.get(&Operation::Deserialize))
                .and_then(|t| t.get("serde_json"))
                .map(|m| m.instructions);

            for (target_key, target_label) in &targets_order {
                let time_ns = data
                    .divan
                    .get(bench)
                    .and_then(|o| o.get(&Operation::Deserialize))
                    .and_then(|t| t.get(*target_key))
                    .copied();

                let instr = data
                    .gungraun
                    .get(bench)
                    .and_then(|o| o.get(&Operation::Deserialize))
                    .and_then(|t| t.get(*target_key))
                    .map(|m| m.instructions);

                let time_str = time_ns.map(format_time).unwrap_or_else(|| "-".to_string());
                let instr_str = instr
                    .map(format_with_commas)
                    .unwrap_or_else(|| "-".to_string());

                // Calculate ratio vs baseline (use instructions if available, else time)
                let ratio_str = if *target_key == "serde_json" {
                    "1.00×".to_string()
                } else if let (Some(val), Some(base)) = (instr, baseline_instr) {
                    if base > 0 {
                        let ratio = val as f64 / base as f64;
                        format_ratio(ratio)
                    } else {
                        "-".to_string()
                    }
                } else if let (Some(val), Some(base)) = (time_ns, baseline_time) {
                    if base > 0.0 {
                        let ratio = val / base;
                        format_ratio(ratio)
                    } else {
                        "-".to_string()
                    }
                } else {
                    "-".to_string()
                };

                md.push_str(&format!(
                    "| {} | {} | {} | {} |\n",
                    target_label, time_str, instr_str, ratio_str
                ));
            }
            md.push('\n');

            // Serialize table (only serde_json and format have serialize)
            let has_serialize = data
                .divan
                .get(bench)
                .and_then(|o| o.get(&Operation::Serialize))
                .map(|t| !t.is_empty())
                .unwrap_or(false);

            if has_serialize {
                md.push_str("**Serialize:**\n\n");
                md.push_str("| Target | Time (median) | Instructions | vs serde_json |\n");
                md.push_str("|--------|---------------|--------------|---------------|\n");

                let baseline_time_ser = data
                    .divan
                    .get(bench)
                    .and_then(|o| o.get(&Operation::Serialize))
                    .and_then(|t| t.get("serde_json"))
                    .copied();

                let baseline_instr_ser = data
                    .gungraun
                    .get(bench)
                    .and_then(|o| o.get(&Operation::Serialize))
                    .and_then(|t| t.get("serde_json"))
                    .map(|m| m.instructions);

                // Only show targets that have serialize benchmarks
                for (target_key, target_label) in
                    &[("serde_json", "serde_json"), ("facet_json_t0", "facet")]
                {
                    let time_ns = data
                        .divan
                        .get(bench)
                        .and_then(|o| o.get(&Operation::Serialize))
                        .and_then(|t| t.get(*target_key))
                        .copied();

                    let instr = data
                        .gungraun
                        .get(bench)
                        .and_then(|o| o.get(&Operation::Serialize))
                        .and_then(|t| t.get(*target_key))
                        .map(|m| m.instructions);

                    let time_str = time_ns.map(format_time).unwrap_or_else(|| "-".to_string());
                    let instr_str = instr
                        .map(format_with_commas)
                        .unwrap_or_else(|| "-".to_string());

                    let ratio_str = if *target_key == "serde_json" {
                        "1.00×".to_string()
                    } else if let (Some(val), Some(base)) = (instr, baseline_instr_ser) {
                        if base > 0 {
                            let ratio = val as f64 / base as f64;
                            format_ratio(ratio)
                        } else {
                            "-".to_string()
                        }
                    } else if let (Some(val), Some(base)) = (time_ns, baseline_time_ser) {
                        if base > 0.0 {
                            let ratio = val / base;
                            format_ratio(ratio)
                        } else {
                            "-".to_string()
                        }
                    } else {
                        "-".to_string()
                    };

                    md.push_str(&format!(
                        "| {} | {} | {} | {} |\n",
                        target_label, time_str, instr_str, ratio_str
                    ));
                }
                md.push('\n');
            }
        }
    }

    // Summary section with key insights
    md.push_str("## Summary\n\n");
    md.push_str("Key performance insights:\n\n");

    // Find benchmarks where Tier-2 beats or matches serde_json
    let mut wins = Vec::new();
    let mut close = Vec::new();
    let mut needs_work = Vec::new();

    for group_id in section_order {
        if let Some(benches) = benchmarks_by_section.get(group_id) {
            for bench in benches {
                let t2_instr = data
                    .gungraun
                    .get(bench)
                    .and_then(|o| o.get(&Operation::Deserialize))
                    .and_then(|t| t.get("facet_json_t2"))
                    .map(|m| m.instructions);

                let serde_instr = data
                    .gungraun
                    .get(bench)
                    .and_then(|o| o.get(&Operation::Deserialize))
                    .and_then(|t| t.get("serde_json"))
                    .map(|m| m.instructions);

                if let (Some(t2), Some(serde)) = (t2_instr, serde_instr) {
                    let ratio = t2 as f64 / serde as f64;
                    if ratio <= 1.0 {
                        wins.push((bench.clone(), ratio));
                    } else if ratio <= 1.5 {
                        close.push((bench.clone(), ratio));
                    } else {
                        needs_work.push((bench.clone(), ratio));
                    }
                }
            }
        }
    }

    if !wins.is_empty() {
        md.push_str("**Tier-2 JIT beats or matches serde_json:**\n");
        for (bench, ratio) in &wins {
            md.push_str(&format!("- `{}`: {}\n", bench, format_ratio(*ratio)));
        }
        md.push('\n');
    }

    if !close.is_empty() {
        md.push_str("**Tier-2 JIT within 1.5× of serde_json:**\n");
        for (bench, ratio) in &close {
            md.push_str(&format!("- `{}`: {}\n", bench, format_ratio(*ratio)));
        }
        md.push('\n');
    }

    if !needs_work.is_empty() {
        md.push_str("**Needs optimization (>1.5× slower):**\n");
        for (bench, ratio) in &needs_work {
            md.push_str(&format!("- `{}`: {}\n", bench, format_ratio(*ratio)));
        }
        md.push('\n');
    }

    // Write to file
    let perf_dir = report_dir.join("perf");
    fs::create_dir_all(&perf_dir).ok();
    let md_path = perf_dir.join("RESULTS.md");
    fs::write(&md_path, &md).expect("Failed to write RESULTS.md");

    println!("   Exported Markdown report to perf/RESULTS.md");
}

/// Format nanoseconds as a human-readable time string
fn format_time(ns: f64) -> String {
    if ns >= 1_000_000_000.0 {
        format!("{:.2}s", ns / 1_000_000_000.0)
    } else if ns >= 1_000_000.0 {
        format!("{:.2}ms", ns / 1_000_000.0)
    } else if ns >= 1_000.0 {
        format!("{:.2}µs", ns / 1_000.0)
    } else {
        format!("{:.0}ns", ns)
    }
}

/// Format a number with comma separators
fn format_with_commas(n: u64) -> String {
    let s = n.to_string();
    let mut result = String::new();
    for (i, c) in s.chars().rev().enumerate() {
        if i > 0 && i % 3 == 0 {
            result.push(',');
        }
        result.push(c);
    }
    result.chars().rev().collect()
}

/// Format a ratio with appropriate styling
fn format_ratio(ratio: f64) -> String {
    if ratio <= 1.0 {
        format!("**{:.2}×** ✓", ratio)
    } else if ratio <= 1.5 {
        format!("{:.2}×", ratio)
    } else {
        format!("{:.2}× ⚠", ratio)
    }
}

/// Export benchmark data in the run-v1.json format
/// Schema: { schema, run, defaults, catalog, results }
fn export_run_json(
    data: &parser::BenchmarkData,
    ordered_benchmarks: &(Vec<String>, std::collections::HashMap<String, Vec<String>>),
    git_info: &report::GitInfo,
    report_dir: &Path,
    divan_failures: &[String],
    gungraun_failures: &[String],
) {
    use indexmap::IndexMap;
    use run_types::*;
    use std::collections::HashMap;

    let (section_order, benchmarks_by_section) = ordered_benchmarks;

    // Get branch info
    let branch_key = sanitize_branch_key(&git_info.branch);
    let branch_original = if branch_key != git_info.branch {
        Some(git_info.branch.clone())
    } else {
        None
    };

    let run_id = format!("{}/{}", branch_key, git_info.commit);
    let timestamp = chrono::Utc::now();

    // Canonical definitions - must match benchmark function names
    // Order: baseline → best → good → reflection
    let targets_order = [
        "serde_json",
        "facet_json_t2",
        "facet_json_t1",
        "facet_json_t0",
    ];
    let metrics_order = [
        "instructions",
        "estimated_cycles",
        "time_median_ns",
        "l1_hits",
        "ll_hits",
        "ram_hits",
        "total_read_write",
    ];

    // Collect all benchmarks in order
    let mut all_benchmarks: Vec<String> = Vec::new();
    for group_id in section_order {
        if let Some(benches) = benchmarks_by_section.get(group_id) {
            all_benchmarks.extend(benches.iter().cloned());
        }
    }

    // Add any uncategorized benchmarks from collected data
    // Handle both old format (bare names) and new format (format::case)
    let mut seen: std::collections::HashSet<String> = all_benchmarks.iter().cloned().collect();
    for bench in data.divan.keys().chain(data.gungraun.keys()) {
        // Strip format prefix if present (e.g., "json::simple_struct" -> "simple_struct")
        // This maintains backward compatibility with old benchmark naming
        let normalized = if let Some((_format, case)) = bench.split_once("::") {
            case.to_string()
        } else {
            bench.clone()
        };

        if !seen.contains(&normalized) && !seen.contains(bench) {
            seen.insert(normalized.clone());
            all_benchmarks.push(normalized);
        }
    }

    // Build groups catalog
    let group_labels: HashMap<&str, &str> = [
        ("micro", "Micro Benchmarks"),
        ("synthetic", "Synthetic Benchmarks"),
        ("realistic", "Realistic Benchmarks"),
        ("other", "Other Benchmarks"),
    ]
    .into_iter()
    .collect();

    // Use IndexMap to preserve insertion order for JSON output
    let mut groups = IndexMap::new();
    for group_id in section_order {
        let default_label: &str = group_id.as_str();
        let label = group_labels
            .get(group_id.as_str())
            .unwrap_or(&default_label);
        let benches = benchmarks_by_section
            .get(group_id)
            .cloned()
            .unwrap_or_default();
        groups.insert(
            group_id.clone(),
            GroupDef {
                label: label.to_string(),
                benchmarks_order: benches,
            },
        );
    }

    // Build benchmarks catalog
    // Use IndexMap to preserve insertion order for JSON output
    let mut benchmarks_catalog = IndexMap::new();
    for bench in &all_benchmarks {
        let group = benchmarks_by_section
            .iter()
            .find(|(_, benches)| benches.contains(bench))
            .map(|(g, _)| g.as_str())
            .unwrap_or("other");

        benchmarks_catalog.insert(
            bench.clone(),
            BenchmarkDef {
                key: bench.clone(),
                label: bench.clone(),
                group: group.to_string(),
                targets_order: targets_order.iter().map(|s| s.to_string()).collect(),
                metrics_order: metrics_order.iter().map(|s| s.to_string()).collect(),
            },
        );
    }

    // Build targets catalog - keys must match benchmark function names
    // Order: baseline → best → good → reflection
    // Use IndexMap to preserve insertion order for JSON output
    let target_defs = [
        ("serde_json", "serde_json", "baseline"),
        ("facet_json_t2", "facet+jit2", "facet"),
        ("facet_json_t1", "facet+jit1", "facet"),
        ("facet_json_t0", "facet", "facet"),
    ];
    let mut targets = IndexMap::new();
    for (key, label, kind) in target_defs {
        targets.insert(
            key.to_string(),
            TargetDef {
                key: key.to_string(),
                label: label.to_string(),
                kind: kind.to_string(),
            },
        );
    }

    // Build metrics catalog
    // Use IndexMap to preserve insertion order for JSON output
    let metric_defs = [
        ("instructions", "Instructions", "count", "lower"),
        ("estimated_cycles", "Est. Cycles", "count", "lower"),
        ("time_median_ns", "Median Time", "ns", "lower"),
        ("l1_hits", "L1 Hits", "count", "lower"),
        ("ll_hits", "LL Hits", "count", "lower"),
        ("ram_hits", "RAM Hits", "count", "lower"),
        ("total_read_write", "Total R/W", "count", "lower"),
    ];
    let mut metrics = IndexMap::new();
    for (key, label, unit, better) in metric_defs {
        metrics.insert(
            key.to_string(),
            MetricDef {
                key: key.to_string(),
                label: label.to_string(),
                unit: unit.to_string(),
                better: better.to_string(),
            },
        );
    }

    // Build results
    let mut values = IndexMap::new();
    for benchmark in &all_benchmarks {
        let mut ops = BenchmarkOps {
            deserialize: IndexMap::new(),
            serialize: IndexMap::new(),
        };

        // Try both bare name and json-prefixed name for backward/forward compatibility
        let benchmark_keys = [benchmark.clone(), format!("json::{}", benchmark)];

        for (op, op_map) in [
            (parser::Operation::Deserialize, &mut ops.deserialize),
            (parser::Operation::Serialize, &mut ops.serialize),
        ] {
            for target in &targets_order {
                // Try each possible benchmark key
                let divan_time = benchmark_keys.iter().find_map(|key| {
                    data.divan
                        .get(key)
                        .and_then(|o| o.get(&op))
                        .and_then(|t| t.get(*target))
                });

                let gungraun_metrics = benchmark_keys.iter().find_map(|key| {
                    data.gungraun
                        .get(key)
                        .and_then(|o| o.get(&op))
                        .and_then(|t| t.get(*target))
                });

                let tier_stats = benchmark_keys.iter().find_map(|key| {
                    data.tier_stats
                        .get(key)
                        .and_then(|o| o.get(&op))
                        .and_then(|t| t.get(*target))
                });

                let target_metrics =
                    if divan_time.is_some() || gungraun_metrics.is_some() || tier_stats.is_some() {
                        let mut tm = TargetMetrics::default();
                        if let Some(gm) = gungraun_metrics {
                            tm.instructions = Some(gm.instructions);
                            tm.estimated_cycles = gm.estimated_cycles;
                            tm.l1_hits = gm.l1_hits;
                            tm.ll_hits = gm.ll_hits;
                            tm.ram_hits = gm.ram_hits;
                            tm.total_read_write = gm.total_read_write;
                        }
                        if let Some(time_ns) = divan_time {
                            tm.time_median_ns = Some(*time_ns);
                        }
                        if let Some(ts) = tier_stats {
                            tm.tier2_attempts = Some(ts.tier2_attempts);
                            tm.tier2_successes = Some(ts.tier2_successes);
                            tm.tier2_compile_unsupported = Some(ts.tier2_compile_unsupported);
                            tm.tier2_runtime_unsupported = Some(ts.tier2_runtime_unsupported);
                            tm.tier2_runtime_error = Some(ts.tier2_runtime_error);
                            tm.tier1_fallbacks = Some(ts.tier1_fallbacks);
                        }
                        Some(tm)
                    } else {
                        None
                    };

                op_map.insert(target.to_string(), target_metrics);
            }
        }

        values.insert(benchmark.clone(), ops);
    }

    // Build errors section
    let errors = if !divan_failures.is_empty() || !gungraun_failures.is_empty() {
        RunErrors {
            parse_failures: Some(ParseFailures {
                divan: divan_failures.to_vec(),
                gungraun: gungraun_failures.to_vec(),
            }),
        }
    } else {
        RunErrors::default()
    };

    // Build the full RunJson structure
    let run_json = RunJson {
        schema: Some("run-v1".to_string()),
        run: RunMeta {
            run_id,
            branch_key,
            branch_original,
            sha: Some(git_info.commit.clone()),
            commit: None, // Not used in new schema
            short: Some(git_info.commit_short.clone()),
            commit_short: None, // Not used in new schema
            timestamp: Some(timestamp.to_rfc3339()),
            generated_at: None, // Not used in new schema
            timestamp_unix: Some(timestamp.timestamp()),
            commit_message: git_info.commit_message.clone(),
            pr_number: git_info.pr_number.clone(),
            pr_title: git_info.pr_title.clone(),
            tool_versions: Some(ToolVersions {
                divan: "present".to_string(),
                gungraun: "present".to_string(),
            }),
        },
        defaults: Some(RunDefaults {
            operation: "deserialize".to_string(),
            metric: "instructions".to_string(),
            baseline_target: "serde_json".to_string(),
            primary_target: "facet_json_t2".to_string(),
            comparison_mode: "none".to_string(),
        }),
        catalog: Some(RunCatalog {
            groups_order: section_order.clone(),
            groups,
            benchmarks: benchmarks_catalog,
            targets,
            metrics,
        }),
        results: RunResults { values, errors },
    };

    // Serialize with facet_json
    let json = facet_json::to_string_pretty(&run_json).expect("Failed to serialize run data");

    // Write to file
    let run_json_file = report_dir.join("run.json");
    fs::write(&run_json_file, &json).expect("Failed to write run.json");
    println!("   Exported run data to run.json (schema: run-v1)");
}

/// Sanitize a branch name to be URL-safe
fn sanitize_branch_key(branch: &str) -> String {
    branch
        .replace(['/', ' ', ':'], "_")
        .chars()
        .filter(|c| c.is_alphanumeric() || *c == '_' || *c == '-' || *c == '.')
        .collect()
}

fn main() {
    let args: Args = match args::from_std_args() {
        Ok(args) => args,
        Err(e) => {
            eprintln!("{:?}", Report::new(e));
            std::process::exit(1);
        }
    };

    // Find workspace root
    let workspace_root = find_workspace_root().unwrap_or_else(|| {
        eprintln!("Could not find workspace root");
        std::process::exit(1);
    });

    let report_dir = workspace_root.join("bench-reports");
    fs::create_dir_all(&report_dir).expect("Failed to create bench-reports directory");

    // Copy fonts for the report
    let fonts_src = workspace_root.join("docs/static/fonts");
    for font in ["IosevkaFtl-Regular.ttf", "IosevkaFtl-Bold.ttf"] {
        let src = fonts_src.join(font);
        let dst = report_dir.join(font);
        if src.exists() && !dst.exists() {
            let _ = fs::copy(&src, &dst);
        }
    }

    let timestamp = Local::now().format("%Y%m%d-%H%M%S").to_string();

    let divan_file = report_dir.join(format!("divan-{}.txt", timestamp));
    let gungraun_file = report_dir.join(format!("gungraun-{}.txt", timestamp));

    if !args.no_run {
        println!("🏃 Running benchmarks...");
        println!();

        // Run divan benchmarks - fail fast if it crashes
        if !run_benchmark_with_progress(
            &workspace_root,
            "unified_divan",
            &divan_file,
            "📊 Running divan (wall-clock)",
            args.filter.as_deref(),
        ) {
            eprintln!();
            eprintln!(
                "{}",
                "❌ Divan benchmark failed. Fix the errors and try again."
                    .red()
                    .bold()
            );
            std::process::exit(1);
        }

        // Run gungraun benchmarks - fail fast if it crashes
        // NOTE: Gungraun requires wildcard patterns to match the full module path.
        // Convert simple filter "foo" to "*foo*" to match any path containing "foo".
        let gungraun_filter = args.filter.as_ref().map(|f| {
            if f.contains('*') || f.contains("::") {
                f.clone() // Already has wildcards or path separators
            } else {
                format!("*{}*", f) // Wrap simple patterns with wildcards
            }
        });

        if !run_benchmark_with_progress(
            &workspace_root,
            "unified_gungraun",
            &gungraun_file,
            "🔬 Running gungraun (instruction counts)",
            gungraun_filter.as_deref(),
        ) {
            eprintln!();
            eprintln!(
                "{}",
                "❌ Gungraun benchmark failed. Fix the errors and try again."
                    .red()
                    .bold()
            );
            std::process::exit(1);
        }
    } else {
        println!("⏭️  Skipping benchmark run (--no-run)");
        // Find most recent files
        if let Some((d, g)) = find_latest_benchmark_files(&report_dir) {
            println!("   Using: {}", d.file_name().unwrap().to_string_lossy());
            println!("   Using: {}", g.file_name().unwrap().to_string_lossy());
            // Copy to new timestamp files for consistency
            fs::copy(&d, &divan_file).ok();
            fs::copy(&g, &gungraun_file).ok();
        } else {
            eprintln!("❌ No existing benchmark files found");
            std::process::exit(1);
        }
    }

    println!();
    println!("📝 Parsing benchmark data and generating HTML report...");

    // Parse outputs
    let divan_text = fs::read_to_string(&divan_file).unwrap_or_default();
    let gungraun_text = fs::read_to_string(&gungraun_file).unwrap_or_default();

    let divan_parsed = parser::parse_divan(&divan_text);
    let gungraun_parsed = parser::parse_gungraun(&gungraun_text);
    let tier_stats_parsed = parser::parse_tier_stats(&gungraun_text);

    println!(
        "   Parsed {} divan results, {} gungraun results, {} tier stats",
        divan_parsed.results.len(),
        gungraun_parsed.results.len(),
        tier_stats_parsed.results.len()
    );

    // Check for parse failures - fail fast on first batch of failures
    if !divan_parsed.failures.is_empty() {
        eprintln!();
        eprintln!(
            "{}",
            format!("❌ {} divan parse failures:", divan_parsed.failures.len())
                .red()
                .bold()
        );
        for failure in &divan_parsed.failures {
            eprintln!("   {}", failure.red());
        }
        eprintln!();
        eprintln!("{}", "Check the raw output file:".yellow());
        eprintln!("   {}", divan_file.display());
        std::process::exit(1);
    }

    if !gungraun_parsed.failures.is_empty() {
        eprintln!();
        eprintln!(
            "{}",
            format!(
                "❌ {} gungraun parse failures:",
                gungraun_parsed.failures.len()
            )
            .red()
            .bold()
        );
        for failure in &gungraun_parsed.failures {
            eprintln!("   {}", failure.red());
        }
        eprintln!();
        eprintln!("{}", "Check the raw output file:".yellow());
        eprintln!("   {}", gungraun_file.display());
        std::process::exit(1);
    }

    let data = parser::combine_results(
        divan_parsed.results,
        gungraun_parsed.results,
        tier_stats_parsed.results,
    );

    // Export gungraun instruction counts to JSON (for perf delta tracking)
    export_perf_json(&data, &report_dir, &timestamp);

    // Load ordered benchmark definitions from KDL (multi-format)
    let all_formats = benchmark_defs::load_ordered_benchmarks(&workspace_root);
    let total_benchmarks: usize = all_formats
        .values()
        .flat_map(|(_, by_section)| by_section.values())
        .map(|v| v.len())
        .sum();
    println!(
        "   Loaded {} benchmark definitions across {} formats",
        total_benchmarks,
        all_formats.len()
    );

    // For backward compatibility, extract JSON format benchmarks
    // TODO: Update to support multi-format export
    let ordered_benchmarks = all_formats
        .get("json")
        .cloned()
        .unwrap_or_else(|| (vec![], std::collections::HashMap::new()));

    // Get git info - prefer environment variables (set by CI) over git commands
    // This ensures consistency with perf_index.rs which also uses these env vars
    let commit_full =
        std::env::var("COMMIT").unwrap_or_else(|_| get_git_output(&["rev-parse", "HEAD"]));
    let commit_short = std::env::var("COMMIT_SHORT")
        .unwrap_or_else(|_| get_git_output(&["rev-parse", "--short", "HEAD"]));
    let commit_message = std::env::var("COMMIT_MESSAGE")
        .unwrap_or_else(|_| get_git_output(&["log", "-1", "--format=%s"]));
    let branch = std::env::var("BRANCH_ORIGINAL")
        .unwrap_or_else(|_| get_git_output(&["branch", "--show-current"]));

    // Try to get PR info from CI environment variables
    // GitHub Actions: PR_NUMBER, PR_TITLE (set by our workflow)
    // Fallback: GITHUB_PR_NUMBER, CI_MERGE_REQUEST_IID
    let pr_number = std::env::var("PR_NUMBER")
        .or_else(|_| std::env::var("GITHUB_PR_NUMBER"))
        .or_else(|_| std::env::var("CI_MERGE_REQUEST_IID"))
        .ok()
        .filter(|s| !s.is_empty());
    let pr_title = std::env::var("PR_TITLE")
        .or_else(|_| std::env::var("GITHUB_PR_TITLE"))
        .or_else(|_| std::env::var("CI_MERGE_REQUEST_TITLE"))
        .ok()
        .filter(|s| !s.is_empty());

    let git_info = report::GitInfo {
        commit: commit_full,
        commit_short,
        branch,
        commit_message,
        pr_number,
        pr_title,
    };

    // Export new run-v1.json format (includes both divan and gungraun metrics)
    export_run_json(
        &data,
        &ordered_benchmarks,
        &git_info,
        &report_dir,
        &[], // divan_failures - empty since we exit early on failures
        &[], // gungraun_failures - empty since we exit early on failures
    );

    // Export Markdown report for LLMs
    export_markdown_report(
        &data,
        &ordered_benchmarks,
        &git_info,
        &report_dir,
        &timestamp,
    );

    println!();
    println!("✅ Benchmark data exported to run.json and perf/RESULTS.md");
    println!();

    // Clone perf repo, copy reports, generate index (default behavior, skip with --no-index)
    if !args.no_index {
        match perf_index::run_perf_index(
            &workspace_root,
            &report_dir,
            args.filter.as_deref(),
            args.push,
        ) {
            Ok(result) => {
                println!();
                println!(
                    "✅ Perf index generated at: {}",
                    file_hyperlink(&result.perf_dir)
                );
                println!();

                // Start server if --serve was passed
                if args.serve {
                    let rt =
                        tokio::runtime::Runtime::new().expect("Failed to create tokio runtime");
                    rt.block_on(async {
                        if let Err(e) = server::serve(&result.perf_dir, 1999).await {
                            eprintln!("Server error: {}", e);
                        }
                    });
                }
            }
            Err(e) => {
                eprintln!();
                eprintln!(
                    "{}",
                    format!("❌ Perf index generation failed: {}", e)
                        .red()
                        .bold()
                );
                std::process::exit(1);
            }
        }
    } else {
        // No --index, just show what's available
        println!("To view the results:");
        println!();
        println!("  With full perf.facet.rs index (recommended):");
        println!("    cargo xtask bench --index --serve");
        println!();
        println!("  Just generate the index locally:");
        println!("    cargo xtask bench --index");
    }
}

fn find_workspace_root() -> Option<PathBuf> {
    let mut current = std::env::current_dir().ok()?;
    loop {
        let cargo_toml = current.join("Cargo.toml");
        if cargo_toml.exists()
            && let Ok(content) = fs::read_to_string(&cargo_toml)
            && content.contains("[workspace]")
        {
            return Some(current);
        }
        if !current.pop() {
            return None;
        }
    }
}

const BUFFER_LINES: usize = 6;
const SPINNER_CHARS: &[char] = &['⠋', '⠙', '⠹', '⠸', '⠼', '⠴', '⠦', '⠧', '⠇', '⠏'];

/// Wrap a string to fit within a given display width (UTF-8 safe).
/// Returns a Vec of lines that each fit within max_width.
fn wrap_to_width(s: &str, max_width: usize) -> Vec<String> {
    use unicode_width::UnicodeWidthChar;

    if max_width == 0 {
        return vec![s.to_string()];
    }

    let mut lines = Vec::new();
    let mut current_line = String::new();
    let mut current_width = 0;

    for c in s.chars() {
        let char_width = c.width().unwrap_or(0);
        if current_width + char_width > max_width && !current_line.is_empty() {
            lines.push(std::mem::take(&mut current_line));
            current_width = 0;
        }
        current_width += char_width;
        current_line.push(c);
    }

    if !current_line.is_empty() {
        lines.push(current_line);
    }

    if lines.is_empty() {
        lines.push(String::new());
    }

    lines
}

fn run_benchmark_with_progress(
    workspace_root: &Path,
    bench_name: &str,
    output_file: &Path,
    label: &str,
    filter: Option<&str>,
) -> bool {
    // Check if we're in CI - if so, skip fancy spinner and just inherit stdio
    let is_ci = std::env::var("CI").is_ok() || std::env::var("GITHUB_ACTIONS").is_ok();

    let mut cmd = Command::new("cargo");
    cmd.args(["bench", "--bench", bench_name, "--features", "jit"]);

    // Add filter and gungraun options (passed after --)
    cmd.arg("--");
    if let Some(f) = filter {
        cmd.arg(f);
    }
    // Enable stderr output for gungraun benchmarks (needed for tier stats)
    if bench_name.contains("gungraun") {
        cmd.arg("--nocapture");
    }

    cmd.current_dir(workspace_root.join("facet-perf-shootout"));

    if is_ci {
        // In CI: stream output to both stdout AND capture for parsing
        println!("▶ {label}...");
        println!();

        let mut child = cmd
            .stdout(Stdio::piped())
            .stderr(Stdio::inherit())
            .spawn()
            .expect("Failed to run benchmark");

        // Read stdout line by line: print AND capture
        let stdout = child.stdout.take().expect("Failed to get stdout");
        let reader = BufReader::new(stdout);
        let mut stdout_lines = Vec::new();

        for line in reader.lines().map_while(Result::ok) {
            println!("{}", line); // Stream to CI logs
            stdout_lines.push(line); // Capture for parsing
        }

        let status = child.wait().expect("Failed to wait for benchmark");

        if !status.success() {
            return false;
        }

        // Write captured output to file for parsing
        let combined = stdout_lines.join("\n");
        fs::write(output_file, combined).expect("Failed to write output file");

        println!();
        println!("✓ {label} complete");
        return true;
    }

    let term = Term::stderr();

    let mut child = cmd
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("Failed to run benchmark");

    // Read stdout and stderr in separate threads, send lines to main thread
    let stdout = child.stdout.take().expect("Failed to get stdout");
    let stderr = child.stderr.take().expect("Failed to get stderr");

    let (tx, rx) = mpsc::channel::<String>();
    let tx_stderr = tx.clone();
    let tx_stdout = tx;

    // Spawn thread to read stderr
    let stderr_handle = std::thread::spawn(move || {
        let reader = BufReader::new(stderr);
        let mut lines = Vec::new();
        for line in reader.lines().map_while(Result::ok) {
            let _ = tx_stderr.send(line.clone());
            lines.push(line);
        }
        lines
    });

    // Spawn thread to read stdout
    let stdout_handle = std::thread::spawn(move || {
        let reader = BufReader::new(stdout);
        let mut lines = Vec::new();
        for line in reader.lines().map_while(Result::ok) {
            let _ = tx_stdout.send(line.clone());
            lines.push(line);
        }
        lines
    });

    // Display rolling buffer of wrapped lines
    let mut display_buffer: VecDeque<String> = VecDeque::with_capacity(BUFFER_LINES);
    let mut total_lines = 0;
    let mut spinner_idx = 0;
    let width = term.size().1 as usize;

    // Print initial header
    let spinner = SPINNER_CHARS[0];
    let _ = term.write_line(&format!("{spinner} {label}..."));
    let mut displayed_lines = 1;

    loop {
        match rx.recv_timeout(Duration::from_millis(50)) {
            Ok(line) => {
                total_lines += 1;
                spinner_idx = (spinner_idx + 1) % SPINNER_CHARS.len();

                // Wrap the incoming line and add each wrapped segment to buffer
                // Account for 2-space indent
                let wrapped = wrap_to_width(&line, width.saturating_sub(2));
                for wrapped_line in wrapped {
                    if display_buffer.len() == BUFFER_LINES {
                        display_buffer.pop_front();
                    }
                    display_buffer.push_back(wrapped_line);
                }

                // Redraw: clear previous lines and redraw buffer
                if displayed_lines > 0 {
                    let _ = term.clear_last_lines(displayed_lines);
                }

                // Draw header + separator + buffer + separator
                let separator = "─".repeat(width);
                let spinner = SPINNER_CHARS[spinner_idx];

                let _ = term.write_line(&format!("{spinner} {label}... ({total_lines} lines)"));
                let _ = term.write_line(&separator);
                for buf_line in &display_buffer {
                    let _ = term.write_line(&format!("  {buf_line}"));
                }
                let _ = term.write_line(&separator);
                displayed_lines = 2 + display_buffer.len() + 1; // header + sep + buffer + sep
            }
            Err(mpsc::RecvTimeoutError::Timeout) => {
                // Check if process has finished
                if let Ok(Some(_)) = child.try_wait() {
                    // Drain remaining messages
                    while let Ok(line) = rx.try_recv() {
                        total_lines += 1;
                        let wrapped = wrap_to_width(&line, width.saturating_sub(2));
                        for wrapped_line in wrapped {
                            if display_buffer.len() == BUFFER_LINES {
                                display_buffer.pop_front();
                            }
                            display_buffer.push_back(wrapped_line);
                        }
                    }
                    break;
                }

                // Keep spinner spinning even when no output
                spinner_idx = (spinner_idx + 1) % SPINNER_CHARS.len();
                if displayed_lines > 0 {
                    let _ = term.clear_last_lines(displayed_lines);
                }
                let separator = "─".repeat(width);
                let spinner = SPINNER_CHARS[spinner_idx];
                let _ = term.write_line(&format!("{spinner} {label}... ({total_lines} lines)"));
                let _ = term.write_line(&separator);
                for buf_line in &display_buffer {
                    let _ = term.write_line(&format!("  {buf_line}"));
                }
                let _ = term.write_line(&separator);
                displayed_lines = 2 + display_buffer.len() + 1;
            }
            Err(mpsc::RecvTimeoutError::Disconnected) => {
                break;
            }
        }
    }

    // Wait for the process and threads
    let status = child.wait().expect("Failed to wait for benchmark");
    let stderr_lines = stderr_handle.join().expect("stderr thread panicked");
    let stdout_lines = stdout_handle.join().expect("stdout thread panicked");

    // Combine output for file
    let mut combined = String::new();
    for line in &stderr_lines {
        combined.push_str(line);
        combined.push('\n');
    }
    for line in &stdout_lines {
        combined.push_str(line);
        combined.push('\n');
    }

    fs::write(output_file, &combined).expect("Failed to write benchmark output");

    // Clear the rolling buffer display
    if displayed_lines > 0 {
        let _ = term.clear_last_lines(displayed_lines);
    }

    if status.success() {
        println!("{label}... ✓ {total_lines} lines");
        true
    } else {
        println!("{label}... ✗ failed");
        eprintln!("Benchmark failed with exit code: {:?}", status.code());
        eprintln!();
        eprintln!("--- Full output ---");
        eprint!("{combined}");
        eprintln!("--- End output ---");
        false
    }
}

fn find_latest_benchmark_files(report_dir: &PathBuf) -> Option<(PathBuf, PathBuf)> {
    let mut divan_files: Vec<_> = fs::read_dir(report_dir)
        .ok()?
        .filter_map(|e| e.ok())
        .filter(|e| e.file_name().to_string_lossy().starts_with("divan-"))
        .collect();

    let mut gungraun_files: Vec<_> = fs::read_dir(report_dir)
        .ok()?
        .filter_map(|e| e.ok())
        .filter(|e| e.file_name().to_string_lossy().starts_with("gungraun-"))
        .collect();

    divan_files.sort_by_key(|e| e.file_name());
    gungraun_files.sort_by_key(|e| e.file_name());

    match (divan_files.last(), gungraun_files.last()) {
        (Some(d), Some(g)) => Some((d.path(), g.path())),
        _ => None,
    }
}

fn get_git_output(args: &[&str]) -> String {
    Command::new("git")
        .args(args)
        .output()
        .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string())
        .unwrap_or_else(|_| "unknown".to_string())
}
