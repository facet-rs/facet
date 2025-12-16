//! Benchmark analyzer: run benchmarks, parse output, generate HTML reports.

mod parser;
mod report;
mod server;

use chrono::Local;
use console::Term;
use facet::Facet;
use facet_args as args;
use miette::Report;
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

    // Load benchmark categories from KDL
    let categories = report::load_categories(&workspace_root);
    println!("   Loaded {} benchmark categories", categories.len());

    // Get git info
    let git_info = report::GitInfo {
        commit: get_git_output(&["rev-parse", "--short", "HEAD"]),
        branch: get_git_output(&["branch", "--show-current"]),
        timestamp: Local::now().format("%Y-%m-%d %H:%M:%S").to_string(),
    };

    // Generate both reports (deserialize and serialize)
    let deser_file = report_dir.join("report-deser.html");
    let ser_file = report_dir.join("report-ser.html");

    let deser_html = report::generate_report(
        &data,
        &git_info,
        &categories,
        report::ReportMode::Deserialize,
    );
    fs::write(&deser_file, &deser_html).expect("Failed to write deserialize report");

    let ser_html =
        report::generate_report(&data, &git_info, &categories, report::ReportMode::Serialize);
    fs::write(&ser_file, &ser_html).expect("Failed to write serialize report");

    // Create symlink to latest deserialize report (default view)
    let latest_link = report_dir.join("report.html");
    let _ = fs::remove_file(&latest_link);
    #[cfg(unix)]
    {
        use std::os::unix::fs::symlink;
        let _ = symlink("report-deser.html", &latest_link);
    }
    #[cfg(windows)]
    {
        // On Windows, just copy the file
        let _ = fs::copy(&deser_file, &latest_link);
    }

    println!();
    println!("✅ Reports generated:");
    println!("   Deserialize: {}", file_hyperlink(&deser_file));
    println!("   Serialize:   {}", file_hyperlink(&ser_file));
    println!("   Latest:      {}", file_hyperlink(&latest_link));
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
) {
    let term = Term::stderr();

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
    } else {
        println!("{label}... ✗ failed");
        eprintln!("Benchmark failed with exit code: {:?}", status.code());
        eprintln!();
        eprintln!("--- Full output ---");
        eprint!("{combined}");
        eprintln!("--- End output ---");
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
