use crate::assert_help_snapshot;
use facet::Facet;
use figue as args;
use figue::FigueBuiltins;

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
    #[facet(args::named, args::short = 'j', args::label = "count")]
    jobs: Option<usize>,

    /// Input file to process
    #[facet(args::positional)]
    input: String,

    /// Output file (optional)
    #[facet(default, args::positional)]
    output: Option<String>,

    /// Standard CLI options
    #[facet(flatten)]
    builtins: FigueBuiltins,
}

#[test]
fn test_help_simple_struct() {
    let config = figue::HelpConfig {
        program_name: Some("myapp".to_string()),
        version: Some("1.0.0".to_string()),
        ..Default::default()
    };
    let help = figue::generate_help::<SimpleArgs>(&config);
    assert_help_snapshot!(help);
}

/// Git-like CLI with subcommands
#[derive(Facet, Debug)]
struct GitArgs {
    /// Git command to run
    #[facet(args::subcommand)]
    command: GitCommand,

    /// Standard CLI options
    #[facet(flatten)]
    builtins: FigueBuiltins,
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

#[test]
fn test_help_with_subcommands() {
    let config = figue::HelpConfig {
        program_name: Some("git".to_string()),
        version: Some("2.40.0".to_string()),
        ..Default::default()
    };
    let help = figue::generate_help::<GitArgs>(&config);
    assert_help_snapshot!(help);
}

#[test]
fn test_help_enum_only() {
    let config = figue::HelpConfig {
        program_name: Some("git".to_string()),
        ..Default::default()
    };
    let help = figue::generate_help::<GitCommand>(&config);
    assert_help_snapshot!(help);
}

/// Test that --help and -h flags trigger help when FigueBuiltins is present
#[test]
fn test_help_flags() {
    // --help
    let result = figue::from_slice::<SimpleArgs>(&["--help"]);
    assert!(result.is_err());
    let err = result.unwrap_err();
    assert!(err.is_help());
    assert!(err.help_text().is_some());
    let help = err.help_text().unwrap();
    assert!(help.contains("USAGE"));
    assert!(help.contains("--verbose"));

    // -h
    let result = figue::from_slice::<SimpleArgs>(&["-h"]);
    assert!(result.is_err());
    let err = result.unwrap_err();
    assert!(err.is_help());
}

/// Test that help output for tuple variant subcommands shows flattened fields
/// instead of `--0 <STRUCTNAME>`.
#[test]
fn test_tuple_variant_subcommand_help_flattening() {
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
    #[allow(dead_code)]
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

        #[facet(flatten)]
        builtins: FigueBuiltins,
    }

    // Test help for the main command
    let config = figue::HelpConfig {
        program_name: Some("myapp".to_string()),
        ..Default::default()
    };
    let help = figue::generate_help::<Args>(&config);
    assert_help_snapshot!("tuple_variant_main_help", help);
}

// ------------------------------------------------------------------------
// Subcommand-aware help generation
// ------------------------------------------------------------------------
// When a user runs `myapp subcommand --help`, the help output should be
// tailored to that specific subcommand, not the root help.

/// CLI with subcommands for testing subcommand-specific help
#[derive(Facet, Debug)]
struct PkgManager {
    /// Package manager command
    #[facet(args::subcommand)]
    command: PkgCommand,

    /// Standard CLI options
    #[facet(flatten)]
    builtins: FigueBuiltins,
}

#[derive(Facet, Debug)]
#[repr(u8)]
#[allow(dead_code)]
enum PkgCommand {
    /// Install a package
    Install {
        /// Package name to install
        #[facet(args::positional)]
        package: String,

        /// Install globally
        #[facet(args::named, args::short = 'g')]
        global: bool,

        /// Force reinstall even if already installed
        #[facet(args::named, args::short = 'f')]
        force: bool,
    },
    /// Remove a package
    #[facet(rename = "rm")]
    Remove {
        /// Package name to remove
        #[facet(args::positional)]
        package: String,

        /// Don't ask for confirmation
        #[facet(args::named, args::short = 'y')]
        yes: bool,
    },
    /// List installed packages
    #[facet(rename = "ls")]
    List {
        /// Show all versions
        #[facet(args::named, args::short = 'a')]
        all: bool,

        /// Output as JSON
        #[facet(args::named)]
        json: bool,
    },
}

#[test]

fn test_help_subcommand_install() {
    // `pkg install --help` should show help specific to the install subcommand
    let result = figue::from_slice::<PkgManager>(&["install", "--help"]);
    assert!(result.is_err());
    let err = result.unwrap_err();
    assert!(err.is_help(), "expected help error, got: {:?}", err);

    let help = err.help_text().expect("should have help text");

    // Should show install-specific info
    assert!(help.contains("install"), "help should mention 'install'");
    assert!(
        help.contains("PACKAGE"),
        "help should show PACKAGE positional"
    );
    assert!(
        help.contains("--global") || help.contains("-g"),
        "help should show --global flag"
    );
    assert!(
        help.contains("--force") || help.contains("-f"),
        "help should show --force flag"
    );

    // Should NOT show other subcommands' options
    assert!(
        !help.contains("--yes"),
        "help should not show --yes from remove"
    );
    assert!(
        !help.contains("--json"),
        "help should not show --json from list"
    );

    assert_help_snapshot!("subcommand_install_help", help);
}

#[test]

fn test_help_subcommand_remove() {
    // `pkg rm --help` should show help specific to the remove subcommand
    let result = figue::from_slice::<PkgManager>(&["rm", "--help"]);
    assert!(result.is_err());
    let err = result.unwrap_err();
    assert!(err.is_help(), "expected help error, got: {:?}", err);

    let help = err.help_text().expect("should have help text");

    // Should show remove-specific info (note: renamed to "rm")
    assert!(
        help.contains("rm") || help.contains("remove"),
        "help should mention 'rm' or 'remove'"
    );
    assert!(
        help.contains("PACKAGE"),
        "help should show PACKAGE positional"
    );
    assert!(
        help.contains("--yes") || help.contains("-y"),
        "help should show --yes flag"
    );

    // Should NOT show other subcommands' options
    assert!(
        !help.contains("--global"),
        "help should not show --global from install"
    );
    assert!(
        !help.contains("--json"),
        "help should not show --json from list"
    );

    assert_help_snapshot!("subcommand_remove_help", help);
}

#[test]

fn test_help_subcommand_list() {
    // `pkg ls --help` should show help specific to the list subcommand
    let result = figue::from_slice::<PkgManager>(&["ls", "--help"]);
    assert!(result.is_err());
    let err = result.unwrap_err();
    assert!(err.is_help(), "expected help error, got: {:?}", err);

    let help = err.help_text().expect("should have help text");

    // Should show list-specific info (note: renamed to "ls")
    assert!(
        help.contains("ls") || help.contains("list"),
        "help should mention 'ls' or 'list'"
    );
    assert!(
        help.contains("--all") || help.contains("-a"),
        "help should show --all flag"
    );
    assert!(help.contains("--json"), "help should show --json flag");

    // Should NOT show other subcommands' options
    assert!(
        !help.contains("--global"),
        "help should not show --global from install"
    );
    assert!(
        !help.contains("--yes"),
        "help should not show --yes from remove"
    );

    assert_help_snapshot!("subcommand_list_help", help);
}

#[test]
fn test_help_root_shows_all_subcommands() {
    // `pkg --help` should show the root help with all subcommands listed
    let result = figue::from_slice::<PkgManager>(&["--help"]);
    assert!(result.is_err());
    let err = result.unwrap_err();
    assert!(err.is_help(), "expected help error, got: {:?}", err);

    let help = err.help_text().expect("should have help text");

    // Root help should list all subcommands
    assert!(help.contains("install"), "root help should list install");
    assert!(
        help.contains("rm"),
        "root help should list rm (renamed from remove)"
    );
    assert!(
        help.contains("ls"),
        "root help should list ls (renamed from list)"
    );

    assert_help_snapshot!("subcommand_root_help", help);
}

// Nested subcommands: help should be aware of the full path
#[derive(Facet, Debug)]
struct NestedCli {
    #[facet(args::subcommand)]
    command: TopLevel,

    #[facet(flatten)]
    builtins: FigueBuiltins,
}

#[derive(Facet, Debug)]
#[repr(u8)]
#[allow(dead_code)]
enum TopLevel {
    /// Manage repositories
    Repo {
        #[facet(args::subcommand)]
        action: RepoCmd,
    },
    /// Show version
    Version,
}

#[derive(Facet, Debug)]
#[repr(u8)]
#[allow(dead_code)]
enum RepoCmd {
    /// Clone a repository
    Clone {
        /// Repository URL
        #[facet(args::positional)]
        url: String,

        /// Clone depth (shallow clone)
        #[facet(args::named)]
        depth: Option<u32>,

        /// Branch to clone
        #[facet(args::named, args::short = 'b')]
        branch: Option<String>,
    },
    /// Push changes
    Push {
        /// Remote name
        #[facet(args::positional, default)]
        remote: Option<String>,

        /// Force push
        #[facet(args::named, args::short = 'f')]
        force: bool,
    },
}

#[test]

fn test_help_nested_subcommand_clone() {
    // `myapp repo clone --help` should show clone-specific help
    let result = figue::from_slice::<NestedCli>(&["repo", "clone", "--help"]);
    assert!(result.is_err());
    let err = result.unwrap_err();
    assert!(err.is_help(), "expected help error, got: {:?}", err);

    let help = err.help_text().expect("should have help text");

    // Should show clone-specific info
    assert!(help.contains("clone"), "help should mention 'clone'");
    assert!(help.contains("URL"), "help should show URL positional");
    assert!(help.contains("--depth"), "help should show --depth flag");
    assert!(
        help.contains("--branch") || help.contains("-b"),
        "help should show --branch flag"
    );

    // Should NOT show push's options
    assert!(
        !help.contains("--force"),
        "help should not show --force from push"
    );

    // Usage line should show the full path
    assert!(
        help.contains("repo") && help.contains("clone"),
        "usage should show full subcommand path"
    );

    assert_help_snapshot!("nested_subcommand_clone_help", help);
}

#[test]

fn test_help_nested_subcommand_push() {
    // `myapp repo push --help` should show push-specific help
    let result = figue::from_slice::<NestedCli>(&["repo", "push", "--help"]);
    assert!(result.is_err());
    let err = result.unwrap_err();
    assert!(err.is_help(), "expected help error, got: {:?}", err);

    let help = err.help_text().expect("should have help text");

    // Should show push-specific info
    assert!(help.contains("push"), "help should mention 'push'");
    assert!(
        help.contains("--force") || help.contains("-f"),
        "help should show --force flag"
    );

    // Should NOT show clone's options
    assert!(
        !help.contains("--depth"),
        "help should not show --depth from clone"
    );
    assert!(
        !help.contains("--branch"),
        "help should not show --branch from clone"
    );

    assert_help_snapshot!("nested_subcommand_push_help", help);
}

#[test]

fn test_help_nested_intermediate_level() {
    // `myapp repo --help` should show repo-level help (listing clone, push)
    let result = figue::from_slice::<NestedCli>(&["repo", "--help"]);
    assert!(result.is_err());
    let err = result.unwrap_err();
    assert!(err.is_help(), "expected help error, got: {:?}", err);

    let help = err.help_text().expect("should have help text");

    // Should show repo's subcommands
    assert!(help.contains("clone"), "help should list clone subcommand");
    assert!(help.contains("push"), "help should list push subcommand");

    // Should show repo in the usage
    assert!(help.contains("repo"), "usage should show 'repo'");

    assert_help_snapshot!("nested_intermediate_repo_help", help);
}

#[test]

fn test_help_short_flag_h_works_in_subcommand() {
    // `pkg install -h` should also work
    let result = figue::from_slice::<PkgManager>(&["install", "-h"]);
    assert!(result.is_err());
    let err = result.unwrap_err();
    assert!(
        err.is_help(),
        "expected help error for -h flag, got: {:?}",
        err
    );

    let help = err.help_text().expect("should have help text");

    // Should show install-specific flags (not just mention "install" in subcommand list)
    assert!(
        help.contains("--global") || help.contains("-g"),
        "help should show --global flag (install-specific)"
    );
    assert!(
        help.contains("--force") || help.contains("-f"),
        "help should show --force flag (install-specific)"
    );
    assert!(
        help.contains("PACKAGE"),
        "help should show PACKAGE positional"
    );

    // Should NOT show other subcommands' options
    assert!(
        !help.contains("--yes"),
        "help should not show --yes from remove"
    );
    assert!(
        !help.contains("--json"),
        "help should not show --json from list"
    );
}
