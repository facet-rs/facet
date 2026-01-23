//! Perf index management: clone perf repo, copy reports, generate index, push.

use crate::run_types::RunJson;
use chrono::{DateTime, Utc};
use facet_json_schema::to_schema;
use facet_typescript::TypeScriptGenerator;
use owo_colors::OwoColorize;
use serde_json::Value;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

const PERF_REPO_SSH: &str = "git@github.com:facet-rs/perf.facet.rs.git";
const PERF_REPO_HTTPS: &str = "https://github.com/facet-rs/perf.facet.rs.git";

/// Result of perf index operations
pub struct PerfIndexResult {
    /// Path to the perf directory (for serving)
    pub perf_dir: PathBuf,
}

/// Clone the perf.facet.rs repository
pub fn clone_perf_repo(workspace_root: &Path) -> Result<PathBuf, String> {
    let perf_dir = workspace_root.join("bench-reports/perf");

    // If it already exists, do a fetch + reset instead of full clone
    if perf_dir.join(".git").exists() {
        println!("📦 Updating existing perf repo...");

        // Try fetching with existing remote
        let fetch = Command::new("git")
            .args(["fetch", "origin", "gh-pages"])
            .current_dir(&perf_dir)
            .status();

        let fetch_success = fetch.is_ok() && fetch.unwrap().success();

        // If fetch failed, try SSH→HTTPS fallback
        if !fetch_success && !try_switch_to_https(&perf_dir)? {
            return Err("Failed to fetch perf repo (tried SSH and HTTPS)".to_string());
        }

        let reset = Command::new("git")
            .args(["reset", "--hard", "origin/gh-pages"])
            .current_dir(&perf_dir)
            .status();

        if reset.is_err() || !reset.unwrap().success() {
            return Err("Failed to reset perf repo".to_string());
        }

        println!("   ✓ Updated perf repo");
        return Ok(perf_dir);
    }

    // Try SSH first (for users with SSH keys), fall back to HTTPS
    println!("📦 Cloning perf.facet.rs repository...");

    // Remove directory if it exists but isn't a git repo
    if perf_dir.exists() {
        fs::remove_dir_all(&perf_dir).map_err(|e| format!("Failed to remove old perf dir: {e}"))?;
    }

    let ssh_result = Command::new("git")
        .args([
            "clone",
            "--branch",
            "gh-pages",
            "--single-branch",
            "--depth",
            "1",
            PERF_REPO_SSH,
            perf_dir.to_str().unwrap(),
        ])
        .status();

    if ssh_result.is_ok() && ssh_result.unwrap().success() {
        // Fetch full history for proper index generation
        let _ = Command::new("git")
            .args(["fetch", "--unshallow"])
            .current_dir(&perf_dir)
            .status();
        println!("   ✓ Cloned via SSH");
        return Ok(perf_dir);
    }

    // SSH failed, try HTTPS (read-only, won't be able to push)
    println!("   SSH clone failed, trying HTTPS (read-only)...");

    let https_result = Command::new("git")
        .args([
            "clone",
            "--branch",
            "gh-pages",
            "--single-branch",
            PERF_REPO_HTTPS,
            perf_dir.to_str().unwrap(),
        ])
        .status();

    if https_result.is_ok() && https_result.unwrap().success() {
        println!("   ✓ Cloned via HTTPS (read-only, --push won't work)");
        return Ok(perf_dir);
    }

    Err("Failed to clone perf repo via SSH or HTTPS".to_string())
}

/// Copy benchmark reports to the perf directory structure
///
/// Layout (v2):
///   perf/
///     - index.html (loads app.js SPA)
///     - app.js (unified SPA with hash routing)
///     - index-v2.json (commit-centric index)
///   perf/runs/{branch_key}/{commit_sha}/
///     - run.json (benchmark data + metadata)
///
/// The SPA uses hash-based routing: /#/runs/:branch/:commit/:op
pub fn copy_reports(
    workspace_root: &Path,
    perf_dir: &Path,
    report_dir: &Path,
) -> Result<(), String> {
    println!("📋 Copying reports to perf structure...");

    // Get git metadata - prefer environment variables (set by CI) over git commands
    let commit = std::env::var("COMMIT").unwrap_or_else(|_| get_git_output(&["rev-parse", "HEAD"]));
    let commit_short = std::env::var("COMMIT_SHORT")
        .unwrap_or_else(|_| get_git_output(&["rev-parse", "--short", "HEAD"]));
    let branch_original = std::env::var("BRANCH_ORIGINAL")
        .unwrap_or_else(|_| get_git_output(&["branch", "--show-current"]));

    // Sanitize branch name for URL-safe directory (branch_key)
    let branch_key: String = branch_original
        .chars()
        .map(|c| {
            if c.is_alphanumeric() || c == '-' || c == '.' {
                c
            } else {
                '_'
            }
        })
        .collect();

    println!("   Branch: {} ({})", branch_original, commit_short);

    // Create destination directory: perf/runs/{branch_key}/{commit}/
    let runs_dir = perf_dir.join("runs");
    let dest = runs_dir.join(&branch_key).join(&commit);
    fs::create_dir_all(&dest).map_err(|e| format!("Failed to create dest dir: {e}"))?;

    // Copy run.json - contains all benchmark data + metadata
    // The SPA loads this via fetch and renders client-side
    let run_json_src = report_dir.join("run.json");
    if run_json_src.exists() {
        let dst = dest.join("run.json");
        fs::copy(&run_json_src, &dst).map_err(|e| format!("Failed to copy run.json: {e}"))?;
        println!("   ✓ Copied run.json");
    } else {
        return Err("run.json not found - run benchmark-analyzer first".to_string());
    }

    // Also copy perf-data-*.json for backward compatibility during transition
    let entries =
        fs::read_dir(report_dir).map_err(|e| format!("Failed to read report dir: {e}"))?;
    for entry in entries.flatten() {
        let name = entry.file_name();
        let name_str = name.to_string_lossy();
        if name_str.starts_with("perf-data-") && name_str.ends_with(".json") {
            let dst = dest.join(&*name_str);
            fs::copy(entry.path(), &dst).ok(); // Best effort for backward compat
        }
    }

    // Generate legacy metadata.json for backward compatibility during transition
    // This can be removed once frontend is fully migrated to run.json
    let now: DateTime<Utc> = Utc::now();
    let timestamp = now.to_rfc3339();
    let timestamp_display = now.format("%Y-%m-%d %H:%M:%S UTC").to_string();
    let commit_message = std::env::var("COMMIT_MESSAGE")
        .unwrap_or_else(|_| get_git_output(&["log", "-1", "--format=%B", &commit]));

    let pr_number = std::env::var("PR_NUMBER").unwrap_or_default();
    let pr_title = if !pr_number.is_empty() {
        get_git_output(&[
            "gh", "pr", "view", &pr_number, "--json", "title", "--jq", ".title",
        ])
    } else {
        String::new()
    };

    let metadata = format!(
        r#"{{
  "commit": "{}",
  "commit_short": "{}",
  "branch": "{}",
  "branch_original": "{}",
  "pr_number": "{}",
  "timestamp": "{}",
  "timestamp_display": "{}",
  "commit_message": "{}",
  "pr_title": "{}"
}}
"#,
        commit,
        commit_short,
        branch_key,
        branch_original,
        pr_number,
        timestamp,
        timestamp_display,
        escape_json(&commit_message),
        escape_json(&pr_title)
    );

    fs::write(dest.join("metadata.json"), metadata).ok(); // Best effort for backward compat

    // Copy fonts to shared location
    let fonts_dir = perf_dir.join("fonts");
    fs::create_dir_all(&fonts_dir).ok();
    for font in ["IosevkaFtl-Regular.ttf", "IosevkaFtl-Bold.ttf"] {
        let src = report_dir.join(font);
        if src.exists() {
            let dst = fonts_dir.join(font);
            fs::copy(&src, &dst).ok();
        }
    }

    // Generate TypeScript types and JSON Schema on-the-fly
    // These are derived from run_types.rs and always kept in sync
    generate_types_and_schema(perf_dir)?;

    // Build and copy the SPA
    // The app.ts handles all routing via hash URLs (/#/runs/:branch/:commit/:op)
    build_spa(workspace_root, perf_dir)?;
    println!("   ✓ Built SPA assets");

    // Type-check the TypeScript SPA
    typecheck_spa(workspace_root)?;

    // Validate run.json against the schema
    validate_run_json(&dest, workspace_root)?;

    // Copy favicons
    let docs_static = workspace_root.join("docs/static");
    for favicon in ["favicon.png", "favicon.ico"] {
        let src = docs_static.join(favicon);
        if src.exists() {
            fs::copy(&src, perf_dir.join(favicon)).ok();
        }
    }
    println!("   ✓ Copied assets");

    // Update "latest" symlink in the branch directory
    let branch_dir = runs_dir.join(&branch_key);
    let latest_link = branch_dir.join("latest");
    let _ = fs::remove_file(&latest_link);
    #[cfg(unix)]
    {
        use std::os::unix::fs::symlink;
        let _ = symlink(&commit, &latest_link);
    }
    #[cfg(windows)]
    {
        let _ = std::os::windows::fs::symlink_dir(&commit, &latest_link);
    }
    println!("   ✓ Updated latest symlink");

    Ok(())
}

/// Run perf-index-generator to create index.html and index.json
pub fn generate_index(workspace_root: &Path, perf_dir: &Path) -> Result<(), String> {
    println!("📊 Generating index pages...");

    let tools_manifest = workspace_root.join("tools/Cargo.toml");
    let status = Command::new("cargo")
        .args([
            "run",
            "--manifest-path",
            tools_manifest.to_str().unwrap(),
            "--release",
            "-p",
            "perf-index-generator",
            "--",
            perf_dir.to_str().unwrap(),
        ])
        .current_dir(workspace_root)
        .status()
        .map_err(|e| format!("Failed to run perf-index-generator: {e}"))?;

    if !status.success() {
        return Err("perf-index-generator failed".to_string());
    }

    println!("   ✓ Generated index.html and index.json");
    Ok(())
}

/// Push results to perf.facet.rs repository
pub fn push_results(perf_dir: &Path) -> Result<(), String> {
    println!("🚀 Pushing to perf.facet.rs...");

    let commit_short = std::env::var("COMMIT_SHORT")
        .unwrap_or_else(|_| get_git_output(&["rev-parse", "--short", "HEAD"]));
    let branch = std::env::var("BRANCH_ORIGINAL")
        .unwrap_or_else(|_| get_git_output(&["branch", "--show-current"]));

    // Configure git user for the commit
    let _ = Command::new("git")
        .args(["config", "user.name", "benchmark-analyzer"])
        .current_dir(perf_dir)
        .status();

    let _ = Command::new("git")
        .args(["config", "user.email", "benchmark-analyzer@facet.rs"])
        .current_dir(perf_dir)
        .status();

    // Stage all changes
    let add = Command::new("git")
        .args(["add", "."])
        .current_dir(perf_dir)
        .status()
        .map_err(|e| format!("git add failed: {e}"))?;

    if !add.success() {
        return Err("git add failed".to_string());
    }

    // Commit
    let commit_msg = format!("Add benchmarks for {}@{}", branch, commit_short);
    let commit = Command::new("git")
        .args(["commit", "-m", &commit_msg])
        .current_dir(perf_dir)
        .status()
        .map_err(|e| format!("git commit failed: {e}"))?;

    if !commit.success() {
        // Might be nothing to commit
        println!("   ⚠ Nothing to commit (no changes?)");
        return Ok(());
    }

    // Push with retry logic
    for attempt in 1..=3 {
        let push = Command::new("git")
            .args(["push", "origin", "gh-pages"])
            .current_dir(perf_dir)
            .status();

        if push.is_ok() && push.unwrap().success() {
            println!("   ✓ Pushed to perf.facet.rs");
            return Ok(());
        }

        if attempt < 3 {
            println!("   ⚠ Push failed, retrying ({}/3)...", attempt + 1);
            // Pull and retry
            let _ = Command::new("git")
                .args(["pull", "--rebase", "origin", "gh-pages"])
                .current_dir(perf_dir)
                .status();
        }
    }

    Err("Failed to push after 3 attempts".to_string())
}

/// Run the full perf index workflow
pub fn run_perf_index(
    workspace_root: &Path,
    report_dir: &Path,
    filter: Option<&str>,
    push: bool,
) -> Result<PerfIndexResult, String> {
    // Safety check: refuse to push filtered results
    if push && filter.is_some() {
        eprintln!();
        eprintln!(
            "{}",
            "❌ Cannot --push with a filter! Partial benchmark results should not be published."
                .red()
                .bold()
        );
        eprintln!("   Filter was: {}", filter.unwrap().yellow());
        eprintln!();
        eprintln!("Remove the filter or remove --push to continue.");
        std::process::exit(1);
    }

    // Clone or update perf repo
    let perf_dir = clone_perf_repo(workspace_root)?;

    // Copy reports to perf structure
    copy_reports(workspace_root, &perf_dir, report_dir)?;

    // Generate index pages
    generate_index(workspace_root, &perf_dir)?;

    // Push if requested
    if push {
        push_results(&perf_dir)?;
    }

    Ok(PerfIndexResult { perf_dir })
}

/// Try switching the remote from SSH to HTTPS and retry fetch
/// Returns Ok(true) if successfully switched and fetched, Ok(false) if remote wasn't SSH
fn try_switch_to_https(perf_dir: &Path) -> Result<bool, String> {
    println!("   Fetch failed, checking remote URL...");

    // Get current remote URL
    let remote_output = Command::new("git")
        .args(["remote", "get-url", "origin"])
        .current_dir(perf_dir)
        .output()
        .map_err(|e| format!("Failed to get remote URL: {e}"))?;

    if !remote_output.status.success() {
        return Ok(false);
    }

    let remote_url = String::from_utf8_lossy(&remote_output.stdout)
        .trim()
        .to_string();

    // Only switch if currently using SSH
    if !remote_url.contains("git@github.com") && !remote_url.starts_with("ssh://") {
        return Ok(false);
    }

    println!("   Switching from SSH to HTTPS...");

    let set_url = Command::new("git")
        .args(["remote", "set-url", "origin", PERF_REPO_HTTPS])
        .current_dir(perf_dir)
        .status()
        .map_err(|e| format!("Failed to set remote URL: {e}"))?;

    if !set_url.success() {
        return Err("Failed to switch remote to HTTPS".to_string());
    }

    // Retry fetch with HTTPS
    let retry_fetch = Command::new("git")
        .args(["fetch", "origin", "gh-pages"])
        .current_dir(perf_dir)
        .status()
        .map_err(|e| format!("Failed to retry fetch: {e}"))?;

    if !retry_fetch.success() {
        return Ok(false);
    }

    println!("   ✓ Switched to HTTPS and fetched successfully");
    Ok(true)
}

fn get_git_output(args: &[&str]) -> String {
    Command::new("git")
        .args(args)
        .output()
        .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string())
        .unwrap_or_else(|_| "unknown".to_string())
}

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

/// Generate TypeScript types and JSON Schema on-the-fly
/// These files are derived from run_types.rs and kept in sync with the Rust types
fn generate_types_and_schema(perf_dir: &Path) -> Result<(), String> {
    // Generate TypeScript types
    let mut generator = TypeScriptGenerator::new();
    generator.add_type::<RunJson>();
    let ts = generator.finish();

    let ts_output = format!(
        r#"// Generated from tools/benchmark-analyzer/src/run_types.rs
// Do not edit manually - regenerated on each benchmark run
//
// These types match the run-v1.json schema produced by benchmark-analyzer

{ts}"#
    );

    fs::write(perf_dir.join("run-types.d.ts"), &ts_output)
        .map_err(|e| format!("Failed to write TypeScript types: {e}"))?;

    // Generate JSON Schema
    let schema_str = to_schema::<RunJson>();
    let mut schema: Value =
        serde_json::from_str(&schema_str).map_err(|e| format!("Invalid JSON from schema: {e}"))?;
    remove_nulls(&mut schema);

    // Add a comment at the root level
    if let Value::Object(ref mut map) = schema {
        let mut new_map = serde_json::Map::new();
        new_map.insert(
            "$comment".to_string(),
            Value::String(
                "Generated from tools/benchmark-analyzer/src/run_types.rs - do not edit manually"
                    .to_string(),
            ),
        );
        for (k, v) in map.iter() {
            new_map.insert(k.clone(), v.clone());
        }
        *map = new_map;
    }

    let schema_output =
        serde_json::to_string_pretty(&schema).map_err(|e| format!("Failed to serialize: {e}"))?;
    fs::write(perf_dir.join("run-schema.json"), &schema_output)
        .map_err(|e| format!("Failed to write JSON Schema: {e}"))?;

    println!("   ✓ Generated TypeScript types and JSON Schema");
    Ok(())
}

/// Recursively remove null values from a JSON Value
fn remove_nulls(value: &mut Value) {
    match value {
        Value::Object(map) => {
            map.retain(|_, v| !v.is_null());
            for v in map.values_mut() {
                remove_nulls(v);
            }
        }
        Value::Array(arr) => {
            arr.retain(|v| !v.is_null());
            for v in arr.iter_mut() {
                remove_nulls(v);
            }
        }
        _ => {}
    }
}

/// Ensure pnpm is available, fail hard if not
fn require_pnpm() -> Result<(), String> {
    let status = Command::new("pnpm")
        .arg("--version")
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status();

    match status {
        Ok(s) if s.success() => Ok(()),
        _ => {
            Err("pnpm is required but not found. Install it with: npm install -g pnpm".to_string())
        }
    }
}

/// Ensure pnpm dependencies are installed
fn ensure_pnpm_deps(scripts_dir: &Path) -> Result<(), String> {
    if !scripts_dir.join("node_modules").exists() {
        println!("   Installing TypeScript dependencies...");
        let output = Command::new("pnpm")
            .arg("install")
            .current_dir(scripts_dir)
            .output()
            .map_err(|e| format!("Failed to run pnpm install: {}", e))?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Err(format!("pnpm install failed:\n{}", stderr));
        }
    }
    Ok(())
}

/// Type-check the TypeScript SPA using @typescript/native-preview (tsgo)
fn typecheck_spa(workspace_root: &Path) -> Result<(), String> {
    let scripts_dir = workspace_root.join("scripts");

    require_pnpm()?;
    ensure_pnpm_deps(&scripts_dir)?;

    let output = Command::new("pnpm")
        .args(["check"])
        .current_dir(&scripts_dir)
        .output()
        .map_err(|e| format!("Failed to run pnpm check: {}", e))?;

    if output.status.success() {
        println!("   ✓ TypeScript type check passed");
        Ok(())
    } else {
        let stderr = String::from_utf8_lossy(&output.stderr);
        let stdout = String::from_utf8_lossy(&output.stdout);
        Err(format!(
            "TypeScript type check failed:\n{}\n{}",
            stdout, stderr
        ))
    }
}

/// Build the SPA (transpile TypeScript to JavaScript) and copy assets
fn build_spa(workspace_root: &Path, perf_dir: &Path) -> Result<(), String> {
    let scripts_dir = workspace_root.join("scripts");

    require_pnpm()?;
    ensure_pnpm_deps(&scripts_dir)?;

    // Build TypeScript -> JavaScript using esbuild
    let output = Command::new("pnpm")
        .args(["build"])
        .current_dir(&scripts_dir)
        .output()
        .map_err(|e| format!("Failed to run pnpm build: {}", e))?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        let stdout = String::from_utf8_lossy(&output.stdout);
        return Err(format!("SPA build failed:\n{}\n{}", stdout, stderr));
    }

    // Copy built app.js to perf dir
    let app_js = scripts_dir.join("app.js");
    if app_js.exists() {
        fs::copy(&app_js, perf_dir.join("app.js"))
            .map_err(|e| format!("Failed to copy app.js: {}", e))?;
    } else {
        return Err("app.js not found after build".to_string());
    }

    // Copy shared styles
    let styles = scripts_dir.join("shared-styles.css");
    if styles.exists() {
        fs::copy(&styles, perf_dir.join("shared-styles.css"))
            .map_err(|e| format!("Failed to copy shared-styles.css: {}", e))?;
    }

    Ok(())
}

/// Validate run.json against the TypeScript types
fn validate_run_json(run_dir: &Path, workspace_root: &Path) -> Result<(), String> {
    let run_json_path = run_dir.join("run.json");
    if !run_json_path.exists() {
        return Err("run.json not found".to_string());
    }

    let scripts_dir = workspace_root.join("scripts");

    require_pnpm()?;
    ensure_pnpm_deps(&scripts_dir)?;

    let output = Command::new("pnpm")
        .args(["validate", run_json_path.to_str().unwrap()])
        .current_dir(&scripts_dir)
        .output()
        .map_err(|e| format!("Failed to run validation: {}", e))?;

    if output.status.success() {
        println!("   ✓ run.json validation passed");
        Ok(())
    } else {
        let stderr = String::from_utf8_lossy(&output.stderr);
        let stdout = String::from_utf8_lossy(&output.stdout);
        Err(format!(
            "run.json validation failed:\n{}\n{}",
            stdout, stderr
        ))
    }
}
