#![allow(dead_code)]

use facet::Facet;
use facet_args as args;

mod common;

/// A sample CLI application for testing help generation.
///
/// This is a longer description that spans multiple lines
/// to test how doc comments are handled.
#[derive(Facet, Debug)]
struct SimpleArgs {
    /// Enable verbose output
    #[facet(args::named, args::short = 'v')]
    verbose: bool,

    /// Number of parallel jobs
    #[facet(args::named, args::short = 'j')]
    jobs: Option<usize>,

    /// Input file to process
    #[facet(args::positional)]
    input: String,

    /// Output file (optional)
    #[facet(default, args::positional)]
    output: Option<String>,
}

#[test]
fn test_help_simple_struct() {
    let config = facet_args::HelpConfig {
        program_name: Some("myapp".to_string()),
        version: Some("1.0.0".to_string()),
        ..Default::default()
    };
    let help = facet_args::generate_help::<SimpleArgs>(&config);
    insta::assert_snapshot!(help);
}

/// Git-like CLI with subcommands
#[derive(Facet, Debug)]
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

#[test]
fn test_help_with_subcommands() {
    let config = facet_args::HelpConfig {
        program_name: Some("git".to_string()),
        version: Some("2.40.0".to_string()),
        ..Default::default()
    };
    let help = facet_args::generate_help::<GitArgs>(&config);
    insta::assert_snapshot!(help);
}

#[test]
fn test_help_enum_only() {
    let config = facet_args::HelpConfig {
        program_name: Some("git".to_string()),
        ..Default::default()
    };
    let help = facet_args::generate_help::<GitCommand>(&config);
    insta::assert_snapshot!(help);
}

// =============================================================================
// Automatic --help detection tests
// =============================================================================

#[test]
fn test_auto_help_long_flag() {
    let result = facet_args::from_slice::<SimpleArgs>(&["--help"]);
    assert!(result.is_err());
    let err = result.unwrap_err();
    assert!(err.is_help_request());
    assert!(err.help_text().is_some());
    let help = err.help_text().unwrap();
    assert!(help.contains("USAGE:"));
    assert!(help.contains("--verbose"));
}

#[test]
fn test_auto_help_short_flag() {
    let result = facet_args::from_slice::<SimpleArgs>(&["-h"]);
    assert!(result.is_err());
    let err = result.unwrap_err();
    assert!(err.is_help_request());
}

#[test]
fn test_auto_help_single_dash() {
    let result = facet_args::from_slice::<SimpleArgs>(&["-help"]);
    assert!(result.is_err());
    let err = result.unwrap_err();
    assert!(err.is_help_request());
}

#[test]
fn test_auto_help_windows_style() {
    let result = facet_args::from_slice::<SimpleArgs>(&["/?"]);
    assert!(result.is_err());
    let err = result.unwrap_err();
    assert!(err.is_help_request());
}

#[test]
fn test_auto_help_with_custom_config() {
    let config = facet_args::HelpConfig {
        program_name: Some("myapp".to_string()),
        version: Some("2.0.0".to_string()),
        ..Default::default()
    };
    let result = facet_args::from_slice_with_config::<SimpleArgs>(&["--help"], &config);
    assert!(result.is_err());
    let err = result.unwrap_err();
    assert!(err.is_help_request());
    let help = err.help_text().unwrap();
    assert!(help.contains("myapp 2.0.0"));
}

#[test]
fn test_auto_help_display() {
    let result = facet_args::from_slice::<SimpleArgs>(&["--help"]);
    let err = result.unwrap_err();
    // When displayed, help requests should show the help text
    let display = format!("{}", err);
    assert!(display.contains("USAGE:"));
}

#[test]
fn test_help_not_triggered_with_other_args() {
    // --help in the middle of other args should NOT trigger help
    // (it would be treated as an unknown flag in this case)
    let result = facet_args::from_slice::<SimpleArgs>(&["input.txt", "--help"]);
    // This should fail, but not with a help request
    assert!(result.is_err());
    let err = result.unwrap_err();
    assert!(!err.is_help_request());
}
