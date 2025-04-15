use facet_ansi::Stylize as _;
use log::{error, info, warn};
use similar::{ChangeTag, TextDiff};
use std::fs;
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::{Arc, Mutex};
use std::time::Instant;

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
            let old_content = match fs::read(&readme_path) {
                Ok(data) => Some(data),
                Err(_) => None,
            };

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
        let old_content = match fs::read(&readme_path) {
            Ok(data) => Some(data),
            Err(_) => None,
        };

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
    let old_content = match fs::read(&base_path) {
        Ok(data) => Some(data),
        Err(_) => None,
    };

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
    let old_content = match fs::read(&target_path) {
        Ok(data) => Some(data),
        Err(_) => None,
    };

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
        let mut cmd = Command::new("rustfmt")
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

pub fn show_jobs_and_apply_if_consent_is_given(jobs: &mut Vec<Job>) {
    use console::{Emoji, style};
    use std::io::{self, Write};
    use termion::{event::Key, raw::IntoRawMode};

    // Emojis for display
    static ACTION_REQUIRED: Emoji<'_, '_> = Emoji("🚧", "");
    static ARROW: Emoji<'_, '_> = Emoji("➤", "");
    static DIFF: Emoji<'_, '_> = Emoji("📝", "");
    static OK: Emoji<'_, '_> = Emoji("✅", "");
    static CANCEL: Emoji<'_, '_> = Emoji("❌", "");

    // Sort jobs alphabetically by path
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

    // Drop-down menu options
    struct MenuOption {
        label: &'static str,
        description: &'static str,
        hotkey: &'static str,
    }

    let menu = [
        MenuOption {
            label: "Apply",
            description: "Apply the above changes",
            hotkey: "y",
        },
        MenuOption {
            label: "Diff",
            description: "Show details of all diffs",
            hotkey: "d",
        },
        MenuOption {
            label: "Cancel",
            description: "Abort without changing files",
            hotkey: "n",
        },
        MenuOption {
            label: "Abort",
            description: "Exit with error",
            hotkey: "a",
        },
    ];

    let mut selected = 0;
    let stdin = io::stdin();
    let mut stdout = io::stdout().into_raw_mode().unwrap();

    fn render_menu<W: Write>(stdout: &mut W, menu: &[MenuOption], selected: usize) {
        write!(
            stdout,
            "\r\n{} {} Please select an action (use ↑/↓ and Enter, or press hotkey letter):\r\n",
            ARROW,
            style("What do you want to do?").green().bold()
        )
        .unwrap();

        for (i, item) in menu.iter().enumerate() {
            if i == selected {
                write!(
                    stdout,
                    "  {} {} {}  {}\r\n",
                    style(">").yellow().bold(),
                    style(format!("[{}]", item.hotkey)).cyan().bold(),
                    style(item.label).yellow().on_blue().bold(),
                    style(item.description).white()
                )
                .unwrap();
            } else {
                write!(
                    stdout,
                    "    {} {}  {}\r\n",
                    style(format!("[{}]", item.hotkey)).dim().cyan(),
                    style(item.label).dim(),
                    style(item.description).dim()
                )
                .unwrap();
            }
        }
        write!(stdout, "{}", style("\n>").bold()).unwrap();
        stdout.flush().unwrap();
    }

    let mut show_menu = || {
        // Clear lines for redraw
        write!(
            stdout,
            "{}{}",
            termion::clear::AfterCursor,
            termion::cursor::Hide,
        )
        .unwrap();
        render_menu(&mut stdout, &menu, selected);
    };

    // termion disables canonical input, so we must provide fallback for non-terminals
    let mut fallback_mode = false;
    if atty::is(atty::Stream::Stdin) && atty::is(atty::Stream::Stdout) {
        show_menu();
    } else {
        fallback_mode = true;
        println!("\nSelect: [y]es = Apply, [d]iff = show diff, [n]o = cancel, [a]bort = fail:");
    }

    loop {
        let action = if !fallback_mode {
            use termion::input::TermReadEventsAndRaw;
            let evt = stdin.lock().events_and_raw().next();
            match evt {
                Some(Ok((termion::event::Event::Key(key), _raw))) => match key {
                    Key::Char('\n') | Key::Char('\r') => Some(selected),
                    Key::Char('y') | Key::Char('Y') => Some(0),
                    Key::Char('d') | Key::Char('D') => Some(1),
                    Key::Char('n') | Key::Char('N') => Some(2),
                    Key::Char('a') | Key::Char('A') => Some(3),
                    Key::Up | Key::Char('k') => {
                        if selected > 0 {
                            selected -= 1;
                        } else {
                            selected = menu.len() - 1;
                        }
                        show_menu();
                        None
                    }
                    Key::Down | Key::Char('j') => {
                        if selected + 1 < menu.len() {
                            selected += 1;
                        } else {
                            selected = 0;
                        }
                        show_menu();
                        None
                    }
                    Key::Ctrl('c') => {
                        write!(stdout, "\r\n{}\r\n", style("Aborted via Ctrl-C").red()).unwrap();
                        std::process::exit(130);
                    }
                    _ => None,
                },
                Some(Ok(_)) => None,
                Some(Err(_)) => None,
                None => {
                    // end of input
                    write!(stdout, "\r\n").unwrap();
                    break;
                }
            }
        } else {
            // fallback: read line and match it
            print!("> ");
            io::stdout().flush().unwrap();
            let mut input = String::new();
            if io::stdin().read_line(&mut input).is_err() {
                eprintln!("{} Failed to read user input, aborting.", CANCEL);
                std::process::exit(1);
            }
            let input = input.trim().to_lowercase();
            match input.as_str() {
                "y" | "yes" => Some(0),
                "d" | "diff" | "show" => Some(1),
                "n" | "no" => Some(2),
                "a" | "abort" => Some(3),
                "" => Some(selected),
                _ => {
                    println!(
                        "{} Unrecognized input, please enter y/yes, n/no, d/diff, or a/abort.",
                        CANCEL
                    );
                    None
                }
            }
        };

        if let Some(idx) = action {
            match idx {
                0 => {
                    // Apply
                    if !fallback_mode {
                        write!(stdout, "\r\n{}\r\n", style("Applying changes...").bold()).unwrap();
                    }
                    for job in jobs.iter() {
                        if let Err(e) = std::fs::write(&job.path, &job.new_content) {
                            eprintln!("{} Failed to write {}: {}", CANCEL, job.path.display(), e);
                            std::process::exit(1);
                        } else {
                            println!("{} {} updated.", OK, style(job.path.display()).green());
                        }
                    }
                    println!("{} All changes applied.", OK);
                    if !fallback_mode {
                        write!(stdout, "{}", termion::cursor::Show).unwrap();
                    }
                    break;
                }
                1 => {
                    // Diff
                    if !fallback_mode {
                        write!(stdout, "\r\n{}\r\n", style("Showing diffs...").bold()).unwrap();
                    }
                    for job in jobs.iter() {
                        println!(
                            "\n{} Diff for {}:",
                            DIFF,
                            style(job.path.display()).bold().blue()
                        );
                        show_diff_for_job(job);
                    }
                    println!("\n-- End of diffs --");
                    if !fallback_mode {
                        show_menu();
                    }
                    // Stay in menu
                }
                2 => {
                    // Cancel
                    println!("{} Changes were not applied.", CANCEL);
                    if !fallback_mode {
                        write!(stdout, "{}", termion::cursor::Show).unwrap();
                    }
                    std::process::exit(0);
                }
                3 => {
                    // Abort (fail)
                    println!("{} Aborted.", CANCEL);
                    if !fallback_mode {
                        write!(stdout, "{}", termion::cursor::Show).unwrap();
                    }
                    std::process::exit(1);
                }
                _ => (),
            }
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
            if check_diff(&job.path, &job.new_content) {
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
