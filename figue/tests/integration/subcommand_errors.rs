//! Tests for subcommand validation error messages
//!
//! These tests cover the scenarios where subcommands have missing required fields,
//! unexpected arguments, or other validation issues that should produce helpful
//! error messages.

use crate::assert_diag_snapshot;
use facet::Facet;
use figue as args;

// ============================================================================
// Test Case 1: Missing required positional argument in subcommand
// ============================================================================

#[test]
fn test_subcommand_missing_required_positional() {
    /// Main CLI with subcommands
    #[derive(Facet, Debug)]
    struct Cli {
        #[facet(args::subcommand)]
        command: Command,

        #[facet(flatten)]
        builtins: args::FigueBuiltins,
    }

    #[derive(Facet, Debug)]
    #[repr(u8)]
    #[allow(dead_code)]
    enum Command {
        /// Initialize a new project
        Init(InitArgs),
        /// Build the project
        Build(BuildArgs),
    }

    #[derive(Facet, Debug)]
    struct InitArgs {
        /// Project name (creates directory with this name)
        #[facet(args::positional)]
        name: String,

        /// Template to use (skips interactive selection)
        #[facet(args::named, args::short = 't', default)]
        template: Option<String>,
    }

    #[derive(Facet, Debug)]
    struct BuildArgs {
        /// Build in release mode
        #[facet(args::named, args::short = 'r', default)]
        release: bool,
    }

    // User types: "ddc init" (without the required positional "name")
    // Should give a helpful error like "Missing required argument: <name>"
    // NOT "Error: missing field `name` in type `InitArgs`"
    let err = figue::from_slice::<Cli>(&["init"]).unwrap_err();
    assert_diag_snapshot!(err);
}

// ============================================================================
// Test Case 2: Subcommand with one positional that gets cancelled/interrupted
// ============================================================================

#[test]
fn test_subcommand_with_partial_input() {
    #[derive(Facet, Debug)]
    struct Cli {
        #[facet(args::subcommand)]
        command: Command,

        #[facet(flatten)]
        builtins: args::FigueBuiltins,
    }

    #[derive(Facet, Debug)]
    #[repr(u8)]
    #[allow(dead_code)]
    enum Command {
        Init(InitArgs),
    }

    #[derive(Facet, Debug)]
    struct InitArgs {
        /// Project name
        #[facet(args::positional)]
        name: String,

        /// Project description
        #[facet(args::positional)]
        description: String,
    }

    // User types: "ddc init myproject" (missing second positional)
    // Should clearly indicate which argument is missing
    let err = figue::from_slice::<Cli>(&["init", "myproject"]).unwrap_err();
    assert_diag_snapshot!(err);
}

// ============================================================================
// Test Case 3: Subcommand with unexpected extra positional argument
// ============================================================================

#[test]
fn test_subcommand_unexpected_extra_positional() {
    #[derive(Facet, Debug)]
    struct Cli {
        #[facet(args::subcommand)]
        command: Command,

        #[facet(flatten)]
        builtins: args::FigueBuiltins,
    }

    #[derive(Facet, Debug)]
    #[repr(u8)]
    #[allow(dead_code)]
    enum Command {
        Init(InitArgs),
    }

    #[derive(Facet, Debug)]
    struct InitArgs {
        /// Project name
        #[facet(args::positional)]
        name: String,
    }

    // User types: "ddc init myproject extra-arg"
    // Should indicate unexpected argument and show what was already parsed
    let err = figue::from_slice::<Cli>(&["init", "myproject", "extra-arg"]).unwrap_err();
    assert_diag_snapshot!(err);
}

// ============================================================================
// Test Case 4: Multiple missing positional arguments
// ============================================================================

#[test]
fn test_subcommand_multiple_missing_positionals() {
    #[derive(Facet, Debug)]
    struct Cli {
        #[facet(args::subcommand)]
        command: Command,
    }

    #[derive(Facet, Debug)]
    #[repr(u8)]
    #[allow(dead_code)]
    enum Command {
        Create(CreateArgs),
    }

    #[derive(Facet, Debug)]
    struct CreateArgs {
        /// Resource type (e.g., user, project)
        #[facet(args::positional)]
        resource_type: String,

        /// Resource name
        #[facet(args::positional)]
        resource_name: String,

        /// Resource location
        #[facet(args::positional)]
        location: String,
    }

    // User types just the subcommand with no arguments
    let err = figue::from_slice::<Cli>(&["create"]).unwrap_err();
    assert_diag_snapshot!(err);
}

// ============================================================================
// Test Case 5: Missing required named argument in subcommand
// ============================================================================

#[test]
fn test_subcommand_missing_required_named() {
    #[derive(Facet, Debug)]
    struct Cli {
        #[facet(args::subcommand)]
        command: Command,
    }

    #[derive(Facet, Debug)]
    #[repr(u8)]
    #[allow(dead_code)]
    enum Command {
        Connect(ConnectArgs),
    }

    #[derive(Facet, Debug)]
    struct ConnectArgs {
        /// Server URL (required)
        #[facet(args::named)]
        url: String,

        /// Optional timeout
        #[facet(args::named, default)]
        timeout: Option<u32>,
    }

    // User types: "cli connect" without --url
    let err = figue::from_slice::<Cli>(&["connect"]).unwrap_err();
    assert_diag_snapshot!(err);
}

// ============================================================================
// Test Case 6: Wrong subcommand name with suggestion
// ============================================================================

#[test]
fn test_unknown_subcommand_with_suggestion() {
    #[derive(Facet, Debug)]
    struct Cli {
        #[facet(args::subcommand)]
        command: Command,
    }

    #[derive(Facet, Debug)]
    #[repr(u8)]
    #[allow(dead_code)]
    enum Command {
        /// Initialize a new project
        Init(InitArgs),
        /// Build the project
        Build(BuildArgs),
    }

    #[derive(Facet, Debug)]
    struct InitArgs {
        #[facet(args::positional)]
        name: String,
    }

    #[derive(Facet, Debug)]
    struct BuildArgs {
        #[facet(args::named, default)]
        release: bool,
    }

    // User types: "cli int" (typo for "init")
    let err = figue::from_slice::<Cli>(&["int"]).unwrap_err();
    assert_diag_snapshot!(err);
}

// ============================================================================
// Test Case 7: Nested subcommands with missing argument
// ============================================================================

#[test]
fn test_nested_subcommand_missing_required() {
    #[derive(Facet, Debug)]
    struct Cli {
        #[facet(args::subcommand)]
        command: Command,
    }

    #[derive(Facet, Debug)]
    #[repr(u8)]
    #[allow(dead_code)]
    enum Command {
        /// Repository operations
        Repo(RepoArgs),
    }

    #[derive(Facet, Debug)]
    struct RepoArgs {
        #[facet(args::subcommand)]
        action: RepoAction,
    }

    #[derive(Facet, Debug)]
    #[repr(u8)]
    #[allow(dead_code)]
    enum RepoAction {
        /// Clone a repository
        Clone(CloneArgs),
        /// Push changes
        Push,
    }

    #[derive(Facet, Debug)]
    struct CloneArgs {
        /// Repository URL
        #[facet(args::positional)]
        url: String,
    }

    // User types: "cli repo clone" without URL
    let err = figue::from_slice::<Cli>(&["repo", "clone"]).unwrap_err();
    assert_diag_snapshot!(err);
}

// ============================================================================
// Test Case 8: Subcommand with mixed missing arguments
// ============================================================================

#[test]
fn test_subcommand_mixed_missing_arguments() {
    #[derive(Facet, Debug)]
    struct Cli {
        #[facet(args::subcommand)]
        command: Command,
    }

    #[derive(Facet, Debug)]
    #[repr(u8)]
    #[allow(dead_code)]
    enum Command {
        Deploy(DeployArgs),
    }

    #[derive(Facet, Debug)]
    struct DeployArgs {
        /// Environment to deploy to
        #[facet(args::positional)]
        environment: String,

        /// Deployment region (required)
        #[facet(args::named)]
        region: String,

        /// Optional version tag
        #[facet(args::named, default)]
        version: Option<String>,
    }

    // User types: "cli deploy production" (has positional but missing --region)
    let err = figue::from_slice::<Cli>(&["deploy", "production"]).unwrap_err();
    assert_diag_snapshot!(err);
}

// ============================================================================
// Test Case 9: Empty command line (no subcommand provided)
// ============================================================================

#[test]
fn test_no_subcommand_provided() {
    #[derive(Facet, Debug)]
    struct Cli {
        #[facet(args::subcommand)]
        command: Command,

        #[facet(flatten)]
        builtins: args::FigueBuiltins,
    }

    #[derive(Facet, Debug)]
    #[repr(u8)]
    #[allow(dead_code)]
    enum Command {
        Init(InitArgs),
        Build(BuildArgs),
    }

    #[derive(Facet, Debug)]
    struct InitArgs {
        #[facet(args::positional)]
        name: String,
    }

    #[derive(Facet, Debug)]
    struct BuildArgs {
        #[facet(args::named, default)]
        release: bool,
    }

    // User types just "cli" with no subcommand
    let err = figue::from_slice::<Cli>(&[]).unwrap_err();
    assert_diag_snapshot!(err);
}

// ============================================================================
// Test Case 10: Subcommand with value that looks like flag
// ============================================================================

#[test]
fn test_subcommand_positional_looks_like_flag() {
    #[derive(Facet, Debug)]
    struct Cli {
        #[facet(args::subcommand)]
        command: Command,
    }

    #[derive(Facet, Debug)]
    #[repr(u8)]
    #[allow(dead_code)]
    enum Command {
        Create(CreateArgs),
    }

    #[derive(Facet, Debug)]
    struct CreateArgs {
        /// Resource name (can start with -)
        #[facet(args::positional)]
        name: String,

        /// Optional description
        #[facet(args::named, default)]
        description: Option<String>,
    }

    // User types: "cli create --help" but --help is unknown in this context
    // (FigueBuiltins not included, so --help is not a valid flag)
    let err = figue::from_slice::<Cli>(&["create", "--help"]).unwrap_err();
    assert_diag_snapshot!(err);
}
