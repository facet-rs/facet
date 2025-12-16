//! Generate index pages for perf.facet.rs from benchmark results
//!
//! This tool scans a directory tree of benchmark results and generates:
//! - index.html: Homepage with latest main + recent activity
//! - branches.html: All branches sorted by recency
//! - index.json: Navigation data for dropdowns

mod types;

use maud::{DOCTYPE, Markup, html};
use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};
use types::{CommitMetadata, PerfDataFile};

/// A single commit with its metadata and performance data
#[derive(Debug, Clone)]
struct CommitInfo {
    commit: String,
    commit_short: String,
    branch_original: String,
    pr_number: Option<String>,
    timestamp: String, // ISO 8601 format
    timestamp_display: String,
    timestamp_unix: i64,
    commit_message: String,
    pr_title: String,
    /// Total instruction count (if perf data available)
    total_instructions: Option<u64>,
    /// Ratio of facet-format-json+jit to serde_json (e.g., 0.85 = 85% of serde)
    facet_vs_serde_ratio: Option<f64>,
}

/// A branch with its commits
#[derive(Debug)]
struct BranchInfo {
    name: String,
    commits: Vec<CommitInfo>,
    latest_timestamp: i64,
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

    // Collect all branches and commits
    let branches = collect_branches(perf_dir)?;

    println!("Found {} branches", branches.len());

    // Generate index.html (minimal shell)
    let index_html = generate_index_shell();
    fs::write(perf_dir.join("index.html"), index_html.into_string())?;

    // Generate index.json with comprehensive data
    let index_json = generate_index_json(&branches);
    fs::write(perf_dir.join("index.json"), index_json)?;

    println!("✅ Generated index.html and index.json");

    Ok(())
}

/// Scan the perf directory and collect all branches and commits
fn collect_branches(perf_dir: &Path) -> Result<Vec<BranchInfo>, Box<dyn std::error::Error>> {
    let mut branches_map: HashMap<String, Vec<CommitInfo>> = HashMap::new();

    // Scan all directories
    for entry in fs::read_dir(perf_dir)? {
        let entry = entry?;
        let path = entry.path();

        if !path.is_dir() {
            continue;
        }

        let branch_name = path
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("")
            .to_string();

        // Skip special directories
        if branch_name == "fonts" || branch_name.is_empty() {
            continue;
        }

        // Scan commits in this branch
        for commit_entry in fs::read_dir(&path)? {
            let commit_entry = commit_entry?;
            let commit_path = commit_entry.path();

            if !commit_path.is_dir() {
                continue;
            }

            let commit_name = commit_path
                .file_name()
                .and_then(|n| n.to_str())
                .unwrap_or("")
                .to_string();

            // Skip symlinks like "latest"
            if commit_name == "latest" || commit_name.is_empty() {
                continue;
            }

            // Read metadata.json
            let metadata_path = commit_path.join("metadata.json");
            if !metadata_path.exists() {
                continue;
            }

            let metadata_json = fs::read_to_string(&metadata_path)?;
            let metadata: CommitMetadata = facet_json::from_str(&metadata_json)?;

            // Parse timestamp to Unix epoch
            let timestamp_unix = parse_iso_timestamp(&metadata.timestamp);

            // Read perf-data.json if available
            let (total_instructions, facet_vs_serde_ratio) = fs::read_dir(&commit_path)?
                .filter_map(|e| e.ok())
                .find(|entry| {
                    let name = entry.file_name();
                    let name_str = name.to_str().unwrap_or("");
                    name_str.starts_with("perf-data-") && name_str.ends_with(".json")
                })
                .and_then(|entry| {
                    let json = fs::read_to_string(entry.path()).ok()?;
                    let perf_data: PerfDataFile = facet_json::from_str(&json).ok()?;
                    Some((
                        Some(perf_data.total_instructions()),
                        perf_data.facet_vs_serde_ratio(),
                    ))
                })
                .unwrap_or((None, None));

            let commit_info = CommitInfo {
                commit: metadata.commit.clone(),
                commit_short: metadata.commit_short,
                branch_original: metadata.branch_original,
                pr_number: metadata.pr_number,
                timestamp: metadata.timestamp.clone(),
                timestamp_display: metadata.timestamp_display,
                timestamp_unix,
                commit_message: metadata.commit_message,
                pr_title: metadata.pr_title,
                total_instructions,
                facet_vs_serde_ratio,
            };

            branches_map
                .entry(branch_name.clone())
                .or_default()
                .push(commit_info);
        }
    }

    // Sort commits within each branch by timestamp (newest first)
    let mut branches: Vec<BranchInfo> = branches_map
        .into_iter()
        .map(|(name, mut commits)| {
            commits.sort_by(|a, b| b.timestamp_unix.cmp(&a.timestamp_unix));
            let latest_timestamp = commits.first().map(|c| c.timestamp_unix).unwrap_or(0);
            BranchInfo {
                name,
                commits,
                latest_timestamp,
            }
        })
        .collect();

    // Sort branches by latest timestamp (newest first), but main always first
    branches.sort_by(|a, b| {
        if a.name == "main" {
            std::cmp::Ordering::Less
        } else if b.name == "main" {
            std::cmp::Ordering::Greater
        } else {
            b.latest_timestamp.cmp(&a.latest_timestamp)
        }
    });

    Ok(branches)
}

/// Parse ISO 8601 timestamp to Unix epoch (best effort)
fn parse_iso_timestamp(iso: &str) -> i64 {
    use chrono::DateTime;
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
                    // Loading state
                    div style="text-align: center; padding: 4em 1em; color: var(--muted);" {
                        "Loading..."
                    }
                }
            }
        }
    }
}

/// Escape a string for JSON (handles quotes, backslashes, newlines, etc.)
fn escape_json(s: &str) -> String {
    s.chars()
        .flat_map(|c| match c {
            '"' => vec!['\\', '"'],
            '\\' => vec!['\\', '\\'],
            '\n' => vec!['\\', 'n'],
            '\r' => vec!['\\', 'r'],
            '\t' => vec!['\\', 't'],
            c if c.is_control() => {
                // Escape other control characters as \uXXXX
                format!("\\u{:04x}", c as u32).chars().collect()
            }
            c => vec![c],
        })
        .collect()
}

fn generate_index_json(branches: &[BranchInfo]) -> String {
    // Build JSON structure manually (keeping it simple)
    let mut json = String::from("{\n  \"branches\": {\n");

    let branch_count = branches.len();
    for (b_idx, branch) in branches.iter().enumerate() {
        json.push_str(&format!("    \"{}\": [\n", branch.name));

        for (c_idx, commit) in branch.commits.iter().enumerate() {
            json.push_str("      {\n");
            json.push_str(&format!("        \"commit\": \"{}\",\n", commit.commit));
            json.push_str(&format!(
                "        \"commit_short\": \"{}\",\n",
                commit.commit_short
            ));
            json.push_str(&format!(
                "        \"branch_original\": \"{}\",\n",
                commit.branch_original
            ));

            if let Some(ref pr) = commit.pr_number {
                json.push_str(&format!("        \"pr_number\": \"{}\",\n", pr));
            }

            json.push_str(&format!(
                "        \"timestamp\": \"{}\",\n",
                commit.timestamp
            ));
            json.push_str(&format!(
                "        \"timestamp_display\": \"{}\",\n",
                commit.timestamp_display
            ));
            json.push_str(&format!(
                "        \"commit_message\": \"{}\",\n",
                escape_json(&commit.commit_message)
            ));
            json.push_str(&format!(
                "        \"pr_title\": \"{}\"",
                escape_json(&commit.pr_title)
            ));

            if let Some(instr) = commit.total_instructions {
                json.push_str(",\n");
                json.push_str(&format!("        \"total_instructions\": {}", instr));
            }

            if let Some(ratio) = commit.facet_vs_serde_ratio {
                json.push_str(",\n");
                json.push_str(&format!("        \"facet_vs_serde_ratio\": {:.6}", ratio));
            }

            json.push_str("\n      }");

            if c_idx < branch.commits.len() - 1 {
                json.push(',');
            }
            json.push('\n');
        }

        json.push_str("    ]");
        if b_idx < branch_count - 1 {
            json.push(',');
        }
        json.push('\n');
    }

    json.push_str("  }\n");
    json.push_str("}\n");
    json
}
