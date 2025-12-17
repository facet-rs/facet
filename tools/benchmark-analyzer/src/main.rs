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
    //     "simple_struct": { "facet_format_jit": 6567, "serde_json": 8000, ... },
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

/// Escape a string for JSON
fn escape_json(s: &str) -> String {
    s.replace('\\', "\\\\")
        .replace('"', "\\\"")
        .replace('\n', "\\n")
        .replace('\r', "\\r")
        .replace('\t', "\\t")
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

    // Canonical definitions
    let groups_order = section_order.clone();
    let metrics_order = vec![
        "instructions",
        "estimated_cycles",
        "time_median_ns",
        "l1_hits",
        "ll_hits",
        "ram_hits",
        "total_read_write",
    ];
    let targets_order = vec![
        "serde_json",
        "facet_format_jit",
        "facet_format",
        "facet_json",
    ];

    // Build JSON manually (avoiding serde dependency per project guidelines)
    let mut json = String::from("{\n");
    json.push_str("  \"schema\": \"run-v1\",\n");

    // === RUN METADATA ===
    json.push_str("  \"run\": {\n");
    json.push_str(&format!("    \"run_id\": \"{}\",\n", run_id));
    json.push_str(&format!("    \"branch_key\": \"{}\",\n", branch_key));
    if let Some(ref orig) = branch_original {
        json.push_str(&format!(
            "    \"branch_original\": \"{}\",\n",
            escape_json(orig)
        ));
    }
    json.push_str(&format!("    \"sha\": \"{}\",\n", git_info.commit));
    json.push_str(&format!("    \"short\": \"{}\",\n", git_info.commit_short));
    json.push_str(&format!(
        "    \"timestamp\": \"{}\",\n",
        timestamp.to_rfc3339()
    ));
    json.push_str(&format!(
        "    \"timestamp_unix\": {},\n",
        timestamp.timestamp()
    ));
    json.push_str(&format!(
        "    \"commit_message\": \"{}\",\n",
        escape_json(&git_info.commit_message)
    ));
    if let Some(ref pr_num) = git_info.pr_number {
        json.push_str(&format!("    \"pr_number\": \"{}\",\n", pr_num));
    }
    if let Some(ref pr_title) = git_info.pr_title {
        json.push_str(&format!(
            "    \"pr_title\": \"{}\",\n",
            escape_json(pr_title)
        ));
    }
    json.push_str("    \"tool_versions\": {\n");
    json.push_str("      \"divan\": \"present\",\n");
    json.push_str("      \"gungraun\": \"present\"\n");
    json.push_str("    }\n");
    json.push_str("  },\n");

    // === DEFAULTS ===
    json.push_str("  \"defaults\": {\n");
    json.push_str("    \"operation\": \"deserialize\",\n");
    json.push_str("    \"metric\": \"instructions\",\n");
    json.push_str("    \"baseline_target\": \"serde_json\",\n");
    json.push_str("    \"primary_target\": \"facet_format_jit\",\n");
    json.push_str("    \"comparison_mode\": \"none\"\n");
    json.push_str("  },\n");

    // === CATALOG ===
    json.push_str("  \"catalog\": {\n");

    // Groups order
    json.push_str("    \"groups_order\": [");
    for (idx, g) in groups_order.iter().enumerate() {
        json.push_str(&format!("\"{}\"", g));
        if idx < groups_order.len() - 1 {
            json.push_str(", ");
        }
    }
    json.push_str("],\n");

    // Groups with labels and benchmark ordering
    json.push_str("    \"groups\": {\n");
    let group_labels: IndexMap<&str, &str> = [
        ("micro", "Micro Benchmarks"),
        ("synthetic", "Synthetic Benchmarks"),
        ("realistic", "Realistic Benchmarks"),
        ("other", "Other Benchmarks"),
    ]
    .into_iter()
    .collect();

    for (gidx, group_id) in groups_order.iter().enumerate() {
        let default_label: &str = group_id.as_str();
        let label = group_labels
            .get(group_id.as_str())
            .unwrap_or(&default_label);
        let benches = benchmarks_by_section
            .get(group_id)
            .map(|v| v.as_slice())
            .unwrap_or(&[]);
        json.push_str(&format!("      \"{}\": {{\n", group_id));
        json.push_str(&format!("        \"label\": \"{}\",\n", label));
        json.push_str("        \"benchmarks_order\": [");
        for (bidx, b) in benches.iter().enumerate() {
            json.push_str(&format!("\"{}\"", b));
            if bidx < benches.len() - 1 {
                json.push_str(", ");
            }
        }
        json.push_str("]\n");
        json.push_str("      }");
        if gidx < groups_order.len() - 1 {
            json.push(',');
        }
        json.push('\n');
    }
    json.push_str("    },\n");

    // Benchmarks catalog
    json.push_str("    \"benchmarks\": {\n");
    let mut all_benchmarks: Vec<String> = Vec::new();
    for group_id in &groups_order {
        if let Some(benches) = benchmarks_by_section.get(group_id) {
            for bench in benches {
                all_benchmarks.push(bench.clone());
            }
        }
    }
    // Add any uncategorized benchmarks (not in KDL)
    let mut seen: std::collections::HashSet<String> = all_benchmarks.iter().cloned().collect();
    for bench in data.divan.keys().chain(data.gungraun.keys()) {
        if !seen.contains(bench) {
            seen.insert(bench.clone());
            all_benchmarks.push(bench.clone());
        }
    }

    let bench_count = all_benchmarks.len();
    for (bidx, bench) in all_benchmarks.iter().enumerate() {
        // Find which group this benchmark belongs to
        let group = benchmarks_by_section
            .iter()
            .find(|(_, benches)| benches.contains(bench))
            .map(|(g, _)| g.as_str())
            .unwrap_or("other");

        json.push_str(&format!("      \"{}\": {{\n", bench));
        json.push_str(&format!("        \"key\": \"{}\",\n", bench));
        json.push_str(&format!("        \"label\": \"{}\",\n", bench));
        json.push_str(&format!("        \"group\": \"{}\",\n", group));
        json.push_str("        \"targets_order\": [");
        for (tidx, t) in targets_order.iter().enumerate() {
            json.push_str(&format!("\"{}\"", t));
            if tidx < targets_order.len() - 1 {
                json.push_str(", ");
            }
        }
        json.push_str("],\n");
        json.push_str("        \"metrics_order\": [");
        for (midx, m) in metrics_order.iter().enumerate() {
            json.push_str(&format!("\"{}\"", m));
            if midx < metrics_order.len() - 1 {
                json.push_str(", ");
            }
        }
        json.push_str("]\n");
        json.push_str("      }");
        if bidx < bench_count - 1 {
            json.push(',');
        }
        json.push('\n');
    }
    json.push_str("    },\n");

    // Targets catalog
    json.push_str("    \"targets\": {\n");
    let target_defs: Vec<(&str, &str, &str)> = vec![
        ("serde_json", "serde_json", "baseline"),
        ("facet_format_jit", "facet-format+jit", "facet"),
        ("facet_format", "facet-format", "facet"),
        ("facet_json", "facet-json", "facet"),
    ];
    for (tidx, (key, label, kind)) in target_defs.iter().enumerate() {
        json.push_str(&format!("      \"{}\": {{\n", key));
        json.push_str(&format!("        \"key\": \"{}\",\n", key));
        json.push_str(&format!("        \"label\": \"{}\",\n", label));
        json.push_str(&format!("        \"kind\": \"{}\"\n", kind));
        json.push_str("      }");
        if tidx < target_defs.len() - 1 {
            json.push(',');
        }
        json.push('\n');
    }
    json.push_str("    },\n");

    // Metrics catalog
    json.push_str("    \"metrics\": {\n");
    let metric_defs: Vec<(&str, &str, &str, &str)> = vec![
        ("instructions", "Instructions", "count", "lower"),
        ("estimated_cycles", "Est. Cycles", "count", "lower"),
        ("time_median_ns", "Median Time", "ns", "lower"),
        ("l1_hits", "L1 Hits", "count", "lower"),
        ("ll_hits", "LL Hits", "count", "lower"),
        ("ram_hits", "RAM Hits", "count", "lower"),
        ("total_read_write", "Total R/W", "count", "lower"),
    ];
    for (midx, (key, label, unit, better)) in metric_defs.iter().enumerate() {
        json.push_str(&format!("      \"{}\": {{\n", key));
        json.push_str(&format!("        \"key\": \"{}\",\n", key));
        json.push_str(&format!("        \"label\": \"{}\",\n", label));
        json.push_str(&format!("        \"unit\": \"{}\",\n", unit));
        json.push_str(&format!("        \"better\": \"{}\"\n", better));
        json.push_str("      }");
        if midx < metric_defs.len() - 1 {
            json.push(',');
        }
        json.push('\n');
    }
    json.push_str("    }\n");

    json.push_str("  },\n");

    // === RESULTS ===
    // Structure: benchmark -> operation -> target -> metric -> value
    json.push_str("  \"results\": {\n");
    json.push_str("    \"values\": {\n");

    for (bidx, benchmark) in all_benchmarks.iter().enumerate() {
        json.push_str(&format!("      \"{}\": {{\n", benchmark));

        for (oidx, (op, op_name)) in [
            (parser::Operation::Deserialize, "deserialize"),
            (parser::Operation::Serialize, "serialize"),
        ]
        .iter()
        .enumerate()
        {
            json.push_str(&format!("        \"{}\": {{\n", op_name));

            for (tidx, target) in targets_order.iter().enumerate() {
                // Get divan timing
                let divan_time = data
                    .divan
                    .get(benchmark)
                    .and_then(|ops| ops.get(op))
                    .and_then(|targets| targets.get(*target));

                // Get gungraun metrics
                let gungraun_metrics = data
                    .gungraun
                    .get(benchmark)
                    .and_then(|ops| ops.get(op))
                    .and_then(|targets| targets.get(*target));

                if divan_time.is_some() || gungraun_metrics.is_some() {
                    json.push_str(&format!("          \"{}\": {{\n", target));

                    let mut metrics_written = 0;
                    if let Some(metrics) = gungraun_metrics {
                        json.push_str(&format!(
                            "            \"instructions\": {}",
                            metrics.instructions
                        ));
                        metrics_written += 1;
                        if let Some(v) = metrics.estimated_cycles {
                            json.push_str(&format!(",\n            \"estimated_cycles\": {}", v));
                            metrics_written += 1;
                        }
                        if let Some(time_ns) = divan_time {
                            json.push_str(&format!(
                                ",\n            \"time_median_ns\": {:.1}",
                                time_ns
                            ));
                            metrics_written += 1;
                        }
                        if let Some(v) = metrics.l1_hits {
                            json.push_str(&format!(",\n            \"l1_hits\": {}", v));
                            metrics_written += 1;
                        }
                        if let Some(v) = metrics.ll_hits {
                            json.push_str(&format!(",\n            \"ll_hits\": {}", v));
                            metrics_written += 1;
                        }
                        if let Some(v) = metrics.ram_hits {
                            json.push_str(&format!(",\n            \"ram_hits\": {}", v));
                            metrics_written += 1;
                        }
                        if let Some(v) = metrics.total_read_write {
                            json.push_str(&format!(",\n            \"total_read_write\": {}", v));
                            metrics_written += 1;
                        }
                    } else if let Some(time_ns) = divan_time {
                        json.push_str(&format!("            \"time_median_ns\": {:.1}", time_ns));
                        metrics_written += 1;
                    }

                    if metrics_written > 0 {
                        json.push('\n');
                    }
                    json.push_str("          }");
                } else {
                    json.push_str(&format!("          \"{}\": null", target));
                }

                if tidx < targets_order.len() - 1 {
                    json.push(',');
                }
                json.push('\n');
            }

            json.push_str("        }");
            if oidx < 1 {
                json.push(',');
            }
            json.push('\n');
        }

        json.push_str("      }");
        if bidx < bench_count - 1 {
            json.push(',');
        }
        json.push('\n');
    }
    json.push_str("    },\n");

    // Errors section
    json.push_str("    \"errors\": {\n");
    // Add any parse failures as errors
    if !divan_failures.is_empty() || !gungraun_failures.is_empty() {
        json.push_str("      \"_parse_failures\": {\n");
        json.push_str("        \"divan\": [");
        for (i, f) in divan_failures.iter().enumerate() {
            json.push_str(&format!("\"{}\"", escape_json(f)));
            if i < divan_failures.len() - 1 {
                json.push_str(", ");
            }
        }
        json.push_str("],\n");
        json.push_str("        \"gungraun\": [");
        for (i, f) in gungraun_failures.iter().enumerate() {
            json.push_str(&format!("\"{}\"", escape_json(f)));
            if i < gungraun_failures.len() - 1 {
                json.push_str(", ");
            }
        }
        json.push_str("]\n");
        json.push_str("      }\n");
    }
    json.push_str("    }\n");
    json.push_str("  }\n");

    json.push_str("}\n");

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
            "unified_benchmarks_divan",
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
        if !run_benchmark_with_progress(
            &workspace_root,
            "unified_benchmarks_gungraun",
            &gungraun_file,
            "🔬 Running gungraun (instruction counts)",
            args.filter.as_deref(),
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

    println!(
        "   Parsed {} divan results, {} gungraun results",
        divan_parsed.results.len(),
        gungraun_parsed.results.len()
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

    let data = parser::combine_results(divan_parsed.results, gungraun_parsed.results);

    // Export gungraun instruction counts to JSON (for perf delta tracking)
    export_perf_json(&data, &report_dir, &timestamp);

    // Load ordered benchmark definitions from KDL
    let ordered_benchmarks = benchmark_defs::load_ordered_benchmarks(&workspace_root);
    let total_benchmarks: usize = ordered_benchmarks.1.values().map(|v| v.len()).sum();
    println!(
        "   Loaded {} benchmark definitions in {} sections",
        total_benchmarks,
        ordered_benchmarks.0.len()
    );

    // Get git info
    let commit_full = get_git_output(&["rev-parse", "HEAD"]);
    let commit_short = get_git_output(&["rev-parse", "--short", "HEAD"]);
    let commit_message = get_git_output(&["log", "-1", "--format=%s"]);

    // Try to get PR info from CI environment variables
    // GitHub Actions: GITHUB_PR_NUMBER, GITHUB_PR_TITLE (or from GITHUB_EVENT_NAME/GITHUB_REF)
    // GitLab CI: CI_MERGE_REQUEST_IID, CI_MERGE_REQUEST_TITLE
    let pr_number = std::env::var("GITHUB_PR_NUMBER")
        .or_else(|_| std::env::var("CI_MERGE_REQUEST_IID"))
        .ok()
        .filter(|s| !s.is_empty());
    let pr_title = std::env::var("GITHUB_PR_TITLE")
        .or_else(|_| std::env::var("CI_MERGE_REQUEST_TITLE"))
        .ok()
        .filter(|s| !s.is_empty());

    let git_info = report::GitInfo {
        commit: commit_full,
        commit_short,
        branch: get_git_output(&["branch", "--show-current"]),
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

    println!();
    println!("✅ Benchmark data exported to run.json");
    println!();

    // Handle --index: clone perf repo, copy reports, generate index
    if args.index {
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
    cmd.args([
        "bench",
        "--bench",
        bench_name,
        "--features",
        "cranelift",
        "--features",
        "jit",
    ]);

    // Add filter if provided (passed after --)
    if let Some(f) = filter {
        cmd.arg("--").arg(f);
    }

    cmd.current_dir(workspace_root.join("facet-json"));

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
