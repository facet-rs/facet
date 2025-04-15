use facet_ansi::Stylize as _;
use log::{error, warn};
use similar::ChangeTag;
use std::fs;
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

mod readme;
mod sample;
mod tuples;

#[derive(Debug)]
pub struct Job {
    pub path: PathBuf,
    pub old_content: Option<Vec<u8>>,
    pub new_content: Vec<u8>,
}

impl Job {
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
        let old = match &self.old_content {
            Some(bytes) => String::from_utf8_lossy(bytes),
            None => "".into(),
        };
        let new = String::from_utf8_lossy(&self.new_content);
        let diff = TextDiff::from_lines(&old, &new);
        for change in diff.iter_all_changes() {
            match change.tag() {
                ChangeTag::Insert => print!("{}", format!("+{}", change).green()),
                ChangeTag::Delete => print!("{}", format!("-{}", change).red()),
                ChangeTag::Equal => print!(" {}", change),
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
    use std::path::Path;

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
            format!(
                "Template file {} not found for workspace. We looked at {}",
                template_name,
                workspace_template_path.display()
            )
            .red()
        );
    }
}

pub fn enqueue_tuple_job(sender: std::sync::mpsc::Sender<Job>) {
    // Path where tuple impls should be written
    let base_path = Path::new("facet-core/src/impls_core/tuple.rs");

    // Generate the tuple impls code
    let output = tuples::generate();
    let content = output.into_bytes();

    // Attempt to read existing file
    let old_content = fs::read(base_path).ok();

    let job = Job {
        path: base_path.to_path_buf(),
        old_content,
        new_content: content,
    };

    if let Err(e) = sender.send(job) {
        error!("Failed to send tuple job: {e}");
    }
}

pub fn enqueue_sample_job(sender: std::sync::mpsc::Sender<Job>) {
    // Path where sample generated code should be written
    let workspace_dir = std::env::current_dir().unwrap();
    let target_path = workspace_dir
        .join("facet")
        .join("src")
        .join("sample_generated_code.rs");

    // Generate the sample expanded and formatted code
    let code = sample::cargo_expand_and_format();
    let content = code.into_bytes();

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
    for path in &staged_files.clean {
        // Only process .rs files
        if let Some(ext) = path.extension() {
            if ext != "rs" {
                continue;
            }
        } else {
            continue;
        }

        let original = match fs::read(path) {
            Ok(val) => val,
            Err(e) => {
                error!(
                    "{} {}: {}",
                    "❌".red(),
                    path.display().to_string().blue(),
                    format!("Failed to read: {e}").dim()
                );
                continue;
            }
        };

        // Format the content via rustfmt (edition 2024)
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
                    "Failed to write src to rustfmt".dim()
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

        if !output.status.success() {
            error!(
                "{} {}: rustfmt failed\n{}\n{}",
                "❌".red(),
                path.display().to_string().blue(),
                String::from_utf8_lossy(&output.stderr).dim(),
                String::from_utf8_lossy(&output.stdout).dim()
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
    use console::{Emoji, style};
    use dialoguer::{Select, theme::ColorfulTheme};
    // Emojis for display
    static ACTION_REQUIRED: Emoji<'_, '_> = Emoji("🚧", "");
    static DIFF: Emoji<'_, '_> = Emoji("📝", "");
    static OK: Emoji<'_, '_> = Emoji("✅", "");
    static CANCEL: Emoji<'_, '_> = Emoji("❌", "");

    jobs.sort_by_key(|job| job.path.clone());

    if jobs.is_empty() {
        println!("{}", style("No changes to apply.").green().bold());
        return;
    }

    println!(
        "\n{}\n{}\n",
        style(format!(
            "{} GENERATION CHANGES {}",
            ACTION_REQUIRED, ACTION_REQUIRED
        ))
        .on_black()
        .bold()
        .yellow()
        .italic()
        .underlined(),
        style(format!(
            "The following {} file{} would be updated/generated:",
            jobs.len(),
            if jobs.len() == 1 { "" } else { "s" }
        ))
        .magenta()
    );
    for (idx, job) in jobs.iter().enumerate() {
        println!(
            "  {}. {}",
            style(idx + 1).bold().cyan(),
            style(job.path.display()).yellow()
        );
    }

    // Menu options and hotkeys
    const APPLY: usize = 0;
    const DIFFV: usize = 1;
    const CANCELV: usize = 2;
    const ABORT: usize = 3;
    let choices = [
        "✅ Apply: Apply the above changes",
        "📝 Diff: Show details of all diffs",
        "❌ Cancel: Abort without changing files",
        "❌ Abort: Exit with error",
    ];

    let theme = ColorfulTheme::default();

    let mut selected = 0;
    loop {
        // Use dialoguer's Select menu for interaction
        let choice = Select::with_theme(&theme)
            .with_prompt("What do you want to do?")
            .items(&choices)
            .default(selected)
            .interact()
            .unwrap_or(CANCELV);

        selected = choice;

        match selected {
            APPLY => {
                println!();
                for job in jobs.iter() {
                    if let Err(e) = std::fs::write(&job.path, &job.new_content) {
                        eprintln!("{} Failed to write {}: {}", CANCEL, job.path.display(), e);
                        std::process::exit(1);
                    } else {
                        println!("{} {} updated.", OK, style(job.path.display()).green());
                    }
                }
                println!("{} All changes applied.", OK);
                break;
            }
            DIFFV => {
                println!("\n{}\n", style("Showing diffs...").bold());
                for job in jobs.iter() {
                    println!(
                        "\n{} Diff for {}:",
                        DIFF,
                        style(job.path.display()).bold().blue()
                    );
                    show_diff_for_job(job);
                }
                println!("\n-- End of diffs --");
                // Stay in menu
            }
            CANCELV => {
                println!("{} Changes were not applied.", CANCEL);
                std::process::exit(0);
            }
            ABORT => {
                println!("{} Aborted.", CANCEL);
                std::process::exit(1);
            }
            _ => {}
        }
    }
}

fn show_diff_for_job(job: &Job) {
    use similar::{ChangeTag, TextDiff};
    let old = match &job.old_content {
        Some(bytes) => String::from_utf8_lossy(bytes),
        None => "".into(),
    };
    let new = String::from_utf8_lossy(&job.new_content);
    let diff = TextDiff::from_lines(&old, &new);
    for change in diff.iter_all_changes() {
        match change.tag() {
            ChangeTag::Insert => print!("{}", format!("+{}", change).green()),
            ChangeTag::Delete => print!("{}", format!("-{}", change).red()),
            ChangeTag::Equal => print!(" {}", change),
        }
    }
    println!();
}

#[derive(Debug, Clone)]
struct Options {
    check: bool,
}

fn main() {
    facet_testhelpers::setup();

    let opts = Options {
        check: std::env::args().any(|arg| arg == "--check"),
    };

    // Check if current directory has a Cargo.toml with [workspace]
    let cargo_toml_path = std::env::current_dir().unwrap().join("Cargo.toml");
    let cargo_toml_content =
        fs_err::read_to_string(cargo_toml_path).expect("Failed to read Cargo.toml");
    if !cargo_toml_content.contains("[workspace]") {
        error!(
            "🚫 {}",
            "Cargo.toml does not contain [workspace] (you must run codegen from the workspace root)"
                .red()
        );
        std::process::exit(1);
    }

    // Use a channel to collect jobs from all tasks.
    use std::sync::mpsc;
    let (sender, receiver) = mpsc::channel();

    // Start threads for each codegen job enqueuer
    let send1 = sender.clone();
    let handle_readme = std::thread::spawn(move || {
        enqueue_readme_jobs(send1);
    });
    let send2 = sender.clone();
    let handle_tuple = std::thread::spawn(move || {
        enqueue_tuple_job(send2);
    });
    let send3 = sender.clone();
    let handle_sample = std::thread::spawn(move || {
        enqueue_sample_job(send3);
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
    handle_tuple.join().unwrap();
    handle_sample.join().unwrap();

    if jobs.is_empty() {
        println!("{}", "No codegen changes detected.".green().bold());
        return;
    }

    if opts.check {
        let mut any_diffs = false;
        for job in &jobs {
            // Compare old_content (current file content) to new_content (generated content)
            let disk_content = std::fs::read(&job.path).unwrap_or_default();
            if disk_content != job.new_content {
                error!(
                    "Diff detected in {}",
                    job.path.display().to_string().yellow().bold()
                );
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
                "│  GENERATED FILES HAVE CHANGED - RUN `just codegen` TO UPDATE THEM          │"
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
                "│  • Then run `just codegen` to regenerate the README.md files               │"
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
        }
    } else {
        show_jobs_and_apply_if_consent_is_given(&mut jobs);
    }
}

#[derive(Debug)]
pub struct StagedFiles {
    pub clean: Vec<PathBuf>,
    pub dirty: Vec<PathBuf>,
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

#[derive(Debug)]
pub struct FormatResult {
    pub success: bool,
    pub applied: bool,
    pub path: PathBuf,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UserAction {
    Fix,
    ShowDiff,
    Skip,
}

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
