//! Demonstration of facet-args colored help and error messages

use facet::Facet;
use facet_args as args;

/// Git-like CLI with subcommands
#[derive(Facet, Debug)]
#[allow(dead_code)]
struct GitArgs {
    /// Show version information
    #[facet(args::named)]
    version: bool,

    /// Git command to run
    #[facet(args::subcommand)]
    command: GitCommand,
}

/// Available git commands
#[derive(Facet, Debug)]
#[repr(u8)]
#[allow(dead_code)]
enum GitCommand {
    /// Clone a repository
    Clone {
        /// URL of the repository to clone
        #[facet(args::positional)]
        url: String,
        /// Directory to clone into
        #[facet(default, args::positional)]
        directory: Option<String>,
    },
    /// Show commit history
    Log {
        /// Number of commits to show
        #[facet(args::named, args::short = 'n')]
        count: Option<usize>,
        /// Show one line per commit
        #[facet(args::named)]
        oneline: bool,
    },
    /// Manage remotes
    Remote {
        /// Remote subcommand
        #[facet(args::subcommand)]
        action: RemoteAction,
    },
}

/// Remote management commands
#[derive(Facet, Debug)]
#[repr(u8)]
#[allow(dead_code)]
enum RemoteAction {
    /// Add a new remote
    Add {
        /// Name of the remote
        #[facet(args::positional)]
        name: String,
        /// URL of the remote
        #[facet(args::positional)]
        url: String,
    },
    /// Remove a remote
    #[facet(rename = "rm")]
    Remove {
        /// Name of the remote to remove
        #[facet(args::positional)]
        name: String,
    },
    /// List all remotes
    #[facet(rename = "ls")]
    List {
        /// Show verbose output
        #[facet(args::named, args::short = 'v')]
        verbose: bool,
    },
}

fn main() {
    let scenarios = [
        ("Top-level help", vec!["--help"]),
        ("Missing required subcommand", vec![]),
        ("Unknown subcommand", vec!["notacommand"]),
        ("Subcommand help", vec!["clone", "--help"]),
        ("Nested subcommand help", vec!["remote", "add", "--help"]),
    ];

    for (description, args_vec) in &scenarios {
        println!("\n{}", "=".repeat(80));
        println!("SCENARIO: {}", description);
        println!("Args: {:?}", args_vec);
        println!("{}", "=".repeat(80));

        let result = args::from_slice::<GitArgs>(args_vec);
        match result {
            Ok(_) => println!("✓ Successfully parsed arguments"),
            Err(e) => {
                let is_help = e.is_help_request();
                println!(
                    "\n{} {}",
                    if is_help {
                        "ℹ️  Help requested"
                    } else {
                        "❌ Error"
                    },
                    if is_help {
                        "(exit code: 0)"
                    } else {
                        "(exit code: 1)"
                    }
                );
                println!("\n{e}\n");
            }
        }
    }

    println!("\n{}", "=".repeat(80));
    println!("Summary:");
    println!("  • Help requests exit with code 0 and show colored help");
    println!("  • Errors exit with code 1 and show diagnostic with suggestions");
    println!("  • Subcommand help works with: <subcommand> --help");
    println!("{}", "=".repeat(80));
}
