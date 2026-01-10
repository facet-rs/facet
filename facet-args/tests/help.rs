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
    // Help text is now colored, so check for "USAGE" without the colon
    assert!(help.contains("USAGE"));
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
    // Help text is now colored, so check for "USAGE" without the colon
    assert!(display.contains("USAGE"));
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

#[test]
fn test_subcommand_help_long_flag() {
    // Test --help after a subcommand
    let result = facet_args::from_slice::<GitArgs>(&["clone", "--help"]);
    assert!(result.is_err());
    let err = result.unwrap_err();
    assert!(err.is_help_request());
    let help = err.help_text().unwrap();
    // Should show help for the clone subcommand
    assert!(help.contains("clone"));
}

#[test]
fn test_subcommand_help_short_flag() {
    // Test -h after a subcommand
    let result = facet_args::from_slice::<GitArgs>(&["log", "-h"]);
    assert!(result.is_err());
    let err = result.unwrap_err();
    assert!(err.is_help_request());
    let help = err.help_text().unwrap();
    assert!(help.contains("log"));
}

#[test]
fn test_nested_subcommand_help() {
    // Test --help for nested subcommands
    let result = facet_args::from_slice::<GitArgs>(&["remote", "add", "--help"]);
    assert!(result.is_err());
    let err = result.unwrap_err();
    assert!(err.is_help_request());
    let help = err.help_text().unwrap();
    assert!(help.contains("add"));
}

// =============================================================================
// Error message tests - colored output for different error scenarios
// =============================================================================

#[test]
fn test_missing_required_subcommand_error() {
    // When a required subcommand is missing, should show error with available subcommands
    let result = facet_args::from_slice::<GitArgs>(&[]);
    assert!(result.is_err());
    let err = result.unwrap_err();
    assert!(!err.is_help_request()); // This is an ERROR, not help

    let display = format!("{}", err);

    // Should show error message
    assert!(display.contains("expected a subcommand"));
    // Should suggest available subcommands in the help text
    assert!(display.contains("clone") || display.contains("log") || display.contains("remote"));
}

#[test]
fn test_unknown_subcommand_error() {
    // When an unknown subcommand is provided, should show error with suggestions
    let result = facet_args::from_slice::<GitArgs>(&["notacommand"]);
    assert!(result.is_err());
    let err = result.unwrap_err();
    assert!(!err.is_help_request()); // This is an ERROR, not help

    let display = format!("{}", err);

    // Should show the unknown subcommand
    assert!(display.contains("notacommand") || display.contains("unknown"));
    // Should list available subcommands
    assert!(display.contains("clone") || display.contains("log") || display.contains("remote"));
}

#[test]
fn test_subcommand_help_colored_snapshot() {
    // Test that subcommand help is properly colored
    let result = facet_args::from_slice::<GitArgs>(&["clone", "--help"]);
    assert!(result.is_err());
    let err = result.unwrap_err();
    assert!(err.is_help_request());

    let help = err.help_text().unwrap();

    // The help text should contain ANSI color codes
    // Yellow/bold for section headers
    assert!(help.contains("USAGE") || help.contains("ARGUMENTS"));

    insta::assert_snapshot!(help);
}

#[test]
fn test_nested_subcommand_help_colored_snapshot() {
    // Test that nested subcommand help is properly colored
    let result = facet_args::from_slice::<GitArgs>(&["remote", "add", "--help"]);
    assert!(result.is_err());
    let err = result.unwrap_err();
    assert!(err.is_help_request());

    let help = err.help_text().unwrap();

    // Should contain the nested command path
    assert!(help.contains("remote") && help.contains("add"));

    insta::assert_snapshot!(help);
}

// =============================================================================
// Tuple variant flattening tests (issue #1468)
// =============================================================================

/// Test that help output for tuple variant subcommands shows flattened fields
/// instead of `--0 <STRUCTNAME>`.
///
/// This reproduces the issue from https://github.com/facet-rs/facet/issues/1468
/// where help showed `--0 <BUILDARGS>` instead of individual flags.
#[test]
fn test_tuple_variant_subcommand_help_flattening() {
    /// Build configuration options
    #[derive(Facet, Debug)]
    struct BuildArgs {
        /// Build in release mode
        #[facet(args::named, args::short = 'r')]
        release: bool,

        /// Disable spawning processes
        #[facet(args::named)]
        no_spawn: bool,

        /// Disable TUI mode
        #[facet(args::named)]
        no_tui: bool,
    }

    #[derive(Facet, Debug)]
    #[repr(u8)]
    enum Command {
        /// Build the project
        Build(BuildArgs),
        /// Run tests
        Test {
            /// Run in verbose mode
            #[facet(args::named, args::short = 'v')]
            verbose: bool,
        },
    }

    #[derive(Facet, Debug)]
    struct Args {
        #[facet(args::subcommand)]
        command: Command,
    }

    // Test help for the main command - should list subcommands
    let config = facet_args::HelpConfig {
        program_name: Some("myapp".to_string()),
        ..Default::default()
    };
    let help = facet_args::generate_help::<Args>(&config);
    insta::assert_snapshot!("tuple_variant_main_help", help);

    // Test help for the "build" subcommand - should show flattened BuildArgs fields
    let result = facet_args::from_slice::<Args>(&["build", "--help"]);
    assert!(result.is_err());
    let err = result.unwrap_err();
    assert!(err.is_help_request());
    let help = err.help_text().unwrap();

    // Should NOT contain `--0` (the tuple field index)
    assert!(
        !help.contains("--0"),
        "Help should not show --0 for tuple variant fields. Got:\n{help}"
    );

    // Should contain the flattened fields from BuildArgs
    assert!(
        help.contains("--release") || help.contains("-r"),
        "Help should show --release flag. Got:\n{help}"
    );
    assert!(
        help.contains("--no-spawn"),
        "Help should show --no-spawn flag. Got:\n{help}"
    );
    assert!(
        help.contains("--no-tui"),
        "Help should show --no-tui flag. Got:\n{help}"
    );

    insta::assert_snapshot!("tuple_variant_subcommand_help", help);
}
