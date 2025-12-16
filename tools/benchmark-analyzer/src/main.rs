//! Benchmark analyzer: run benchmarks, parse output, generate HTML reports.

mod parser;
mod report;
mod server;

use chrono::Local;
use facet::Facet;
use facet_args as args;
use indicatif::{ProgressBar, ProgressStyle};
use miette::Report;
use std::fs;
use std::io::{BufRead, BufReader};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
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

/// Run benchmarks, parse output, and generate HTML reports.
#[derive(Facet, Debug)]
struct Args {
    /// Start HTTP server to view the report after generation
    #[facet(args::named)]
    serve: bool,

    /// Skip running benchmarks, reuse previous benchmark data
    #[facet(args::named)]
    no_run: bool,
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

    let timestamp = Local::now().format("%Y%m%d-%H%M%S").to_string();

    let divan_file = report_dir.join(format!("divan-{}.txt", timestamp));
    let gungraun_file = report_dir.join(format!("gungraun-{}.txt", timestamp));
    let report_file = report_dir.join(format!("report-{}.html", timestamp));

    if !args.no_run {
        println!("🏃 Running benchmarks...");
        println!();

        // Run divan benchmarks
        run_benchmark_with_progress(
            &workspace_root,
            "unified_benchmarks_divan",
            &divan_file,
            "📊 Running divan (wall-clock)",
        );

        // Run gungraun benchmarks
        run_benchmark_with_progress(
            &workspace_root,
            "unified_benchmarks_gungraun",
            &gungraun_file,
            "🔬 Running gungraun (instruction counts)",
        );
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

    let divan_results = parser::parse_divan(&divan_text);
    let gungraun_results = parser::parse_gungraun(&gungraun_text);

    println!(
        "   Parsed {} divan results, {} gungraun results",
        divan_results.len(),
        gungraun_results.len()
    );

    let data = parser::combine_results(divan_results, gungraun_results);

    // Get git info
    let git_info = report::GitInfo {
        commit: get_git_output(&["rev-parse", "--short", "HEAD"]),
        branch: get_git_output(&["branch", "--show-current"]),
        timestamp: Local::now().format("%Y-%m-%d %H:%M:%S").to_string(),
    };

    // Generate report
    let html = report::generate_report(&data, &git_info);
    fs::write(&report_file, &html).expect("Failed to write report");

    // Create symlink to latest
    let latest_link = report_dir.join("report.html");
    let _ = fs::remove_file(&latest_link);
    #[cfg(unix)]
    {
        use std::os::unix::fs::symlink;
        let _ = symlink(report_file.file_name().unwrap(), &latest_link);
    }
    #[cfg(windows)]
    {
        // On Windows, just copy the file
        let _ = fs::copy(&report_file, &latest_link);
    }

    println!();
    println!("✅ Report generated: {}", file_hyperlink(&report_file));
    println!("   Latest: {}", file_hyperlink(&latest_link));
    println!();

    if args.serve {
        // Start HTTP server
        let rt = tokio::runtime::Runtime::new().expect("Failed to create tokio runtime");
        rt.block_on(async {
            if let Err(e) = server::serve(&report_dir, 1999).await {
                eprintln!("Server error: {}", e);
            }
        });
    } else {
        println!("To view:");
        println!("  open {}", file_hyperlink(&latest_link));
        println!();
        println!("Or auto-serve:");
        println!("  cargo xtask bench-report --serve");
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

fn run_benchmark_with_progress(
    workspace_root: &Path,
    bench_name: &str,
    output_file: &Path,
    label: &str,
) {
    let spinner = ProgressBar::new_spinner();
    spinner.set_style(
        ProgressStyle::default_spinner()
            .tick_strings(&["⠋", "⠙", "⠹", "⠸", "⠼", "⠴", "⠦", "⠧", "⠇", "⠏"])
            .template("{spinner:.cyan} {msg}")
            .unwrap(),
    );
    spinner.set_message(format!("{label}..."));
    spinner.enable_steady_tick(Duration::from_millis(80));

    let mut child = Command::new("cargo")
        .args([
            "bench",
            "--bench",
            bench_name,
            "--features",
            "cranelift",
            "--features",
            "jit",
        ])
        .current_dir(workspace_root.join("facet-json"))
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("Failed to run benchmark");

    // Read stdout and stderr in separate threads to avoid blocking
    let stdout = child.stdout.take().expect("Failed to get stdout");
    let stderr = child.stderr.take().expect("Failed to get stderr");

    let spinner_clone = spinner.clone();
    let label_owned = label.to_string();

    // Spawn thread to read stderr (where cargo outputs progress)
    let stderr_handle = std::thread::spawn(move || {
        let reader = BufReader::new(stderr);
        let mut lines = Vec::new();
        let mut line_count = 0;
        for line in reader.lines().map_while(Result::ok) {
            line_count += 1;
            spinner_clone.set_message(format!("{label_owned}... ({line_count} lines)"));
            lines.push(line);
        }
        lines
    });

    // Read stdout in main thread context (via another thread)
    let stdout_handle = std::thread::spawn(move || {
        let reader = BufReader::new(stdout);
        let mut lines = Vec::new();
        for line in reader.lines().map_while(Result::ok) {
            lines.push(line);
        }
        lines
    });

    // Wait for the process
    let status = child.wait().expect("Failed to wait for benchmark");

    // Collect output
    let stderr_lines = stderr_handle.join().expect("stderr thread panicked");
    let stdout_lines = stdout_handle.join().expect("stdout thread panicked");

    // Combine output (stderr first, then stdout, like original)
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

    let total_lines = stderr_lines.len() + stdout_lines.len();

    spinner.finish_and_clear();

    if status.success() {
        println!("{label}... ✓ {total_lines} lines");
    } else {
        println!("{label}... ✗ failed ({total_lines} lines)");
        eprintln!("Benchmark failed with exit code: {:?}", status.code());
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
