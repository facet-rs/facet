use anyhow::Result;
use clap::Parser;

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
    // Initialize logging (optional, but good practice)
    // env_logger::init(); // Or use another logger like `tracing`

    println!("Measure Bloat Utility");

    let cli_args = cli::Cli::parse();

    match cli_args.command {
        cli::Commands::Compare {
            repo_path,
            output_path,
            main_branch_name,
        } => {
            runner::run_global_comparison(&repo_path, &output_path, &main_branch_name).map_err(
                |e| {
                    eprintln!("Error during comparison: {:?}", e);
                    e
                },
            )?;
        }
    }

    Ok(())
}
