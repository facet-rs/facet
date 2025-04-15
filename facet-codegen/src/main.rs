use crate::Options;
use crate::write_if_different;
use facet_ansi::Stylize as _;
use facet_ansi::Stylize as _;
use facet_ansi::Stylize as _;
use log::{error, info};
use log::{error, info, warn};
use similar::{ChangeTag, TextDiff};
use std::fs;
use std::io::Write;
use std::io::{self, Write};
use std::path::Path;
use std::path::Path;
use std::path::PathBuf;
use std::process;
use std::process::{Command, Stdio};
use std::sync::{Arc, Mutex};
use std::time::Instant;
use tuples::generate_tuples_impls;

mod tuples;

fn main() {
    facet_testhelpers::setup();

    let opts = Options {
        check: std::env::args().any(|arg| arg == "--check"),
    };
    let mut has_diffs = false;

    // Check if current directory has a Cargo.toml with [workspace]
    let cargo_toml_path = std::env::current_dir().unwrap().join("Cargo.toml");
    let cargo_toml_content =
        fs_err::read_to_string(cargo_toml_path).expect("Failed to read Cargo.toml");
    if !cargo_toml_content.contains("[workspace]") {
        error!("🚫 {}", "Cargo.toml does not contain [workspace] (you must run codegen from the workspace root)".red());
        panic!();
    }

    // Collect staged, not dirty files before running parallel tasks
    let staged_files = match fmt_staged::collect_staged_files() {
        Ok(files) => files,
        Err(e) => {
            error!("Failed to collect staged files: {e}");
            process::exit(1);
        }
    };

    // Run four tasks in parallel: three codegen tasks, and formatting
    let opts_clone1 = opts.clone();
    let tuple_impls_result = std::thread::spawn(move || {
        let mut local_has_diffs = false;
        generate_tuple_impls(&mut local_has_diffs, opts_clone1);
        local_has_diffs
    });

    let opts_clone2 = opts.clone();
    let readme_had_diffs = std::thread::spawn(move || readmes::generate_readme_files(opts_clone2));

    let opts_clone3 = opts.clone();
    let sample_code_result = std::thread::spawn(move || {
        let mut local_has_diffs = false;
        copy_cargo_expand_output(&mut local_has_diffs, &opts_clone3);
        local_has_diffs
    });

    // Collect results and update has_diffs
    has_diffs |= tuple_impls_result
        .join()
        .expect("tuple_impls thread panicked");
    has_diffs |= readme_had_diffs
        .join()
        .expect("readme_files thread panicked");
    has_diffs |= sample_code_result
        .join()
        .expect("sample_code thread panicked");

    // Wait for fmt thread
    fmt_result.join().expect("fmt_staged thread panicked");

    if opts.check && has_diffs {
        // Print a big banner with error message about generated files
        error!("┌────────────────────────────────────────────────────────────────────────────┐");
        error!("│                                                                            │");
        error!("│  GENERATED FILES HAVE CHANGED - RUN `just codegen` TO UPDATE THEM          │");
        error!("│                                                                            │");
        error!("│  For README.md files:                                                      │");
        error!("│                                                                            │");
        error!("│  • Don't edit README.md directly - edit the README.md.in template instead  │");
        error!("│  • Then run `just codegen` to regenerate the README.md files               │");
        error!("│  • A pre-commit hook is set up by cargo-husky to do just that              │");
        error!("│                                                                            │");
        error!("│  See CONTRIBUTING.md                                                       │");
        error!("│                                                                            │");
        error!("└────────────────────────────────────────────────────────────────────────────┘");
        process::exit(1);
    }
}

fn copy_cargo_expand_output(has_diffs: &mut bool, opts: &Options) {
    let workspace_dir = std::env::current_dir().unwrap();
    let sample_dir = workspace_dir.join("sample");

    // Run cargo expand command and measure execution time
    let start_time = std::time::Instant::now();

    // Command 1: cargo rustc for expansion
    let cargo_expand_output = std::process::Command::new("cargo")
        .env("RUSTC_BOOTSTRAP", "1") // Necessary for -Z flags
        .current_dir(&sample_dir) // Set working directory instead of changing it
        .arg("rustc")
        .arg("--target-dir")
        .arg("/tmp/facet-codegen-expand") // Use a temporary, less intrusive target dir
        .arg("--lib") // Expand the library crate in the current directory
        .arg("--") // Separator for rustc flags
        .arg("-Zunpretty=expanded") // The flag to expand macros
        .output() // Execute and capture output
        .expect("Failed to execute cargo rustc for expansion");

    // Check if cargo rustc succeeded
    if !cargo_expand_output.status.success() {
        error!(
            "🚫 {}:\n--- stderr ---\n{}\n--- stdout ---\n{}",
            "cargo rustc expansion failed".red(),
            String::from_utf8_lossy(&cargo_expand_output.stderr).trim(),
            String::from_utf8_lossy(&cargo_expand_output.stdout).trim()
        );
        std::process::exit(1);
    }

    // Prepare the code for rustfmt: prepend the necessary lines
    let expanded_code = String::from_utf8(cargo_expand_output.stdout)
        .expect("Failed to convert cargo expand output to UTF-8 string");

    // Replace any ::facet:: references with crate::
    let expanded_code = expanded_code.replace("::facet::", "crate::");
    let expanded_code = expanded_code.replace("use facet::", "use crate::");

    let expanded_code = expanded_code.replace(
        "::impls::_core::marker::PhantomData",
        "::core::marker::PhantomData",
    );

    // Command 2: rustfmt to format the expanded code
    let mut rustfmt_cmd = std::process::Command::new("rustfmt")
        .arg("--edition")
        .arg("2024")
        .arg("--emit")
        .arg("stdout")
        .stdin(std::process::Stdio::piped()) // Prepare to pipe stdin
        .stdout(std::process::Stdio::piped()) // Capture stdout
        .stderr(std::process::Stdio::piped()) // Capture stderr
        .spawn()
        .expect("Failed to spawn rustfmt");

    // Write the combined code to rustfmt's stdin in a separate scope
    // to ensure stdin is closed, signaling EOF to rustfmt.
    {
        let mut stdin = rustfmt_cmd
            .stdin
            .take()
            .expect("Failed to open rustfmt stdin");
        stdin
            .write_all(expanded_code.as_bytes())
            .expect("Failed to write to rustfmt stdin");
    } // stdin is closed here

    // Wait for rustfmt to finish and collect its output
    let output = rustfmt_cmd
        .wait_with_output()
        .expect("Failed to wait for rustfmt");

    // Check if rustfmt succeeded (using the final 'output' variable)
    // Note: The original code only checked the final status, which might hide
    // the cargo expand error if rustfmt succeeds. We now check both stages.
    if !output.status.success() {
        error!(
            "🚫 {}:\n--- stderr ---\n{}\n--- stdout ---\n{}",
            "rustfmt failed".red(),
            String::from_utf8_lossy(&output.stderr).trim(),
            String::from_utf8_lossy(&output.stdout).trim()
        );
        // We still need to check the final status for the rest of the function
        // but the process might have already exited if cargo expand failed.
        // If rustfmt itself fails, exit here.
        std::process::exit(1);
    }
    let execution_time = start_time.elapsed();

    if !output.status.success() {
        error!("🚫 {}", "Cargo expand command failed".red());
        std::process::exit(1);
    }

    let expanded_code =
        String::from_utf8(output.stdout).expect("Failed to convert output to string");

    // First collect doc comments, then filter out lines we don't want
    let doc_comments = expanded_code
        .lines()
        .filter(|line| line.trim_start().starts_with("//!"))
        .collect::<Vec<_>>()
        .join("\n");

    let expanded_code = expanded_code
        .lines()
        .filter(|line| {
            let trimmed = line.trim_start();
            !trimmed.starts_with("#![")
                && !trimmed.starts_with("#[facet(")
                && !trimmed.starts_with("#[macro_use]")
                && !trimmed.starts_with("//!")
        })
        .collect::<Vec<_>>()
        .join("\n");
    let expanded_code = format!("{}\n#![allow(warnings)]\n{}", doc_comments, expanded_code);

    // Ensure a trailing newline for consistency
    let expanded_code = if expanded_code.is_empty() {
        String::new()
    } else {
        format!("{}\n", expanded_code)
    };

    // Write the expanded code to the target file
    let target_path = workspace_dir
        .join("facet")
        .join("src")
        .join("sample_generated_code.rs");

    let was_different = write_if_different(&target_path, expanded_code.into_bytes(), opts.check);
    *has_diffs |= was_different;

    if opts.check {
        info!(
            "✅ Checked {} (took {:?})",
            "sample_generated_code.rs".blue().green(),
            execution_time
        );
    } else if was_different {
        info!(
            "🔧 Generated {} (took {:?})",
            "sample_generated_code.rs".blue().green(),
            execution_time
        );
    } else {
        info!(
            "✅ No changes to {} (took {:?})",
            "sample_generated_code.rs".blue().green(),
            execution_time
        );
    }
}

#[derive(Debug, Clone)]
struct Options {
    check: bool,
}

fn check_diff(path: &Path, new_content: &[u8]) -> bool {
    if !path.exists() {
        warn!(
            "📁 {}: {}",
            path.display(),
            "would create new file".yellow()
        );
        return true;
    }

    let old_content = fs_err::read(path).unwrap();
    if old_content != new_content {
        let old_str = String::from_utf8_lossy(&old_content);
        let new_str = String::from_utf8_lossy(new_content);

        let diff = TextDiff::from_lines(&old_str, &new_str);
        info!("📝 {}", format!("Diff for {}:", path.display()).blue());

        // Track consecutive equal lines
        let mut equal_count = 0;
        let mut last_tag = None;

        for change in diff.iter_all_changes() {
            let tag = change.tag();

            // If we're switching from Equal to another tag, and we have >=4 equal lines, show the count
            if last_tag == Some(ChangeTag::Equal) && tag != ChangeTag::Equal && equal_count > 3 {
                info!(" {} lines omitted.", equal_count - 1);
                equal_count = 0;
            }

            match tag {
                ChangeTag::Equal => {
                    if equal_count == 0 {
                        // Always show the first equal line
                        info!(" {}", change);
                    } else if equal_count < 3 {
                        // Show the 2nd and 3rd equal lines
                        info!(" {}", change);
                    }
                    equal_count += 1;
                }
                ChangeTag::Delete => {
                    equal_count = 0;
                    info!("-{}", change.red());
                }
                ChangeTag::Insert => {
                    equal_count = 0;
                    info!("+{}", change.green());
                }
            }

            last_tag = Some(tag);
        }

        // Handle case where diff ends with equal lines
        if last_tag == Some(ChangeTag::Equal) && equal_count > 3 {
            info!(" {} lines omitted.", equal_count - 1);
        }

        return true;
    }
    false
}

pub(crate) fn write_if_different(path: &Path, content: Vec<u8>, check_mode: bool) -> bool {
    let is_different = check_diff(path, &content);
    if check_mode {
        return is_different;
    }
    if is_different {
        info!("Overwriting {} (had changes)", path.display().blue());
        fs_err::write(path, content).expect("Failed to write file");
        return true;
    }
    false
}

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

pub(crate) fn generate_readme_files(opts: Options) -> bool {
    let mut has_diffs = false;

    let start = Instant::now();

    // Get all crate directories in the workspace
    let workspace_dir = std::env::current_dir().unwrap();
    let entries = fs_err::read_dir(&workspace_dir).expect("Failed to read workspace directory");

    // Keep track of all crates we generate READMEs for
    let mut generated_crates = Vec::new();

    let template_name = "README.md.in";

    // Process each crate in the workspace
    for entry in entries {
        let entry = entry.expect("Failed to read directory entry");
        let crate_path = entry.path();

        // Skip non-directories and entries starting with '.' or '_'
        if !crate_path.is_dir()
            || crate_path.file_name().is_some_and(|name| {
                let name = name.to_string_lossy();
                name.starts_with('.') || name.starts_with('_')
            })
        {
            continue;
        }

        // Skip target directory
        let dir_name = crate_path.file_name().unwrap().to_string_lossy();
        if dir_name == "target" {
            continue;
        }

        // Check if this is a crate directory (has a Cargo.toml)
        let cargo_toml_path = crate_path.join("Cargo.toml");
        if !cargo_toml_path.exists() {
            continue;
        }

        // Get crate name from directory name
        let crate_name = dir_name.to_string();

        // Check for templates
        let template_path = if crate_name == "facet" {
            Path::new(template_name).to_path_buf()
        } else {
            crate_path.join(template_name)
        };

        if template_path.exists() {
            // Get crate name from directory name
            let crate_name = crate_path
                .file_name()
                .unwrap()
                .to_string_lossy()
                .to_string();
            process_readme_template(
                &crate_name,
                &crate_path,
                &template_path,
                &mut has_diffs,
                opts.clone(),
            );
            generated_crates.push(crate_name);
        } else {
            error!("🚫 Missing template: {}", template_path.display().red());
            panic!();
        }
    }

    // Generate workspace README, too (which is the same as the `facet` crate)
    let workspace_template_path = workspace_dir.join(template_name);
    if !workspace_template_path.exists() {
        error!(
            "🚫 {}",
            format!(
                "Template file {} not found for workspace. We looked at {}",
                template_name,
                workspace_template_path.display()
            )
            .red()
        );
        panic!();
    }

    process_readme_template(
        "facet",
        &workspace_dir,
        &workspace_template_path,
        &mut has_diffs,
        opts.clone(),
    );

    // Add workspace to the list of generated READMEs
    generated_crates.push("workspace".to_string());

    // Print a comma-separated list of all crates we generated READMEs for
    let execution_time = start.elapsed();
    if opts.check {
        info!(
            "📚 Checked READMEs for: {} (took {:?})",
            generated_crates.join(", ").blue(),
            execution_time
        );
    } else if has_diffs {
        info!(
            "📚 Generated READMEs for: {} (took {:?})",
            generated_crates.join(", ").blue(),
            execution_time
        );
    } else {
        info!(
            "✅ No changes to READMEs for: {} (took {:?})",
            generated_crates.join(", ").blue(),
            execution_time
        );
    }
    has_diffs
}

fn generate_tuple_impls(has_diffs: &mut bool, opts: Options) {
    // Start timer to measure execution time
    let start_time = std::time::Instant::now();

    // Define the base path and template path
    let base_path = Path::new("facet-core/src/impls_core/tuple.rs");

    let output = generate_tuples_impls();

    // Format the generated code using rustfmt
    let mut fmt = std::process::Command::new("rustfmt")
        .arg("--edition")
        .arg("2024")
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .spawn()
        .expect("Failed to spawn rustfmt");

    // Write to rustfmt's stdin
    fmt.stdin
        .take()
        .expect("Failed to get stdin")
        .write_all(output.as_bytes())
        .expect("Failed to write to rustfmt stdin");

    // Get formatted output
    let formatted_output = fmt.wait_with_output().expect("Failed to wait for rustfmt");
    if !formatted_output.status.success() {
        // Save the problematic output for inspection
        let _ = std::fs::write("/tmp/output.rs", &output);
        error!(
            "🚫 {} {}",
            "rustfmt failed to format the code.".red(),
            "The unformatted output has been saved to /tmp/output.rs for inspection.".yellow(),
        );

        error!(
            "🚫 {}",
            format!("rustfmt failed with exit code: {}", formatted_output.status).red()
        );
        std::process::exit(1);
    }

    let was_different = write_if_different(base_path, formatted_output.stdout, opts.check);
    *has_diffs |= was_different;

    // Calculate execution time
    let execution_time = start_time.elapsed();

    // Print success message with execution time
    if opts.check {
        info!(
            "✅ Checked {} (took {:?})",
            "tuple implementations".blue().green(),
            execution_time
        );
    } else if was_different {
        info!(
            "🔧 Generated {} (took {:?})",
            "tuple implementations".blue().green(),
            execution_time
        );
    } else {
        info!(
            "✅ No changes to {} (took {:?})",
            "tuple implementations".blue().green(),
            execution_time
        );
    }
}
