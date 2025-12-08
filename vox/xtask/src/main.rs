//! xtask: Development tasks for rapace
//!
//! Run with: `cargo xtask <command>`

use clap::{Parser, Subcommand};

#[derive(Parser)]
#[command(name = "xtask")]
#[command(about = "Development tasks for rapace")]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Run fuzz tests with bolero
    Fuzz {
        /// Target to fuzz (e.g., "descriptor-validation")
        target: Option<String>,
    },
    /// Run property tests
    Proptest,
    /// Run all tests (unit + conformance)
    Test,
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let cli = Cli::parse();

    match cli.command {
        Commands::Fuzz { target } => {
            println!("TODO: Run bolero fuzz tests");
            if let Some(t) = target {
                println!("  target: {t}");
            }
        }
        Commands::Proptest => {
            println!("TODO: Run proptest");
        }
        Commands::Test => {
            println!("TODO: Run all tests");
        }
    }

    Ok(())
}
