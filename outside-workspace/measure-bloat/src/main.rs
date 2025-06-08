use anyhow::{Context, Result};
use clap::Parser;
use owo_colors::OwoColorize;
use std::path::PathBuf;
use std::process::Command;

// Declare modules according to the new structure
mod analysis;
mod build;
mod cli;
mod config;
mod report;
mod runner;
mod types;
mod workspace;

// External Crate Aliases if needed (e.g. for toml_edit)
// use toml_edit;

fn main() -> Result<()> {
    // Initialize color-backtrace for better error display
    color_backtrace::install();
    // Initialize logging (optional, but good practice)
    // env_logger::init(); // Or use another logger like `tracing`

    println!(
        "{} {}",
        "Measure Bloat Utility".bright_blue().bold(),
        "v0.1.0".bright_black()
    );

    let cli_args = cli::Cli::parse();

    match cli_args.command {
        cli::Commands::Compare {
            repo_path,
            output_path,
            main_branch_name,
        } => {
            // Find the repository root
            let repo_path = match repo_path {
                Some(path) => path,
                None => find_repository_root()?,
            };

            runner::run_global_comparison(&repo_path, &output_path, &main_branch_name).map_err(
                |e| {
                    eprintln!("{} Error during comparison: {:?}", "❌".red(), e);
                    e
                },
            )?;
        }
    }

    Ok(())
}

/// Find the git repository root from the current directory
fn find_repository_root() -> Result<String> {
    let output = Command::new("git")
        .args(&["rev-parse", "--show-toplevel"])
        .output()
        .context("Failed to execute git rev-parse --show-toplevel")?;

    if !output.status.success() {
        anyhow::bail!(
            "Failed to find git repository root. Make sure you're in a git repository.\nStderr: {}",
            String::from_utf8_lossy(&output.stderr)
        );
    }

    let root_path = String::from_utf8(output.stdout)
        .context("Invalid UTF-8 in git output")?
        .trim()
        .to_string();

    println!(
        "{} {} Found repository root: {}",
        "🔍".bright_green(),
        "Git:".bright_black(),
        root_path.bright_cyan()
    );

    Ok(root_path)
}
