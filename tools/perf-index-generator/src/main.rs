//! Generate index-v2.json for perf.facet.rs from benchmark results
//!
//! This tool scans a directory tree of benchmark results and generates:
//! - index.html: Minimal shell (app loads from index-v2.json)
//! - index-v2.json: Commit-centric navigation data with CommitSummary
//!
//! Expected directory layout:
//!   runs/{branch_key}/{commit_sha}/run.json

mod types;

use chrono::{DateTime, Utc};
use indexmap::IndexMap;
use maud::{DOCTYPE, Markup, html};
use std::collections::{BTreeMap, HashMap};
use std::fs;
use std::path::{Path, PathBuf};

/// Per-benchmark metrics for computing highlights
#[derive(Debug, Clone, Default)]
struct BenchmarkMetrics {
    /// serde_json instructions for deserialize
    serde_instructions: u64,
    /// facet_format_jit instructions for deserialize
    facet_instructions: u64,
}

/// Parsed run.json data (only what we need for the index)
#[derive(Debug)]
struct RunInfo {
    /// Branch key (directory name)
    branch_key: String,
    /// Original branch name
    branch_original: Option<String>,
    /// Commit SHA
    commit: String,
    /// Short commit SHA
    commit_short: String,
    /// ISO timestamp
    timestamp: String,
    /// Unix timestamp
    timestamp_unix: i64,
    /// PR number (if applicable)
    pr_number: Option<String>,
    /// Commit subject (first line of message or PR title)
    subject: String,
    /// Full commit message
    commit_message: String,
    /// PR title (if applicable)
    pr_title: Option<String>,
    /// Headline: sum of serde_json instructions for deserialize
    serde_sum: u64,
    /// Headline: sum of facet_format_jit instructions for deserialize
    facet_sum: u64,
    /// Per-benchmark metrics for computing highlights
    benchmarks: IndexMap<String, BenchmarkMetrics>,
}

#[derive(Debug)]
struct Args {
    perf_dir: PathBuf,
}

impl Args {
    fn from_args() -> Result<Self, String> {
        let mut args = std::env::args().skip(1);
        let perf_dir = args
            .next()
            .ok_or_else(|| "Usage: perf-index-generator <perf-directory>".to_string())?;
        Ok(Self {
            perf_dir: PathBuf::from(perf_dir),
        })
    }
}

fn main() {
    let args = match Args::from_args() {
        Ok(args) => args,
        Err(e) => {
            eprintln!("Error: {}", e);
            std::process::exit(1);
        }
    };

    if let Err(e) = run(&args.perf_dir) {
        eprintln!("Error: {}", e);
        std::process::exit(1);
    }
}

fn run(perf_dir: &Path) -> Result<(), Box<dyn std::error::Error>> {
    println!("Scanning {}...", perf_dir.display());

    // Collect all runs
    let runs = collect_runs(perf_dir)?;

    println!("Found {} runs", runs.len());

    // Generate index.html (minimal shell)
    let index_html = generate_index_shell();
    fs::write(perf_dir.join("index.html"), index_html.into_string())?;

    // Generate index-v2.json with commit-centric structure
    let index_json = generate_index_v2(&runs);
    fs::write(perf_dir.join("index-v2.json"), index_json)?;

    // Also write to index.json for backward compatibility during transition
    let index_json = generate_index_v2(&runs);
    fs::write(perf_dir.join("index.json"), index_json)?;

    println!("✅ Generated index.html and index-v2.json");

    Ok(())
}

/// Scan the runs directory and collect all run.json files
fn collect_runs(perf_dir: &Path) -> Result<Vec<RunInfo>, Box<dyn std::error::Error>> {
    let mut runs = Vec::new();

    let runs_dir = perf_dir.join("runs");
    if !runs_dir.exists() {
        // Fall back to old layout for backward compatibility during transition
        return collect_runs_old_layout(perf_dir);
    }

    // Scan runs/{branch_key}/{commit_sha}/run.json
    for branch_entry in fs::read_dir(&runs_dir)? {
        let branch_entry = branch_entry?;
        let branch_path = branch_entry.path();

        if !branch_path.is_dir() {
            continue;
        }

        let branch_key = branch_path
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("")
            .to_string();

        if branch_key.is_empty() {
            continue;
        }

        // Scan commits in this branch
        for commit_entry in fs::read_dir(&branch_path)? {
            let commit_entry = commit_entry?;
            let commit_path = commit_entry.path();

            if !commit_path.is_dir() {
                continue;
            }

            let commit_sha = commit_path
                .file_name()
                .and_then(|n| n.to_str())
                .unwrap_or("")
                .to_string();

            // Skip symlinks like "latest"
            if commit_sha == "latest" || commit_sha.is_empty() {
                continue;
            }

            // Read run.json
            let run_json_path = commit_path.join("run.json");
            if !run_json_path.exists() {
                continue;
            }

            match parse_run_json(&run_json_path, &branch_key) {
                Ok(run_info) => runs.push(run_info),
                Err(e) => {
                    eprintln!(
                        "Warning: Failed to parse {}: {}",
                        run_json_path.display(),
                        e
                    );
                }
            }
        }
    }

    Ok(runs)
}

/// Fall back to old layout for backward compatibility
/// Old layout: {branch_name}/{commit_sha}/metadata.json + perf-data-*.json
fn collect_runs_old_layout(perf_dir: &Path) -> Result<Vec<RunInfo>, Box<dyn std::error::Error>> {
    let mut runs = Vec::new();

    for branch_entry in fs::read_dir(perf_dir)? {
        let branch_entry = branch_entry?;
        let branch_path = branch_entry.path();

        if !branch_path.is_dir() {
            continue;
        }

        let branch_key = branch_path
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("")
            .to_string();

        // Skip special directories and files
        if branch_key.is_empty()
            || branch_key == "fonts"
            || branch_key == "runs"
            || branch_key.ends_with(".html")
            || branch_key.ends_with(".json")
            || branch_key.ends_with(".js")
            || branch_key.ends_with(".css")
        {
            continue;
        }

        // Scan commits in this branch
        for commit_entry in fs::read_dir(&branch_path)? {
            let commit_entry = commit_entry?;
            let commit_path = commit_entry.path();

            if !commit_path.is_dir() {
                continue;
            }

            let commit_sha = commit_path
                .file_name()
                .and_then(|n| n.to_str())
                .unwrap_or("")
                .to_string();

            if commit_sha == "latest" || commit_sha.is_empty() {
                continue;
            }

            // Check for run.json first (new format)
            let run_json_path = commit_path.join("run.json");
            if run_json_path.exists() {
                match parse_run_json(&run_json_path, &branch_key) {
                    Ok(run_info) => {
                        runs.push(run_info);
                        continue;
                    }
                    Err(e) => {
                        eprintln!(
                            "Warning: Failed to parse {}: {}",
                            run_json_path.display(),
                            e
                        );
                    }
                }
            }

            // Fall back to metadata.json (old format)
            let metadata_path = commit_path.join("metadata.json");
            if metadata_path.exists() {
                match parse_old_metadata(&metadata_path, &branch_key) {
                    Ok(run_info) => runs.push(run_info),
                    Err(e) => {
                        eprintln!(
                            "Warning: Failed to parse {}: {}",
                            metadata_path.display(),
                            e
                        );
                    }
                }
            }
        }
    }

    Ok(runs)
}

/// Parse run.json into RunInfo
fn parse_run_json(path: &Path, branch_key: &str) -> Result<RunInfo, Box<dyn std::error::Error>> {
    let json_str = fs::read_to_string(path)?;

    // Manual JSON parsing since run.json has nested structure
    // Extract just what we need for the index
    // Support both old schema (commit, commit_short, generated_at) and new schema (sha, short, timestamp)
    let _run_id = extract_json_string(&json_str, "run_id").unwrap_or_default();
    let commit = extract_json_string(&json_str, "sha")
        .or_else(|| extract_json_string(&json_str, "commit"))
        .unwrap_or_default();
    let commit_short = extract_json_string(&json_str, "short")
        .or_else(|| extract_json_string(&json_str, "commit_short"))
        .unwrap_or_default();
    let timestamp = extract_json_string(&json_str, "timestamp")
        .or_else(|| extract_json_string(&json_str, "generated_at"))
        .unwrap_or_default();
    let branch_original = extract_json_string(&json_str, "branch_original");
    let pr_number = extract_json_string(&json_str, "pr_number");

    // Try to get timestamp_unix directly from JSON (new schema), otherwise parse from ISO
    let timestamp_unix = extract_json_number(&json_str, "timestamp_unix")
        .unwrap_or_else(|| parse_iso_timestamp(&timestamp));

    // For subject, prefer PR title if available, else first line of commit message
    let commit_message = extract_json_string(&json_str, "commit_message").unwrap_or_default();
    let pr_title = extract_json_string(&json_str, "pr_title");
    let subject = pr_title
        .clone()
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| commit_message.lines().next().unwrap_or("").to_string());

    // Extract instruction data for headline and highlights computation
    let metrics = extract_metrics(&json_str);

    Ok(RunInfo {
        branch_key: branch_key.to_string(),
        branch_original,
        commit,
        commit_short,
        timestamp,
        timestamp_unix,
        pr_number,
        subject,
        commit_message,
        pr_title,
        serde_sum: metrics.serde_sum,
        facet_sum: metrics.facet_sum,
        benchmarks: metrics.benchmarks,
    })
}

/// Extracted metrics from run.json
struct ExtractedMetrics {
    serde_sum: u64,
    facet_sum: u64,
    benchmarks: IndexMap<String, BenchmarkMetrics>,
}

/// Extract instruction data from run.json for headline and highlights computation
/// Parses the run-v1 schema using facet_json
fn extract_metrics(json_str: &str) -> ExtractedMetrics {
    use benchmark_analyzer::run_types::RunJsonMinimal;

    let mut result = ExtractedMetrics {
        serde_sum: 0,
        facet_sum: 0,
        benchmarks: IndexMap::new(),
    };

    // Parse the run.json with facet_json (minimal struct for compatibility)
    let run: RunJsonMinimal = match facet_json::from_str(json_str) {
        Ok(r) => r,
        Err(e) => {
            eprintln!("Warning: Failed to parse run.json: {}", e);
            return result;
        }
    };

    // Extract metrics from results.values (new schema only)
    // Old schema files will have values: None and return empty metrics
    let values = match &run.results.values {
        Some(v) => v,
        None => return result, // Old schema - no metrics extracted
    };

    for (bench_name, bench_ops) in values {
        let mut metrics = BenchmarkMetrics::default();

        // Get deserialize metrics for serde_json and facet_format_jit
        let serde_instructions = bench_ops
            .deserialize
            .get("serde_json")
            .and_then(|o| o.as_ref())
            .and_then(|m| m.instructions);

        let facet_instructions = bench_ops
            .deserialize
            .get("facet_format_jit")
            .and_then(|o| o.as_ref())
            .and_then(|m| m.instructions);

        // Only include in sums if BOTH targets have data (apples-to-apples comparison)
        // This avoids skewing the ratio when one target crashes/fails on certain benchmarks
        if let (Some(serde), Some(facet)) = (serde_instructions, facet_instructions) {
            metrics.serde_instructions = serde;
            metrics.facet_instructions = facet;
            result.serde_sum += serde;
            result.facet_sum += facet;
            result.benchmarks.insert(bench_name.clone(), metrics);
        }
    }

    result
}

/// Parse old metadata.json into RunInfo
fn parse_old_metadata(
    path: &Path,
    branch_key: &str,
) -> Result<RunInfo, Box<dyn std::error::Error>> {
    let json_str = fs::read_to_string(path)?;
    let metadata: types::CommitMetadata = facet_json::from_str(&json_str)?;

    let timestamp_unix = parse_iso_timestamp(&metadata.timestamp);

    // For subject, prefer PR title if non-empty, else first line of commit message
    let subject = if !metadata.pr_title.is_empty() {
        metadata.pr_title.clone()
    } else {
        metadata
            .commit_message
            .lines()
            .next()
            .unwrap_or("")
            .to_string()
    };

    Ok(RunInfo {
        branch_key: branch_key.to_string(),
        branch_original: Some(metadata.branch_original),
        commit: metadata.commit,
        commit_short: metadata.commit_short,
        timestamp: metadata.timestamp,
        timestamp_unix,
        pr_number: metadata.pr_number,
        subject,
        commit_message: metadata.commit_message,
        pr_title: if metadata.pr_title.is_empty() {
            None
        } else {
            Some(metadata.pr_title)
        },
        // Old metadata format doesn't have instruction data
        serde_sum: 0,
        facet_sum: 0,
        benchmarks: IndexMap::new(),
    })
}

/// Extract a string value from JSON (simple regex-like approach)
fn extract_json_string(json: &str, key: &str) -> Option<String> {
    let pattern = format!("\"{}\"", key);
    let start = json.find(&pattern)?;
    let after_key = &json[start + pattern.len()..];

    // Skip whitespace and colon
    let after_colon = after_key.trim_start().strip_prefix(':')?;
    let after_colon = after_colon.trim_start();

    if after_colon.starts_with("null") {
        return None;
    }

    // Find the opening quote
    let value_start = after_colon.strip_prefix('"')?;

    // Find closing quote (handle escapes)
    let mut chars = value_start.chars().peekable();
    let mut result = String::new();
    while let Some(c) = chars.next() {
        if c == '\\' {
            // Handle escape sequences
            match chars.next() {
                Some('n') => result.push('\n'),
                Some('r') => result.push('\r'),
                Some('t') => result.push('\t'),
                Some('"') => result.push('"'),
                Some('\\') => result.push('\\'),
                Some(c) => {
                    result.push('\\');
                    result.push(c);
                }
                None => break,
            }
        } else if c == '"' {
            break;
        } else {
            result.push(c);
        }
    }

    Some(result)
}

/// Extract a number value from JSON (for integer fields like timestamp_unix)
fn extract_json_number(json: &str, key: &str) -> Option<i64> {
    let pattern = format!("\"{}\"", key);
    let start = json.find(&pattern)?;
    let after_key = &json[start + pattern.len()..];

    // Skip whitespace and colon
    let after_colon = after_key.trim_start().strip_prefix(':')?;
    let after_colon = after_colon.trim_start();

    // Read digits (and optional leading minus)
    let num_str: String = after_colon
        .chars()
        .take_while(|c| c.is_ascii_digit() || *c == '-')
        .collect();
    num_str.parse().ok()
}

/// Parse ISO 8601 timestamp to Unix epoch
fn parse_iso_timestamp(iso: &str) -> i64 {
    DateTime::parse_from_rfc3339(iso)
        .map(|dt| dt.timestamp())
        .unwrap_or(0)
}

fn generate_index_shell() -> Markup {
    html! {
        (DOCTYPE)
        html {
            head {
                meta charset="UTF-8";
                title { "facet benchmarks" }
                link rel="icon" href="/favicon.png" sizes="32x32" type="image/png";
                link rel="icon" href="/favicon.ico" type="image/x-icon";
                link rel="apple-touch-icon" href="/favicon.png";
                link rel="stylesheet" href="/shared-styles.css";
                script type="module" src="/app.js" {}
            }
            body {
                div #app {
                    div style="text-align: center; padding: 4em 1em; color: var(--muted);" {
                        "Loading..."
                    }
                }
            }
        }
    }
}

/// Escape a string for JSON
fn escape_json(s: &str) -> String {
    s.chars()
        .flat_map(|c| match c {
            '"' => vec!['\\', '"'],
            '\\' => vec!['\\', '\\'],
            '\n' => vec!['\\', 'n'],
            '\r' => vec!['\\', 'r'],
            '\t' => vec!['\\', 't'],
            c if c.is_control() => format!("\\u{:04x}", c as u32).chars().collect(),
            c => vec![c],
        })
        .collect()
}

fn generate_index_v2(runs: &[RunInfo]) -> String {
    // Build commit-centric structures
    let mut commits: HashMap<String, CommitData> = HashMap::new();
    let mut branches: BTreeMap<String, BranchData> = BTreeMap::new();

    // First pass: collect all data
    for run in runs {
        // Update or create commit entry
        let commit_data = commits
            .entry(run.commit.clone())
            .or_insert_with(|| CommitData {
                sha: run.commit.clone(),
                short: run.commit_short.clone(),
                subject: run.subject.clone(),
                timestamp_unix: run.timestamp_unix,
                branches_present: Vec::new(),
                runs: HashMap::new(),
                serde_sum: 0,
                facet_sum: 0,
                benchmarks: IndexMap::new(),
            });

        // Update timestamp to be the canonical one (prefer main branch timestamp)
        if run.branch_key == "main" || commit_data.timestamp_unix == 0 {
            commit_data.timestamp_unix = run.timestamp_unix;
        }

        // Update headline sums and benchmarks (prefer main branch data, or use first available)
        if run.branch_key == "main" || commit_data.serde_sum == 0 {
            commit_data.serde_sum = run.serde_sum;
            commit_data.facet_sum = run.facet_sum;
            commit_data.benchmarks = run.benchmarks.clone();
        }

        // Add branch to branches_present if not already there
        if !commit_data.branches_present.contains(&run.branch_key) {
            commit_data.branches_present.push(run.branch_key.clone());
        }

        // Add run entry
        commit_data.runs.insert(
            run.branch_key.clone(),
            RunEntry {
                branch_key: run.branch_key.clone(),
                branch_original: run.branch_original.clone(),
                pr_number: run.pr_number.clone(),
                pr_title: run.pr_title.clone(),
                timestamp: run.timestamp.clone(),
                commit_message: run.commit_message.clone(),
            },
        );

        // Update branch data
        let branch_data = branches
            .entry(run.branch_key.clone())
            .or_insert_with(|| BranchData {
                key: run.branch_key.clone(),
                display: compute_branch_display(&run.branch_key, run.pr_number.as_deref()),
                kind: compute_branch_kind(&run.branch_key, &run.branch_original),
                branch_original: run.branch_original.clone(),
                pr_number: run.pr_number.clone(),
                last_timestamp: run.timestamp.clone(),
                commits: Vec::new(),
            });

        // Update last_timestamp if this run is newer
        if run.timestamp > branch_data.last_timestamp {
            branch_data.last_timestamp = run.timestamp.clone();
        }
    }

    // Second pass: build branch_commits lists
    for run in runs {
        if let Some(branch) = branches.get_mut(&run.branch_key) {
            // Check if we already have this commit in this branch
            if !branch.commits.iter().any(|c| c.sha == run.commit) {
                // Find parent SHA (previous commit in the same branch by timestamp)
                let parent_sha = find_parent_commit(runs, &run.branch_key, run.timestamp_unix);

                branch.commits.push(BranchCommitEntry {
                    sha: run.commit.clone(),
                    short: run.commit_short.clone(),
                    timestamp_unix: run.timestamp_unix,
                    parent_sha,
                    serde_sum: run.serde_sum,
                    facet_sum: run.facet_sum,
                });
            }
        }
    }

    // Sort branch commits by timestamp (newest first)
    for branch in branches.values_mut() {
        branch
            .commits
            .sort_by(|a, b| b.timestamp_unix.cmp(&a.timestamp_unix));
    }

    // Find baseline (latest main commit) with per-benchmark data for highlights
    let baseline = branches.get("main").and_then(|main| {
        main.commits.first().map(|c| {
            // Get benchmarks from the commits data
            let benchmarks = commits
                .get(&c.sha)
                .map(|cd| cd.benchmarks.clone())
                .unwrap_or_default();
            BaselineData {
                name: "main tip".to_string(),
                branch_key: "main".to_string(),
                commit_sha: c.sha.clone(),
                commit_short: c.short.clone(),
                timestamp: main.last_timestamp.clone(),
                serde_sum: c.serde_sum,
                facet_sum: c.facet_sum,
                benchmarks,
            }
        })
    });

    // Build JSON
    let mut json = String::from("{\n");

    // Version
    json.push_str("  \"version\": 2,\n");

    // Generated timestamp
    let now = Utc::now().to_rfc3339();
    json.push_str(&format!("  \"generated_at\": \"{}\",\n", now));

    // Repo
    json.push_str("  \"repo\": \"facet-rs/facet\",\n");

    // Metric specs
    json.push_str("  \"metric_specs\": {\n");
    json.push_str("    \"instructions\": { \"label\": \"Instructions\", \"unit\": \"count\", \"better\": \"lower\", \"format\": \"int\", \"source\": \"gungraun\" },\n");
    json.push_str("    \"time_median_ns\": { \"label\": \"Time (median)\", \"unit\": \"ns\", \"better\": \"lower\", \"format\": \"int\", \"source\": \"divan\" },\n");
    json.push_str("    \"l1_hits\": { \"label\": \"L1 Cache Hits\", \"unit\": \"count\", \"better\": \"higher\", \"format\": \"int\", \"source\": \"gungraun\" },\n");
    json.push_str("    \"ll_hits\": { \"label\": \"LL Cache Hits\", \"unit\": \"count\", \"better\": \"lower\", \"format\": \"int\", \"source\": \"gungraun\" },\n");
    json.push_str("    \"ram_hits\": { \"label\": \"RAM Hits\", \"unit\": \"count\", \"better\": \"lower\", \"format\": \"int\", \"source\": \"gungraun\" },\n");
    json.push_str("    \"estimated_cycles\": { \"label\": \"Est. Cycles\", \"unit\": \"count\", \"better\": \"lower\", \"format\": \"int\", \"source\": \"gungraun\" }\n");
    json.push_str("  },\n");

    // Defaults
    json.push_str("  \"defaults\": {\n");
    json.push_str("    \"index_metric\": \"instructions\",\n");
    json.push_str("    \"index_operation\": \"deserialize\",\n");
    json.push_str("    \"baseline_target\": \"serde_json\",\n");
    json.push_str("    \"headline_target\": \"facet_format_jit\",\n");
    json.push_str("    \"ratio_mode\": \"speedup\",\n");
    json.push_str("    \"max_commits_default\": 50\n");
    json.push_str("  },\n");

    // Baseline
    if let Some(ref b) = baseline {
        let baseline_ratio = if b.facet_sum > 0 {
            b.serde_sum as f64 / b.facet_sum as f64
        } else {
            0.0
        };
        json.push_str("  \"baseline\": {\n");
        json.push_str(&format!("    \"name\": \"{}\",\n", b.name));
        json.push_str(&format!("    \"branch_key\": \"{}\",\n", b.branch_key));
        json.push_str(&format!("    \"commit_sha\": \"{}\",\n", b.commit_sha));
        json.push_str(&format!("    \"commit_short\": \"{}\",\n", b.commit_short));
        json.push_str("    \"operation\": \"deserialize\",\n");
        json.push_str("    \"metric\": \"instructions\",\n");
        json.push_str("    \"baseline_target\": \"serde_json\",\n");
        json.push_str("    \"headline_target\": \"facet_format_jit\",\n");
        json.push_str(&format!("    \"timestamp\": \"{}\",\n", b.timestamp));
        json.push_str(&format!(
            "    \"run_json_url\": \"/runs/{}/{}/run.json\",\n",
            b.branch_key, b.commit_sha
        ));
        // Add headline data for baseline
        json.push_str("    \"headline\": {\n");
        json.push_str(&format!("      \"serde_sum\": {},\n", b.serde_sum));
        json.push_str(&format!("      \"facet_sum\": {},\n", b.facet_sum));
        json.push_str(&format!("      \"ratio\": {:.4}\n", baseline_ratio));
        json.push_str("    }\n");
        json.push_str("  },\n");
    } else {
        json.push_str("  \"baseline\": null,\n");
    }

    // Timeline: all commits sorted by timestamp (newest first)
    let mut timeline: Vec<(&String, i64)> = commits
        .iter()
        .map(|(sha, c)| (sha, c.timestamp_unix))
        .collect();
    timeline.sort_by(|a, b| b.1.cmp(&a.1)); // newest first

    json.push_str("  \"timeline\": [");
    for (idx, (sha, _)) in timeline.iter().enumerate() {
        json.push_str(&format!("\"{}\"", sha));
        if idx < timeline.len() - 1 {
            json.push_str(", ");
        }
    }
    json.push_str("],\n");

    // Branches
    json.push_str("  \"branches\": {\n");
    let branch_keys: Vec<_> = branches.keys().collect();
    for (idx, key) in branch_keys.iter().enumerate() {
        let branch = &branches[*key];
        json.push_str(&format!("    \"{}\": {{\n", key));
        json.push_str(&format!("      \"key\": \"{}\",\n", branch.key));
        json.push_str(&format!(
            "      \"display\": \"{}\",\n",
            escape_json(&branch.display)
        ));
        json.push_str(&format!("      \"kind\": \"{}\",\n", branch.kind));
        if let Some(ref orig) = branch.branch_original {
            json.push_str(&format!(
                "      \"branch_original\": \"{}\",\n",
                escape_json(orig)
            ));
        }
        if let Some(ref pr) = branch.pr_number {
            json.push_str(&format!("      \"pr_number\": \"{}\",\n", pr));
        }
        json.push_str(&format!(
            "      \"last_timestamp\": \"{}\"\n",
            branch.last_timestamp
        ));
        json.push_str("    }");
        if idx < branch_keys.len() - 1 {
            json.push(',');
        }
        json.push('\n');
    }
    json.push_str("  },\n");

    // Branch commits
    json.push_str("  \"branch_commits\": {\n");
    for (idx, key) in branch_keys.iter().enumerate() {
        let branch = &branches[*key];
        json.push_str(&format!("    \"{}\": [\n", key));
        for (cidx, commit) in branch.commits.iter().enumerate() {
            json.push_str("      {\n");
            json.push_str(&format!("        \"sha\": \"{}\",\n", commit.sha));
            json.push_str(&format!("        \"short\": \"{}\",\n", commit.short));
            json.push_str(&format!(
                "        \"timestamp_unix\": {},\n",
                commit.timestamp_unix
            ));
            if let Some(ref parent) = commit.parent_sha {
                json.push_str(&format!("        \"parent_sha\": \"{}\",\n", parent));
            } else {
                json.push_str("        \"parent_sha\": null,\n");
            }
            json.push_str(&format!(
                "        \"run_json_url\": \"/runs/{}/{}/run.json\"\n",
                key, commit.sha
            ));
            json.push_str("      }");
            if cidx < branch.commits.len() - 1 {
                json.push(',');
            }
            json.push('\n');
        }
        json.push_str("    ]");
        if idx < branch_keys.len() - 1 {
            json.push(',');
        }
        json.push('\n');
    }
    json.push_str("  },\n");

    // Commits
    json.push_str("  \"commits\": {\n");
    let mut commit_shas: Vec<_> = commits.keys().collect();
    commit_shas.sort();
    for (idx, sha) in commit_shas.iter().enumerate() {
        let commit = &commits[*sha];
        json.push_str(&format!("    \"{}\": {{\n", sha));
        json.push_str(&format!("      \"sha\": \"{}\",\n", commit.sha));
        json.push_str(&format!("      \"short\": \"{}\",\n", commit.short));
        json.push_str(&format!(
            "      \"subject\": \"{}\",\n",
            escape_json(&commit.subject)
        ));
        json.push_str(&format!(
            "      \"timestamp_unix\": {},\n",
            commit.timestamp_unix
        ));

        // branches_present
        json.push_str("      \"branches_present\": [");
        for (bidx, branch) in commit.branches_present.iter().enumerate() {
            json.push_str(&format!("\"{}\"", branch));
            if bidx < commit.branches_present.len() - 1 {
                json.push_str(", ");
            }
        }
        json.push_str("],\n");

        // primary_default
        let primary = if commit.branches_present.contains(&"main".to_string()) {
            "main"
        } else {
            commit
                .branches_present
                .first()
                .map(|s| s.as_str())
                .unwrap_or("main")
        };
        json.push_str(&format!(
            "      \"primary_default\": {{ \"branch_key\": \"{}\" }},\n",
            primary
        ));

        // headline: pre-computed ratio for index display
        // ratio = serde_sum / facet_sum (how many times faster facet is vs serde)
        let ratio = if commit.facet_sum > 0 {
            commit.serde_sum as f64 / commit.facet_sum as f64
        } else {
            0.0
        };

        // Compute delta vs baseline
        let (delta_vs_baseline, delta_direction) = if let Some(ref b) = baseline {
            let baseline_ratio = if b.facet_sum > 0 {
                b.serde_sum as f64 / b.facet_sum as f64
            } else {
                0.0
            };
            if baseline_ratio > 0.0 && ratio > 0.0 {
                // delta = (current / baseline - 1) * 100
                // Positive = current is faster (better), Negative = current is slower (worse)
                let delta = (ratio / baseline_ratio - 1.0) * 100.0;
                let direction = if delta > 0.5 {
                    "better"
                } else if delta < -0.5 {
                    "worse"
                } else {
                    "same"
                };
                (Some(delta), Some(direction))
            } else {
                (None, None)
            }
        } else {
            (None, None)
        };

        // Compute highlights (regressions/improvements vs baseline)
        let (regressions, improvements) = if let Some(ref b) = baseline {
            compute_highlights(&commit.benchmarks, &b.benchmarks, 5)
        } else {
            (Vec::new(), Vec::new())
        };

        // Output summary section
        json.push_str("      \"summary\": {\n");
        json.push_str("        \"headline\": {\n");
        json.push_str("          \"metric\": \"instructions\",\n");
        json.push_str("          \"operation\": \"deserialize\",\n");
        json.push_str(&format!("          \"serde_sum\": {},\n", commit.serde_sum));
        json.push_str(&format!("          \"facet_sum\": {},\n", commit.facet_sum));
        json.push_str(&format!("          \"ratio\": {:.4}", ratio));
        if let Some(delta) = delta_vs_baseline {
            json.push_str(&format!(",\n          \"delta_vs_baseline\": {:.2}", delta));
        }
        if let Some(direction) = delta_direction {
            json.push_str(&format!(
                ",\n          \"delta_direction\": \"{}\"",
                direction
            ));
        }
        json.push_str("\n        },\n");

        // Highlights
        json.push_str("        \"highlights\": {\n");

        // Regressions
        json.push_str("          \"regressions\": [");
        for (ridx, reg) in regressions.iter().enumerate() {
            if ridx > 0 {
                json.push_str(", ");
            }
            json.push_str(&format!(
                "{{\"benchmark\": \"{}\", \"delta_percent\": {:.2}, \"current_ratio\": {:.4}, \"baseline_ratio\": {:.4}}}",
                reg.benchmark, reg.delta_percent, reg.current_ratio, reg.baseline_ratio
            ));
        }
        json.push_str("],\n");

        // Improvements
        json.push_str("          \"improvements\": [");
        for (iidx, imp) in improvements.iter().enumerate() {
            if iidx > 0 {
                json.push_str(", ");
            }
            json.push_str(&format!(
                "{{\"benchmark\": \"{}\", \"delta_percent\": {:.2}, \"current_ratio\": {:.4}, \"baseline_ratio\": {:.4}}}",
                imp.benchmark, imp.delta_percent, imp.current_ratio, imp.baseline_ratio
            ));
        }
        json.push_str("]\n");

        json.push_str("        },\n");

        // Status
        json.push_str("        \"status\": {\n");
        json.push_str(&format!(
            "          \"incomplete\": {}\n",
            commit.benchmarks.is_empty()
        ));
        json.push_str("        }\n");
        json.push_str("      },\n");

        // runs
        json.push_str("      \"runs\": {\n");
        let run_keys: Vec<_> = commit.runs.keys().collect();
        for (ridx, run_key) in run_keys.iter().enumerate() {
            let run = &commit.runs[*run_key];
            json.push_str(&format!("        \"{}\": {{\n", run_key));
            json.push_str(&format!(
                "          \"branch_key\": \"{}\",\n",
                run.branch_key
            ));
            if let Some(ref orig) = run.branch_original {
                json.push_str(&format!(
                    "          \"branch_original\": \"{}\",\n",
                    escape_json(orig)
                ));
            }
            if let Some(ref pr) = run.pr_number {
                json.push_str(&format!("          \"pr_number\": \"{}\",\n", pr));
            }
            if let Some(ref pr_title) = run.pr_title {
                json.push_str(&format!(
                    "          \"pr_title\": \"{}\",\n",
                    escape_json(pr_title)
                ));
            }
            json.push_str(&format!(
                "          \"timestamp\": \"{}\",\n",
                run.timestamp
            ));
            json.push_str(&format!(
                "          \"commit_message\": \"{}\",\n",
                escape_json(&run.commit_message)
            ));
            json.push_str(&format!(
                "          \"run_json_url\": \"/runs/{}/{}/run.json\"\n",
                run.branch_key, commit.sha
            ));
            json.push_str("        }");
            if ridx < run_keys.len() - 1 {
                json.push(',');
            }
            json.push('\n');
        }
        json.push_str("      }\n");

        json.push_str("    }");
        if idx < commit_shas.len() - 1 {
            json.push(',');
        }
        json.push('\n');
    }
    json.push_str("  }\n");

    json.push_str("}\n");
    json
}

// Helper structs for building the index

/// A benchmark diff item for highlights
#[derive(Debug, Clone)]
struct DiffItem {
    benchmark: String,
    delta_percent: f64, // positive = regression (slower), negative = improvement (faster)
    current_ratio: f64,
    baseline_ratio: f64,
}

#[derive(Debug)]
struct CommitData {
    sha: String,
    short: String,
    subject: String,
    timestamp_unix: i64,
    branches_present: Vec<String>,
    runs: HashMap<String, RunEntry>,
    /// Headline: sum of serde instructions (from primary run)
    serde_sum: u64,
    /// Headline: sum of facet instructions (from primary run)
    facet_sum: u64,
    /// Per-benchmark metrics for computing highlights
    benchmarks: IndexMap<String, BenchmarkMetrics>,
}

#[derive(Debug)]
struct RunEntry {
    branch_key: String,
    branch_original: Option<String>,
    pr_number: Option<String>,
    pr_title: Option<String>,
    timestamp: String,
    commit_message: String,
}

#[derive(Debug)]
struct BranchData {
    key: String,
    display: String,
    kind: String,
    branch_original: Option<String>,
    pr_number: Option<String>,
    last_timestamp: String,
    commits: Vec<BranchCommitEntry>,
}

#[derive(Debug)]
struct BranchCommitEntry {
    sha: String,
    short: String,
    timestamp_unix: i64,
    parent_sha: Option<String>,
    serde_sum: u64,
    facet_sum: u64,
}

#[derive(Debug)]
struct BaselineData {
    name: String,
    branch_key: String,
    serde_sum: u64,
    facet_sum: u64,
    commit_sha: String,
    commit_short: String,
    timestamp: String,
    benchmarks: IndexMap<String, BenchmarkMetrics>,
}

/// Compute display name for a branch
fn compute_branch_display(branch_key: &str, pr_number: Option<&str>) -> String {
    if branch_key == "main" {
        return "main".to_string();
    }

    if let Some(pr) = pr_number {
        return format!("PR #{}", pr);
    }

    // For other branches, use the key as display
    branch_key.to_string()
}

/// Compute branch kind based on key and original name
fn compute_branch_kind(branch_key: &str, branch_original: &Option<String>) -> String {
    if branch_key == "main" {
        return "main".to_string();
    }

    let orig = branch_original.as_deref().unwrap_or(branch_key);

    if orig.starts_with("gh-readonly-queue/") {
        return "queue".to_string();
    }

    if orig.starts_with("renovate/") {
        return "renovate".to_string();
    }

    // Check if it looks like a PR (has pr_number or specific patterns)
    if orig.contains("/pr-") {
        return "pr".to_string();
    }

    "feature".to_string()
}

/// Find the parent commit (previous commit in the same branch)
fn find_parent_commit(runs: &[RunInfo], branch_key: &str, timestamp_unix: i64) -> Option<String> {
    // Find the most recent commit in this branch that's older than the current one
    runs.iter()
        .filter(|r| r.branch_key == branch_key && r.timestamp_unix < timestamp_unix)
        .max_by_key(|r| r.timestamp_unix)
        .map(|r| r.commit.clone())
}

/// Compute ratio for a benchmark (serde/facet = how many times faster facet is)
fn compute_ratio(metrics: &BenchmarkMetrics) -> Option<f64> {
    if metrics.facet_instructions > 0 {
        Some(metrics.serde_instructions as f64 / metrics.facet_instructions as f64)
    } else {
        None
    }
}

/// Compute highlights (regressions/improvements) comparing commit to baseline
/// Returns (regressions, improvements) as sorted `Vec<DiffItem>`
fn compute_highlights(
    commit_benchmarks: &IndexMap<String, BenchmarkMetrics>,
    baseline_benchmarks: &IndexMap<String, BenchmarkMetrics>,
    max_items: usize,
) -> (Vec<DiffItem>, Vec<DiffItem>) {
    let mut diffs: Vec<DiffItem> = Vec::new();

    for (bench_name, commit_metrics) in commit_benchmarks {
        if let Some(baseline_metrics) = baseline_benchmarks.get(bench_name) {
            let commit_ratio = match compute_ratio(commit_metrics) {
                Some(r) => r,
                None => continue,
            };
            let baseline_ratio = match compute_ratio(baseline_metrics) {
                Some(r) => r,
                None => continue,
            };

            // delta = (current_ratio / baseline_ratio - 1) * 100
            // Negative = regression (current is slower relative to serde)
            // Positive = improvement (current is faster relative to serde)
            // BUT we invert because higher ratio = better (faster than serde)
            // So if current_ratio < baseline_ratio, that's a regression
            let delta_percent = (commit_ratio / baseline_ratio - 1.0) * 100.0;

            // Invert for display: positive delta = regression (worse), negative = improvement
            let display_delta = -delta_percent;

            diffs.push(DiffItem {
                benchmark: bench_name.clone(),
                delta_percent: display_delta,
                current_ratio: commit_ratio,
                baseline_ratio,
            });
        }
    }

    // Sort by delta: regressions (positive) first, improvements (negative) last
    diffs.sort_by(|a, b| {
        b.delta_percent
            .partial_cmp(&a.delta_percent)
            .unwrap_or(std::cmp::Ordering::Equal)
    });

    // Split into regressions (positive delta) and improvements (negative delta)
    let regressions: Vec<DiffItem> = diffs
        .iter()
        .filter(|d| d.delta_percent > 3.0) // >3% threshold - below this is statistically insignificant
        .take(max_items)
        .cloned()
        .collect();

    let improvements: Vec<DiffItem> = diffs
        .iter()
        .filter(|d| d.delta_percent < -3.0) // <-3% threshold - below this is statistically insignificant
        .rev() // Most improved first
        .take(max_items)
        .cloned()
        .collect();

    (regressions, improvements)
}
