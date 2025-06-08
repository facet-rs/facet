use indicatif::{MultiProgress, ProgressBar, ProgressStyle};
use log::{LevelFilter, debug, error, warn};
use owo_colors::OwoColorize;
use similar::ChangeTag;
#[cfg(not(windows))]
use std::os::unix::process::ExitStatusExt;
#[cfg(windows)]
use std::os::windows::process::ExitStatusExt;
use std::{
    collections::HashMap,
    fs,
    io::{self, Write},
    path::{Path, PathBuf},
    process::{Command, Output, Stdio},
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
        mpsc::{RecvTimeoutError, channel},
    },
    thread,
    time::{Duration, Instant},
};

mod readme;
mod sample;

#[derive(Debug, Clone)]
pub struct Job {
    pub path: PathBuf,
    pub old_content: Option<Vec<u8>>,
    pub new_content: Vec<u8>,
}

impl Job {
    pub fn is_noop(&self) -> bool {
        match &self.old_content {
            Some(old) => &self.new_content == old,
            None => self.new_content.is_empty(),
        }
    }

    /// Computes a summary of the diff between old_content and new_content.
    /// Returns (num_plus, num_minus): plus lines (insertions), minus lines (deletions).
    pub fn diff_plus_minus(&self) -> (usize, usize) {
        use similar::TextDiff;
        let old = match &self.old_content {
            Some(bytes) => String::from_utf8_lossy(bytes),
            None => "".into(),
        };
        let new = String::from_utf8_lossy(&self.new_content);
        let diff = TextDiff::from_lines(&old, &new);
        let mut plus = 0;
        let mut minus = 0;
        for change in diff.iter_all_changes() {
            match change.tag() {
                ChangeTag::Insert => plus += 1,
                ChangeTag::Delete => minus += 1,
                ChangeTag::Equal => {}
            }
        }
        (plus, minus)
    }

    pub fn show_diff(&self) {
        use similar::{ChangeTag, TextDiff};

        let context_lines = 3;

        let old = match &self.old_content {
            Some(bytes) => String::from_utf8_lossy(bytes),
            None => "".into(),
        };
        let new = String::from_utf8_lossy(&self.new_content);
        let diff = TextDiff::from_lines(&old, &new);

        // Collect the changes for random access
        let changes: Vec<_> = diff.iter_all_changes().collect();

        // Identify the indices of changes (added/removed lines)
        let mut change_indices = vec![];
        for (i, change) in changes.iter().enumerate() {
            match change.tag() {
                ChangeTag::Insert | ChangeTag::Delete => change_indices.push(i),
                _ => {}
            }
        }

        let mut show_line = vec![false; changes.len()];
        // Mark lines to show: up to context_lines before/after each change
        for &idx in &change_indices {
            let start = idx.saturating_sub(context_lines);
            let end = (idx + context_lines + 1).min(changes.len());
            #[allow(clippy::needless_range_loop)]
            for i in start..end {
                show_line[i] = true;
            }
        }

        // Always show a few lines at the top and bottom of the diff for context,
        // in case the first or last lines are not changes.
        #[allow(clippy::needless_range_loop)]
        for i in 0..context_lines.min(changes.len()) {
            show_line[i] = true;
        }
        #[allow(clippy::needless_range_loop)]
        for i in changes.len().saturating_sub(context_lines)..changes.len() {
            show_line[i] = true;
        }

        let mut last_was_ellipsis = false;
        for (i, change) in changes.iter().enumerate() {
            if show_line[i] {
                match change.tag() {
                    ChangeTag::Insert => print!("{}", format_args!("    +{}", change).green()),
                    ChangeTag::Delete => print!("{}", format_args!("    -{}", change).red()),
                    ChangeTag::Equal => print!("{}", format_args!("    {}", change).dimmed()),
                }
                last_was_ellipsis = false;
            } else if !last_was_ellipsis {
                println!("{}", "    ...".dimmed());
                last_was_ellipsis = true;
            }
        }
        println!();
    }

    /// Applies the job by writing out the new_content to path and staging the file.
    pub fn apply(&self) -> std::io::Result<()> {
        use std::fs;
        use std::process::Command;
        fs::write(&self.path, &self.new_content)?;
        // Now stage it, best effort
        let _ = Command::new("git").arg("add").arg(&self.path).status();
        Ok(())
    }
}

pub fn enqueue_readme_jobs(sender: std::sync::mpsc::Sender<Job>) {
    let workspace_dir = std::env::current_dir().unwrap();
    let entries = match fs_err::read_dir(&workspace_dir) {
        Ok(e) => e,
        Err(e) => {
            error!("Failed to read workspace directory ({})", e);
            return;
        }
    };

    let template_name = "README.md.in";

    for entry in entries {
        let entry = match entry {
            Ok(entry) => entry,
            Err(e) => {
                warn!("Skipping entry: {e}");
                continue;
            }
        };
        let crate_path = entry.path();

        if !crate_path.is_dir()
            || crate_path.file_name().is_some_and(|name| {
                let name = name.to_string_lossy();
                name.starts_with('.') || name.starts_with('_')
            })
        {
            continue;
        }

        let dir_name = crate_path.file_name().unwrap().to_string_lossy();
        if dir_name == "target" {
            continue;
        }

        let cargo_toml_path = crate_path.join("Cargo.toml");
        if !cargo_toml_path.exists() {
            continue;
        }

        let crate_name = dir_name.to_string();

        let template_path = if crate_name == "facet" {
            Path::new(template_name).to_path_buf()
        } else {
            crate_path.join(template_name)
        };

        if template_path.exists() {
            // Read the template file
            let template_input = match fs::read_to_string(&template_path) {
                Ok(s) => s,
                Err(e) => {
                    error!("Failed to read template {}: {e}", template_path.display());
                    continue;
                }
            };

            // Generate the README content using readme::generate
            let readme_content = readme::generate(readme::GenerateReadmeOpts {
                crate_name: crate_name.clone(),
                input: template_input,
            });

            // Determine the README.md output path
            let readme_path = crate_path.join("README.md");

            // Read old_content from README.md if exists, otherwise None
            let old_content = fs::read(&readme_path).ok();

            // Build the job
            let job = Job {
                path: readme_path,
                old_content,
                new_content: readme_content.into_bytes(),
            };

            // Send job
            if let Err(e) = sender.send(job) {
                error!("Failed to send job: {e}");
            }
        } else {
            error!("🚫 Missing template: {}", template_path.display().red());
        }
    }

    // Also handle the workspace README (the "facet" crate at root)
    let workspace_template_path = workspace_dir.join(template_name);
    if workspace_template_path.exists() {
        // Read the template file
        let template_input = match fs::read_to_string(&workspace_template_path) {
            Ok(s) => s,
            Err(e) => {
                error!(
                    "Failed to read template {}: {e}",
                    workspace_template_path.display()
                );
                return;
            }
        };

        // Generate the README content using readme::generate
        let readme_content = readme::generate(readme::GenerateReadmeOpts {
            crate_name: "facet".to_string(),
            input: template_input,
        });

        // Determine the README.md output path
        let readme_path = workspace_dir.join("README.md");

        // Read old_content from README.md if exists, otherwise None
        let old_content = fs::read(&readme_path).ok();

        // Build the job
        let job = Job {
            path: readme_path,
            old_content,
            new_content: readme_content.into_bytes(),
        };

        // Send job
        if let Err(e) = sender.send(job) {
            error!("Failed to send workspace job: {e}");
        }
    } else {
        error!(
            "🚫 {}",
            format_args!(
                "Template file {} not found for workspace. We looked at {}",
                template_name,
                workspace_template_path.display()
            )
            .red()
        );
    }
}

pub fn enqueue_sample_job(sender: std::sync::mpsc::Sender<Job>) {
    use log::trace;
    use std::time::Instant;

    // Path where sample generated code should be written
    let rel_path = std::path::PathBuf::from("facet/src/sample_generated_code.rs");
    let workspace_dir = std::env::current_dir().unwrap();
    let target_path = workspace_dir.join(&rel_path);

    trace!(
        "Expanding sample code at {:?}",
        target_path.display().blue()
    );
    let start = Instant::now();

    // Generate the sample expanded and formatted code
    let code = sample::cargo_expand_and_format();
    let content = code.into_bytes();
    let size_mb = (content.len() as f64) / (1024.0 * 1024.0);

    let duration = start.elapsed();
    let secs = duration.as_secs_f64();
    let mbps = if secs > 0.0 { size_mb / secs } else { 0.0 };

    debug!(
        "Generated and formatted sample code for {}: {:.2} MiB in {:.2} s ({:.2} MiB/s)",
        rel_path.display().blue(),
        size_mb,
        secs,
        mbps.bright_magenta()
    );

    // Attempt to read existing file
    let old_content = fs::read(&target_path).ok();

    let job = Job {
        path: target_path,
        old_content,
        new_content: content,
    };

    if let Err(e) = sender.send(job) {
        error!("Failed to send sample job: {e}");
    }
}

pub fn enqueue_rustfmt_jobs(sender: std::sync::mpsc::Sender<Job>, staged_files: &StagedFiles) {
    use log::trace;
    use std::time::Instant;

    for path in &staged_files.clean {
        // Only process .rs files
        if let Some(ext) = path.extension() {
            if ext != "rs" {
                continue;
            }
        } else {
            continue;
        }

        trace!("rustfmt: formatting {}", path.display());

        let original = match fs::read(path) {
            Ok(val) => val,
            Err(e) => {
                error!(
                    "{} {}: {}",
                    "❌".red(),
                    path.display().to_string().blue(),
                    format_args!("Failed to read: {e}").dimmed()
                );
                continue;
            }
        };

        let size_mb = (original.len() as f64) / (1024.0 * 1024.0);

        // Format the content via rustfmt (edition 2024)
        let start = Instant::now();
        let cmd = Command::new("rustfmt")
            .arg("--edition")
            .arg("2024")
            .arg("--emit")
            .arg("stdout")
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn();

        let mut cmd = match cmd {
            Ok(child) => child,
            Err(e) => {
                error!("Failed to spawn rustfmt for {}: {}", path.display(), e);
                continue;
            }
        };

        // Write source to rustfmt's stdin
        {
            let mut stdin = cmd.stdin.take().expect("Failed to take rustfmt stdin");
            if stdin.write_all(&original).is_err() {
                error!(
                    "{} {}: {}",
                    "❌".red(),
                    path.display().to_string().blue(),
                    "Failed to write src to rustfmt".dimmed()
                );
                continue;
            }
        }

        let output = match cmd.wait_with_output() {
            Ok(out) => out,
            Err(e) => {
                error!("Failed to get rustfmt output for {}: {}", path.display(), e);
                continue;
            }
        };

        let duration = start.elapsed();
        let secs = duration.as_secs_f64();
        let mbps = if secs > 0.0 { size_mb / secs } else { 0.0 };
        debug!(
            "rustfmt: {} formatted {:.2} MiB in {:.2} s ({:.2} MiB/s)",
            path.display(),
            size_mb,
            secs,
            mbps.magenta()
        );

        if !output.status.success() {
            error!(
                "{} {}: rustfmt failed\n{}\n{}",
                "❌".red(),
                path.display().to_string().blue(),
                String::from_utf8_lossy(&output.stderr).dimmed(),
                String::from_utf8_lossy(&output.stdout).dimmed()
            );
            continue;
        }

        let formatted = output.stdout;

        // Only enqueue a job if the formatted output is different
        if formatted != original {
            let job = Job {
                path: path.clone(),
                old_content: Some(original),
                new_content: formatted,
            };
            if let Err(e) = sender.send(job) {
                error!("Failed to send rustfmt job for {}: {}", path.display(), e);
            }
        }
    }
}

pub fn show_jobs_and_apply_if_consent_is_given(jobs: &mut [Job]) {
    use std::io::{self, Write};

    // Emojis for display
    const ACTION_REQUIRED: &str = "🚧";

    const OK: &str = "✅";
    const CANCEL: &str = "🛑";

    jobs.sort_by_key(|job| job.path.clone());

    if jobs.is_empty() {
        println!(
            "{}",
            "All generated files are up-to-date and all Rust files are formatted properly"
                .green()
                .bold()
        );
        return;
    }

    println!(
        "\n{}\n{}\n",
        format_args!("{} GENERATION CHANGES {}", ACTION_REQUIRED, ACTION_REQUIRED)
            .on_black()
            .bold()
            .yellow()
            .italic()
            .underline(),
        format_args!(
            "The following {} file{} would be updated/generated:",
            jobs.len(),
            if jobs.len() == 1 { "" } else { "s" }
        )
        .magenta()
    );
    for (idx, job) in jobs.iter().enumerate() {
        let (plus, minus) = job.diff_plus_minus();
        println!(
            "  {}. {} {}{}",
            (idx + 1).bold().cyan(),
            job.path.display().yellow(),
            if plus > 0 {
                format!("+{}", plus).green().to_string()
            } else {
                String::new()
            },
            if minus > 0 {
                format!(" -{}", minus).red().to_string()
            } else {
                String::new()
            }
        );
    }

    let jobs_vec = jobs.to_vec();

    for job in &jobs_vec {
        print!("{} Applying {} ... ", OK, job.path.display().yellow());
        io::stdout().flush().unwrap();
        match job.apply() {
            Ok(_) => {
                println!("{}", "ok".green());
            }
            Err(e) => {
                println!("{} {}", CANCEL, format_args!("failed: {e}").red());
            }
        }
    }
    println!("{} {}", OK, "All fixes applied and staged!".green().bold());
    std::process::exit(0);
}

enum Subcommand {
    Check,
    Generate,
    Prepush,
}

fn main() {
    facet_testhelpers::setup();
    // Accept allowed log levels: trace, debug, error, warn, info
    log::set_max_level(LevelFilter::Info);
    if let Ok(log_level) = std::env::var("RUST_LOG") {
        let allowed = ["trace", "debug", "error", "warn", "info"];
        let log_level_lc = log_level.to_lowercase();
        if allowed.contains(&log_level_lc.as_str()) {
            let level = match log_level_lc.as_str() {
                "trace" => LevelFilter::Trace,
                "debug" => LevelFilter::Debug,
                "info" => LevelFilter::Info,
                "warn" => LevelFilter::Warn,
                "error" => LevelFilter::Error,
                _ => LevelFilter::Info,
            };
            log::set_max_level(level);
        }
    }

    // Parse subcommand
    let args: Vec<String> = std::env::args().collect();
    let subcommand = if args.len() < 2 {
        eprintln!("Usage: {} <check|generate|prepush>", args[0]);
        std::process::exit(1);
    } else {
        match args[1].as_str() {
            "check" => Subcommand::Check,
            "generate" => Subcommand::Generate,
            "prepush" => Subcommand::Prepush,
            other => {
                eprintln!("Unknown subcommand: {}", other);
                eprintln!("Usage: {} <check|generate|prepush>", args[0]);
                std::process::exit(1);
            }
        }
    };

    // Handle prepush immediately
    if matches!(subcommand, Subcommand::Prepush) {
        pre_push();
        std::process::exit(0); // Exit after pre-push check
    }

    // Check if current directory has a Cargo.toml with [workspace]
    // (Required for check and generate)
    let cargo_toml_path = std::env::current_dir().unwrap().join("Cargo.toml");
    let cargo_toml_content = match fs_err::read_to_string(&cargo_toml_path) {
        Ok(content) => content,
        Err(e) => {
            error!(
                "🚫 Failed to read {}: {}",
                cargo_toml_path.display(),
                e.to_string().red()
            );
            std::process::exit(1);
        }
    };
    if !cargo_toml_content.contains("[workspace]") {
        error!(
            "🚫 {}",
            "Cargo.toml does not contain [workspace] (you must run codegen from the workspace root)"
                .red()
        );
        std::process::exit(1);
    }

    // Collect staged files (needed for rustfmt jobs in check/generate)
    let staged_files = match collect_staged_files() {
        Ok(sf) => sf,
        Err(e) => {
            if std::env::var("GITHUB_ACTIONS").is_ok() {
                // In GitHub Actions, continue without error.
                error!("Failed to collect staged files: {e} (continuing due to GITHUB_ACTIONS)");
                StagedFiles {
                    clean: Vec::new(),
                    dirty: Vec::new(),
                    unstaged: Vec::new(),
                }
            } else {
                error!(
                    "Failed to collect staged files: {e}\n\
                    This tool requires Git to be installed and a Git repository initialized."
                );
                std::process::exit(1);
            }
        }
    };

    // --- Generate jobs (for check and generate) ---

    // Use a channel to collect jobs from all tasks.
    use std::sync::mpsc;
    let (sender, receiver) = mpsc::channel();

    // Start threads for each codegen job enqueuer
    let send1 = sender.clone();
    let handle_readme = std::thread::spawn(move || {
        enqueue_readme_jobs(send1);
    });
    let send2 = sender.clone();
    let handle_sample = std::thread::spawn(move || {
        enqueue_sample_job(send2);
    });
    // Rustfmt job: enqueue formatting for staged .rs files
    let send3 = sender.clone();
    let handle_rustfmt = std::thread::spawn(move || {
        enqueue_rustfmt_jobs(send3, &staged_files);
    });

    // Drop original sender so the channel closes when all workers finish
    drop(sender);

    // Collect jobs
    let mut jobs: Vec<Job> = Vec::new();
    for job in receiver {
        jobs.push(job);
    }

    // Wait for all job enqueuers to finish
    handle_readme.join().unwrap();
    handle_sample.join().unwrap();
    handle_rustfmt.join().unwrap();

    // --- Process jobs based on subcommand ---

    if jobs.is_empty() {
        println!("{}", "No codegen changes detected.".green().bold());
        return; // Exit 0
    }

    match subcommand {
        Subcommand::Check => {
            let mut any_diffs = false;
            for job in &jobs {
                // Compare disk content (could be None if file doesn't exist) to new_content
                let disk_content_opt = std::fs::read(&job.path).ok();
                let disk_content = disk_content_opt.unwrap_or_default(); // Treat non-existent as empty

                if disk_content != job.new_content {
                    error!(
                        "Diff detected in {}",
                        job.path.display().to_string().yellow().bold()
                    );
                    // Optionally show the diff here? For now, just flag it.
                    any_diffs = true;
                }
            }
            if any_diffs {
                // Print a big banner with error message about generated files
                error!(
                    "┌────────────────────────────────────────────────────────────────────────────┐"
                );
                error!(
                    "│                                                                            │"
                );
                error!(
                    "│  GENERATED FILES HAVE CHANGED - RUN `just gen` TO UPDATE THEM              │"
                );
                error!(
                    "│                                                                            │"
                );
                error!(
                    "│  For README.md files:                                                      │"
                );
                error!(
                    "│                                                                            │"
                );
                error!(
                    "│  • Don't edit README.md directly - edit the README.md.in template instead  │"
                );
                error!(
                    "│  • Then run `just gen` to regenerate the README.md files                   │"
                );
                error!(
                    "│  • A pre-commit hook is set up by cargo-husky to do just that              │"
                );
                error!(
                    "│                                                                            │"
                );
                error!(
                    "│  See CONTRIBUTING.md                                                       │"
                );
                error!(
                    "│                                                                            │"
                );
                error!(
                    "└────────────────────────────────────────────────────────────────────────────┘"
                );
                std::process::exit(1);
            } else {
                println!("{}", "✅ All generated files up to date.".green().bold());
                std::process::exit(0);
            }
        }
        Subcommand::Generate => {
            // Remove no-op jobs (where the content is unchanged).
            jobs.retain(|job| !job.is_noop());
            if jobs.is_empty() {
                println!(
                    "{}",
                    "All generated files are already up-to-date.".green().bold()
                );
                return; // Exit 0
            }
            // Show menu and potentially apply changes
            show_jobs_and_apply_if_consent_is_given(&mut jobs);
            // The show_jobs... function handles exit status based on user choice.
        }
        Subcommand::Prepush => {
            // This case was handled earlier
            unreachable!("Prepush should have exited earlier");
        }
    }
}

#[derive(Debug)]
pub struct StagedFiles {
    /// Files that are staged (in the index) and not dirty (working tree matches index).
    pub clean: Vec<PathBuf>,
    /// Files that are staged and dirty (index does NOT match working tree).
    pub dirty: Vec<PathBuf>,
    /// Files that are untracked or unstaged (not added to the index).
    pub unstaged: Vec<PathBuf>,
}

// -- Formatting support types --

#[derive(Debug)]
pub struct FormatCandidate {
    pub path: PathBuf,
    pub original: Vec<u8>,
    pub formatted: Vec<u8>,
    pub diff: Option<String>,
}

pub fn collect_staged_files() -> io::Result<StagedFiles> {
    // If running in GitHub Actions, return empty staged files.
    if std::env::var("GITHUB_ACTIONS").is_ok() {
        return Ok(StagedFiles {
            clean: Vec::new(),
            dirty: Vec::new(),
            unstaged: Vec::new(),
        });
    }

    // Run `git status --porcelain`
    let output = Command::new("git")
        .arg("status")
        .arg("--porcelain")
        .output()?;

    if !output.status.success() {
        panic!("Failed to run `git status --porcelain`");
    }
    let stdout = String::from_utf8_lossy(&output.stdout);

    log::trace!("Parsing {} output:", "`git status --porcelain`".blue());
    log::trace!("---\n{}\n---", stdout);

    let mut clean = Vec::new();
    let mut dirty = Vec::new();
    let mut unstaged = Vec::new();

    for line in stdout.lines() {
        log::trace!("Parsing git status line: {:?}", line.dimmed());
        // E.g. "M  src/main.rs", "A  foo.rs", "AM foo/bar.rs"
        if line.len() < 3 {
            log::trace!("Skipping short line: {:?}", line.dimmed());
            continue;
        }
        let x = line.chars().next().unwrap();
        let y = line.chars().nth(1).unwrap();
        let path = line[3..].to_string();

        log::trace!(
            "x: {:?}, y: {:?}, path: {:?}",
            x.magenta(),
            y.cyan(),
            path.dimmed()
        );

        // Staged and not dirty (to be formatted/committed)
        if x != ' ' && x != '?' && y == ' ' {
            log::debug!(
                "{} {}",
                "-> clean (staged, not dirty):".green().bold(),
                path.as_str().blue()
            );
            clean.push(PathBuf::from(&path));
        }
        // Staged + dirty (index does not match worktree; skip and warn)
        else if x != ' ' && x != '?' && y != ' ' {
            log::debug!(
                "{} {}",
                "-> dirty (staged and dirty):".yellow().bold(),
                path.as_str().blue()
            );
            dirty.push(PathBuf::from(&path));
        }
        // Untracked or unstaged files (may be useful for warning)
        else if x == '?' {
            log::debug!(
                "{} {}",
                "-> unstaged/untracked:".cyan().bold(),
                path.as_str().blue()
            );
            unstaged.push(PathBuf::from(&path));
        } else {
            log::debug!("{} {}", "-> not categorized:".red(), path.as_str().blue());
        }
    }
    Ok(StagedFiles {
        clean,
        dirty,
        unstaged,
    })
}

#[derive(Debug, Clone)]
struct CommandInfo {
    /// Short name for display purposes
    name: String,
    /// The actual command and its arguments
    command: Vec<String>,
}

fn pre_push() {
    println!("{}", "🚀 Running pre-push checks...".bold());
    let overall_start_time = Instant::now();

    // --- Setup Ctrl+C Handling ---
    let interrupted = Arc::new(AtomicBool::new(false));
    let i = interrupted.clone();
    ctrlc::set_handler(move || {
        if i.load(Ordering::SeqCst) { // Already received, force exit
            eprintln!("{}", "\n🛑 Force exiting...".red().bold());
            std::process::exit(130); // Standard exit code for Ctrl+C
        }
        i.store(true, Ordering::SeqCst);
        eprintln!("{}", "\n⏳ Ctrl-C received, attempting graceful shutdown... Press Ctrl-C again to force exit.".yellow().bold());
    })
    .expect("Error setting Ctrl-C handler");

    // --- Define Commands ---
    let commands = vec![
        CommandInfo {
            name: "codegen check".to_string(),
            command: {
                let current_exe =
                    std::env::current_exe().expect("Failed to get current executable path");

                vec![current_exe.to_string_lossy().into_owned(), "check".into()]
            },
        },
        CommandInfo {
            name: "absolve".to_string(),
            command: vec!["./facet-dev/absolve.sh".into()],
        },
        CommandInfo {
            name: "clippy".to_string(),
            command: vec![
                "cargo".into(),
                "clippy".into(),
                "--workspace".into(),
                "--all-targets".into(),
                "--target-dir".into(),
                "target/clippy".into(),
                "--".into(),
                "-D".into(),
                "warnings".into(),
            ],
        },
        CommandInfo {
            name: "test".to_string(),
            command: vec![
                "cargo".into(),
                "nextest".into(),
                "run".into(),
                "--no-fail-fast".into(),
            ],
        },
    ];
    let total_commands = commands.len();

    // --- Setup Indicatif ---
    let multi_progress = MultiProgress::new();
    let mut progress_bars: HashMap<String, ProgressBar> = HashMap::new();

    // --- Setup Communication Channel ---
    #[derive(Debug)]
    enum CommandResult {
        Success {
            info: CommandInfo,
            duration: Duration,
        },
        Failure {
            info: CommandInfo,
            output: Output,
            duration: Duration,
        },
        IoError {
            info: CommandInfo,
            error: io::Error,
            duration: Duration,
        },
    }

    let (sender, receiver) = channel::<CommandResult>();
    let mut handles = Vec::new();

    // --- Spawn Command Threads ---
    for cmd_info in commands.into_iter() {
        if interrupted.load(Ordering::SeqCst) {
            break; // Don't start new tasks if already interrupted
        }

        let pb = multi_progress.add(ProgressBar::new_spinner());
        pb.set_style(
            ProgressStyle::default_spinner()
                .template("{spinner:.cyan.bold} {prefix:>15!.yellow.bold} {msg}")
                .unwrap()
                .tick_strings(&["⠋", "⠙", "⠹", "⠸", "⠼", "⠴", "⠦", "⠧", "⠇", "⠏", " "]),
        );
        pb.enable_steady_tick(Duration::from_millis(80));
        pb.set_prefix(cmd_info.name.clone());
        pb.set_message("Running...");
        progress_bars.insert(cmd_info.name.clone(), pb.clone());

        let sender_clone = sender.clone();
        let cmd_info_clone = cmd_info.clone(); // Clone info for the thread
        let interrupted_clone = interrupted.clone();

        let handle = thread::spawn(move || {
            // Don't start if interrupted before thread execution
            if interrupted_clone.load(Ordering::SeqCst) {
                return;
            }

            let command_start_time = Instant::now(); // Start timer for this command

            let mut command_process = Command::new(&cmd_info_clone.command[0]);
            command_process
                .args(&cmd_info_clone.command[1..])
                .env("CLICOLOR_FORCE", "1") // Force color output for tools that support it
                .env("CARGO_TERM_COLOR", "always") // Force color for cargo commands
                .stdout(Stdio::piped())
                .stderr(Stdio::piped());

            // Execute and wait for the command
            let output_result = command_process.output();
            let command_duration = command_start_time.elapsed(); // Stop timer

            // Check interruption *after* potentially long command finishes, before sending result
            if interrupted_clone.load(Ordering::SeqCst) {
                return; // Don't send result if interrupted
            }

            // Send the result back to the main thread
            let cmd_result = match output_result {
                Ok(output) => {
                    if output.status.success() {
                        CommandResult::Success {
                            info: cmd_info_clone.clone(),
                            duration: command_duration,
                        }
                    } else {
                        CommandResult::Failure {
                            info: cmd_info_clone.clone(),
                            output,
                            duration: command_duration,
                        }
                    }
                }
                Err(e) => CommandResult::IoError {
                    info: cmd_info_clone.clone(),
                    error: e,
                    duration: command_duration,
                },
            };

            if sender_clone.send(cmd_result).is_err() {
                // Receiver disconnected (main thread likely exited or panicked)
                // Can't log using `log` crate here easily without setup.
                // eprintln!("Warning: Failed to send result for command '{}', main thread may have exited.", cmd_info_clone.name);
            }
        });
        handles.push(handle);
    }

    drop(sender); // Drop original sender so receiver knows when all threads are done

    // --- Collect Results and Update Progress ---
    let mut results = Vec::new();
    let mut finished_count = 0;

    // Keep polling for results and check for interruption
    while finished_count < handles.len() {
        if interrupted.load(Ordering::SeqCst) {
            break; // Exit collection loop if interrupted
        }

        match receiver.recv_timeout(Duration::from_millis(100)) {
            Ok(result) => {
                finished_count += 1;
                let (name, duration, status) = match &result {
                    CommandResult::Success { info, duration, .. } => {
                        (info.name.clone(), duration, "Success")
                    }
                    CommandResult::Failure { info, duration, .. } => {
                        (info.name.clone(), duration, "Failure")
                    }
                    CommandResult::IoError { info, duration, .. } => {
                        (info.name.clone(), duration, "IoError")
                    }
                };
                let duration_ms = duration.as_millis();
                let time_str = format!("({} ms)", duration_ms);

                if let Some(pb) = progress_bars.get(&name) {
                    match status {
                        "Success" => {
                            pb.finish_with_message(format!("{} {}", "✔ OK".green(), time_str));
                        }
                        "Failure" => {
                            pb.finish_with_message(format!(
                                "{} Failed {}",
                                "✖".red(),
                                time_str.dimmed()
                            ));
                        }
                        "IoError" => {
                            pb.finish_with_message(format!(
                                "{} {} {}",
                                "⚠ Error".red(),
                                "IO Error".red(),
                                time_str.dimmed()
                            ));
                        }
                        _ => unreachable!(),
                    }
                }
                results.push(result);
            }
            Err(RecvTimeoutError::Timeout) => {
                // No result yet, continue loop and check interruption flag again
            }
            Err(RecvTimeoutError::Disconnected) => {
                // All senders dropped. This means all threads finished or panicked.
                break;
            }
        }
    }

    // --- Wait for Threads and Final Cleanup ---
    // Ensure all spawned threads have completed (or acknowledged interruption)
    for handle in handles {
        let _ = handle.join(); // Ignore join errors, maybe thread panicked
    }

    // Mark any remaining spinners as interrupted/cancelled if needed
    if interrupted.load(Ordering::SeqCst) {
        for pb in progress_bars.values() {
            if !pb.is_finished() {
                pb.finish_with_message(format!("{}", "↪ Cancelled".dimmed()));
            }
        }
    }

    // Explicitly finish the MultiProgress manager (optional, but good practice)
    multi_progress.clear().ok(); // Clear progress bars before final message

    println!(); // Add a newline after progress bars finish

    let overall_duration = overall_start_time.elapsed(); // Stop overall timer
    let overall_duration_ms = overall_duration.as_millis();
    let overall_time_str = format!("in {} ms", overall_duration_ms);

    // --- Process Results and Exit ---
    if interrupted.load(Ordering::SeqCst) {
        println!(
            "{}",
            "🛑 Pre-push checks cancelled by user.".yellow().bold()
        );
        std::process::exit(130); // Standard exit code for Ctrl+C
    }

    let mut failures = Vec::new();
    for result in results {
        match result {
            CommandResult::Failure { info, output, .. } => {
                failures.push((info, output));
            }
            CommandResult::IoError { info, error, .. } => {
                failures.push((
                    info,
                    Output {
                        status: std::process::ExitStatus::from_raw(1),
                        stdout: Vec::new(),
                        stderr: format!("Error executing command: {}", error).into_bytes(),
                    },
                ));
            }
            CommandResult::Success { .. } => {}
        }
    }

    if failures.is_empty() {
        println!(
            "{} {} {}",
            "✅ All pre-push checks passed!".green().bold(),
            "ACCEPTED".green().bold(),
            overall_time_str.dimmed()
        );
        std::process::exit(0);
    } else {
        println!(
            "{} {} {}",
            format!(
                "❌ {}/{} pre-push checks failed:",
                failures.len(),
                total_commands
            )
            .red()
            .bold(),
            "REJECTED".red().bold(),
            overall_time_str
        );
        for (info, output) in failures {
            println!(
                "\n{}",
                "--------------------------------------------------".dimmed()
            );
            println!(
                "Failed Check: {} {}",
                info.name.yellow().bold(),
                format!("(Command: {})", info.command.join(" ")).dimmed()
            );
            println!("Exit Status: {}", output.status.to_string().red());

            let stdout_str = String::from_utf8_lossy(&output.stdout);
            if !stdout_str.trim().is_empty() {
                println!("{}", "\n--- Standard Output ---".cyan());
                println!("{}", stdout_str.trim_end());
            }

            let stderr_str = String::from_utf8_lossy(&output.stderr);
            if !stderr_str.trim().is_empty() {
                println!("{}", "\n--- Standard Error ---".red());
                println!("{}", stderr_str.trim_end());
            }
            println!(
                "{}",
                "--------------------------------------------------".dimmed()
            );
        }
        println!("\n{}", " PUSH REJECTED ".on_red().white().bold());
        std::process::exit(1);
    }
}
