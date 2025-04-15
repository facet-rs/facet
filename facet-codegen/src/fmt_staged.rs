use std::fs;
use std::io::{self, Write};
use std::path::PathBuf;
use std::process::{Command, Stdio};
use std::sync::{Arc, Mutex};

use facet_ansi::Stylize as _;

// --- Data structures ---

#[derive(Debug, Clone)]
pub struct StagedFiles {
    pub clean: Vec<PathBuf>,
    pub dirty: Vec<PathBuf>,
    pub unstaged: Vec<PathBuf>,
}

impl StagedFiles {
    pub fn is_empty(&self) -> bool {
        self.clean.is_empty() && self.dirty.is_empty() && self.unstaged.is_empty()
    }
}

#[derive(Debug)]
pub struct FormatCandidate {
    pub path: PathBuf,
    pub original: Vec<u8>,
    pub formatted: Vec<u8>,
    pub diff: Option<String>,
}

#[derive(Debug)]
pub struct FormatResult {
    pub success: bool,
    pub applied: bool,
    pub path: PathBuf,
}

#[derive(Debug)]
pub enum UserAction {
    Fix,
    ShowDiff,
    Skip,
}

// --- Helpers ---

pub fn collect_staged_files() -> io::Result<StagedFiles> {
    // Run `git status --porcelain`
    let output = Command::new("git")
        .arg("status")
        .arg("--porcelain")
        .output()?;

    if !output.status.success() {
        panic!("Failed to run `git status --porcelain`");
    }
    let stdout = String::from_utf8_lossy(&output.stdout);

    let mut clean = Vec::new();
    let mut dirty = Vec::new();
    let mut unstaged = Vec::new();

    for line in stdout.lines() {
        // E.g. "M  src/main.rs", "A  foo.rs", "AM foo/bar.rs"
        if line.len() < 3 {
            continue;
        }
        let x = line.chars().next().unwrap();
        let y = line.chars().nth(1).unwrap();
        let path = line[3..].to_string();

        // Staged and not dirty (to be formatted/committed)
        if x != ' ' && x != '?' && y == ' ' {
            clean.push(PathBuf::from(path));
        }
        // Staged + dirty (index does not match worktree; skip and warn)
        else if x != ' ' && x != '?' && y != ' ' {
            dirty.push(PathBuf::from(path));
        }
        // Untracked or unstaged files (may be useful for warning)
        else if x == '?' {
            unstaged.push(PathBuf::from(path));
        }
    }
    Ok(StagedFiles {
        clean,
        dirty,
        unstaged,
    })
}

// --- Formatting process ---

pub fn run_rustfmt_on_files_parallel(files: &[PathBuf]) -> Vec<FormatCandidate> {
    use rayon::prelude::*;

    // For diff, we'll use 'similar' crate, as in main, if available.
    // Add similar = "2" to Cargo.toml if not already present.
    use similar::{Algorithm, TextDiff};

    let candidates: Arc<Mutex<Vec<FormatCandidate>>> = Arc::new(Mutex::new(Vec::new()));

    files.par_iter().for_each(|path| {
        let original = match fs::read(path) {
            Ok(val) => val,
            Err(e) => {
                eprintln!(
                    "{} {}: {}",
                    "❌".red(),
                    path.display().to_string().blue(),
                    format!("Failed to read: {e}").dim()
                );
                return;
            }
        };

        // Run rustfmt via stdin/stdout
        let mut cmd = Command::new("rustfmt")
            .arg("--edition")
            .arg("2024")
            .arg("--emit")
            .arg("stdout")
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .expect("Failed to spawn rustfmt");

        // Write original to rustfmt's stdin
        {
            let mut stdin = cmd.stdin.take().expect("Failed to take rustfmt stdin");
            if stdin.write_all(&original).is_err() {
                eprintln!(
                    "{} {}: {}",
                    "❌".red(),
                    path.display().to_string().blue(),
                    "Failed to write src to rustfmt".dim()
                );
                return;
            }
        }

        let output = cmd.wait_with_output().expect("Failed to wait on rustfmt");
        if !output.status.success() {
            eprintln!(
                "{} {}: rustfmt failed\n{}\n{}",
                "❌".red(),
                path.display().to_string().blue(),
                String::from_utf8_lossy(&output.stderr).dim(),
                String::from_utf8_lossy(&output.stdout).dim()
            );
            return;
        }
        let formatted = output.stdout;
        if formatted != original {
            // Compute diff using 'similar'
            let orig_str = String::from_utf8_lossy(&original);
            let fmt_str = String::from_utf8_lossy(&formatted);
            let diff = TextDiff::configure()
                .algorithm(Algorithm::Myers)
                .diff_lines(&orig_str, &fmt_str)
                .unified_diff()
                .header("before", "after")
                .to_string();

            let diff = if diff.trim().is_empty() {
                None
            } else {
                Some(diff)
            };

            let candidate = FormatCandidate {
                path: path.clone(),
                original,
                formatted,
                diff,
            };
            let mut guard = candidates.lock().unwrap();
            guard.push(candidate);
        }
    });

    Arc::try_unwrap(candidates).unwrap().into_inner().unwrap()
}

// --- User interaction ---

fn prompt_user_action(n: usize) -> UserAction {
    use console::{Emoji, style};

    // Emojis we will use
    static ACTION_REQUIRED: Emoji<'_, '_> = Emoji("🚧", "");
    static ARROW: Emoji<'_, '_> = Emoji("➤", "");

    let banner = format!(
        "{}\n{}\n{}\n",
        style(format!(
            "{}  ACTION REQUIRED {}",
            ACTION_REQUIRED, ACTION_REQUIRED
        ))
        .yellow()
        .bold()
        .italic()
        .on_black()
        .underlined(),
        style(format!(
            "Found {} file{} staged for commit that are not using rustfmt edition 2024.",
            n.to_string().magenta().bold(),
            if n == 1 { "" } else { "s" }
        )),
        style("Would you like to fix them now? [y]es/[d]iff/[n]o")
            .cyan()
            .bold()
    );
    println!("\n{}", banner);
    print!("{} {} ", ARROW, style("Your choice:").green().bold());
    io::stdout().flush().unwrap();

    // Spinner
    let spinner_frames = vec!["⠋", "⠙", "⠹", "⠸", "⠼", "⠴", "⠦", "⠧", "⠇", "⠏"];
    let spinner_idx = Arc::new(Mutex::new(0usize));
    let spinner_done = Arc::new(Mutex::new(false));
    let idx_clone = Arc::clone(&spinner_idx);
    let done_clone = Arc::clone(&spinner_done);

    // display spinner while waiting for input
    let handle = std::thread::spawn(move || {
        while !*done_clone.lock().unwrap() {
            {
                let mut idx = idx_clone.lock().unwrap();
                print!("\r{} ", spinner_frames[*idx].blue());
                io::stdout().flush().unwrap();
                *idx = (*idx + 1) % spinner_frames.len();
            }
            std::thread::sleep(std::time::Duration::from_millis(64));
        }
        print!("\r   \r"); // Clean up spinner
        io::stdout().flush().unwrap();
    });

    let mut input = String::new();
    io::stdin().read_line(&mut input).unwrap();
    *spinner_done.lock().unwrap() = true;
    handle.join().unwrap();

    let choice = input.trim().to_lowercase();
    match choice.as_str() {
        "y" | "yes" => UserAction::Fix,
        "d" | "diff" => UserAction::ShowDiff,
        _ => UserAction::Skip,
    }
}

pub fn show_diffs(candidates: &[FormatCandidate]) {
    println!("{}", "🦀  Showing rustfmt diffs:".yellow().bold().italic());
    for cand in candidates {
        println!(
            "\n    {}\n---",
            cand.path.display().to_string().blue().bold()
        );
        if let Some(ref diff) = cand.diff {
            println!("{}", diff.yellow());
        } else {
            println!("{}", "no diff available".dim());
        }
    }
}

pub fn apply_fmt_and_stage(candidates: &[FormatCandidate], check_mode: bool) -> Vec<FormatResult> {
    let mut results = Vec::new();
    for c in candidates {
        if check_mode {
            println!(
                "{} {}: {}",
                "⚠️".yellow(),
                c.path.display().to_string().blue(),
                "Would be reformatted".magenta()
            );
            results.push(FormatResult {
                success: false,
                applied: false,
                path: c.path.clone(),
            });
            continue;
        }
        if super::write_if_different(&c.path, c.formatted.clone(), false) {
            let status = Command::new("git")
                .arg("add")
                .arg(&c.path)
                .status()
                .expect("Failed to run git add");
            let applied = status.success();
            println!(
                "{} {}: {}",
                if applied { "✅".green() } else { "❌".red() },
                c.path.display().to_string().blue(),
                if applied {
                    "Formatted and staged".green().bold()
                } else {
                    "Failed to stage".red().bold()
                }
            );
            results.push(FormatResult {
                success: status.success(),
                applied,
                path: c.path.clone(),
            });
        }
    }
    results
}

// --- Orchestrator function ---

pub fn format_and_stage_files(files: &[PathBuf], check_mode: bool) {
    let staged_files = collect_staged_files().expect("Failed to get staged files");

    if !staged_files.dirty.is_empty() {
        println!(
            "{} {}",
            "⚠️ Ignored files with both staged and unstaged changes:".yellow(),
            "(not formatted)".dim()
        );
        for p in &staged_files.dirty {
            println!("   {}", p.display().to_string().dim());
        }
    }
    if !staged_files.unstaged.is_empty() {
        println!(
            "{} {}",
            "ℹ️ Unstaged/untracked files (ignored):".cyan(),
            "(not formatted)".dim()
        );
        for p in &staged_files.unstaged {
            println!("   {}", p.display().to_string().dim());
        }
    }

    let to_format: Vec<PathBuf> = files
        .iter()
        .filter(|p| staged_files.clean.contains(p))
        .cloned()
        .collect();

    if to_format.is_empty() {
        println!("{}", "No staged files require formatting.".green().bold());
        return;
    }

    println!(
        "{} {} file{} staged for commit using Rust 2024 formatting...",
        "🔎".magenta(),
        to_format.len().to_string().magenta().bold(),
        if to_format.len() == 1 { "" } else { "s" }
    );

    let candidates = run_rustfmt_on_files_parallel(&to_format);

    if candidates.is_empty() {
        println!(
            "{} {}",
            "✅".green(),
            "All staged files already formatted using rustfmt 2024."
                .green()
                .bold()
        );
        return;
    }

    // Banner and prompt for action
    let mut action = prompt_user_action(candidates.len());
    if let UserAction::ShowDiff = action {
        show_diffs(&candidates);
        // Second prompt, this time with just yes/no
        println!(
            "\n{}\n{}\n{}",
            "🚧  ACTION REQUIRED 🚧"
                .yellow()
                .bold()
                .italic()
                .on_black()
                .underline(),
            "Apply rustfmt 2024 formatting to these files?"
                .cyan()
                .bold(),
            "[y]es/[n]o".cyan().bold()
        );
        let mut input = String::new();
        io::stdin().read_line(&mut input).unwrap();
        if input.trim().eq_ignore_ascii_case("y") || input.trim().eq_ignore_ascii_case("yes") {
            action = UserAction::Fix;
        } else {
            action = UserAction::Skip;
        }
    }

    match action {
        UserAction::Fix => {
            apply_fmt_and_stage(&candidates, check_mode);
            println!(
                "\n{} {} file{} were reformatted and staged.",
                "✨".green().bold(),
                candidates.len().to_string().magenta().bold(),
                if candidates.len() == 1 { "" } else { "s" }
            );
        }
        UserAction::Skip => {
            println!(
                "{} {}",
                "ℹ️".cyan(),
                "No changes applied. Continuing with commit...".dim()
            );
        }
        UserAction::ShowDiff => {
            // ShowDiff never returned as final at this point
        }
    }
}
