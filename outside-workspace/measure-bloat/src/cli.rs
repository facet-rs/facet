use clap::Parser;

// Command-line interface for the measure-bloat utility
// Used for: Parsing command line arguments and routing to appropriate functionality
#[derive(Parser, Debug)]
#[clap(author, version, about = "A utility to measure and compare binary sizes and build times", long_about = None)]
pub struct Cli {
    #[clap(subcommand)]
    pub command: Commands,
}

// Available commands for the measure-bloat utility
#[derive(Parser, Debug)]
pub enum Commands {
    /// Compare build metrics between HEAD, main branch, and optionally a Serde baseline.
    Compare {
        /// Path to the root of the repository to be measured.
        /// Assumed to be a non-shallow git clone. The current checkout (HEAD) will be measured.
        #[clap(long, default_value = ".")]
        repo_path: String,

        /// Path where the Markdown comparison report will be saved.
        #[clap(long, default_value = "comparison_report.md")]
        output_path: String,

        /// The name of the main/base branch to compare against (e.g., "main", "master").
        #[clap(long, default_value = "main")]
        main_branch_name: String,
    },
}
