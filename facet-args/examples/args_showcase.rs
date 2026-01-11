//! facet-args Showcase
//!
//! This example demonstrates facet-args' capabilities for command-line argument
//! parsing, including rich error diagnostics, help generation, and shell completions.
//!
//! Run with: cargo run -p facet-args --example args_showcase --features ariadne

use facet::Facet;
use facet_args::{self as args, ArgsErrorWithInput};
use facet_showcase::{Language, Scenario, ShowcaseRunner};

/// Extension trait for displaying args parse results with Ariadne error formatting.
trait ArgsResultExt<'a, 'b, T: facet::Facet<'b>> {
    /// Display the result - success values with facet-pretty, errors with Ariadne.
    fn args_result(self, result: &'b Result<T, ArgsErrorWithInput>) -> Self;
}

impl<'a, 'b, T: facet::Facet<'b>> ArgsResultExt<'a, 'b, T> for Scenario<'a> {
    fn args_result(self, result: &'b Result<T, ArgsErrorWithInput>) -> Self {
        match result {
            Ok(value) => self.success(value),
            Err(err) if err.is_help_request() => {
                // Help text is not an error, display it as output (contains ANSI colors)
                self.ansi_output(err.help_text().unwrap_or(""))
            }
            Err(err) => {
                // Use Ariadne for pretty error display
                self.ansi_output(&err.to_ariadne_string())
            }
        }
    }
}

// =============================================================================
// Type Definitions
// =============================================================================

/// A simple CLI tool for file processing.
#[derive(Facet, Debug)]
struct SimpleArgs {
    /// Enable verbose output
    #[facet(args::named, args::short = 'v')]
    verbose: bool,

    /// Number of parallel jobs to run
    #[facet(default, args::named, args::short = 'j')]
    jobs: Option<usize>,

    /// Input file to process
    #[facet(args::positional)]
    input: String,

    /// Output file (defaults to stdout)
    #[facet(default, args::positional)]
    output: Option<String>,
}

/// Git-like CLI with subcommands.
#[derive(Facet, Debug)]
struct GitLikeArgs {
    /// Show version information
    #[facet(args::named)]
    version: bool,

    /// Git command to run
    #[facet(args::subcommand)]
    command: GitCommand,
}

/// Available commands
#[derive(Facet, Debug)]
#[repr(u8)]
#[allow(dead_code)]
enum GitCommand {
    /// Clone a repository into a new directory
    Clone {
        /// The repository URL to clone
        #[facet(args::positional)]
        url: String,

        /// Directory to clone into
        #[facet(default, args::positional)]
        directory: Option<String>,

        /// Clone only the specified branch
        #[facet(default, args::named, args::short = 'b')]
        branch: Option<String>,

        /// Create a shallow clone with limited history
        #[facet(default, args::named)]
        depth: Option<usize>,
    },
    /// Show the working tree status
    Status {
        /// Show short-format output
        #[facet(args::named, args::short = 's')]
        short: bool,

        /// Show the branch even in short-format
        #[facet(args::named, args::short = 'b')]
        branch: bool,
    },
    /// Manage set of tracked repositories
    Remote {
        /// Remote action to perform
        #[facet(args::subcommand)]
        action: RemoteAction,
    },
}

/// Remote management commands
#[derive(Facet, Debug)]
#[repr(u8)]
#[allow(dead_code)]
enum RemoteAction {
    /// Add a remote named <name> for the repository at <url>
    Add {
        /// Name of the remote
        #[facet(args::positional)]
        name: String,

        /// URL of the remote repository
        #[facet(args::positional)]
        url: String,
    },
    /// Remove the remote named <name>
    #[facet(rename = "rm")]
    Remove {
        /// Name of the remote to remove
        #[facet(args::positional)]
        name: String,
    },
    /// List all remotes
    #[facet(rename = "ls")]
    List {
        /// Show remote URLs after names
        #[facet(args::named, args::short = 'v')]
        verbose: bool,
    },
}

/// A build tool configuration
#[derive(Facet, Debug)]
struct BuildArgs {
    /// Build in release mode with optimizations
    #[facet(args::named, args::short = 'r')]
    release: bool,

    /// Number of parallel jobs
    #[facet(default, args::named, args::short = 'j')]
    jobs: Option<usize>,

    /// Package to build
    #[facet(default, args::named, args::short = 'p')]
    package: Option<String>,

    /// Build all packages in the workspace
    #[facet(args::named)]
    workspace: bool,

    /// Space-separated list of features to enable
    #[facet(default, args::named, args::short = 'F')]
    features: Option<String>,

    /// Target triple to build for
    #[facet(default, args::named)]
    target: Option<String>,
}

fn main() {
    let mut runner = ShowcaseRunner::new("Args").language(Language::Rust);

    runner.header();
    runner.intro("[`facet-args`](https://docs.rs/facet-args) turns any `Facet` struct into a command-line interface. Define your CLI with doc comments and attributes like `args::named`, `args::positional`, and `args::subcommand`. Get auto-generated help text, shell completions for bash/zsh/fish, and rich error diagnostics with typo suggestions.");

    // =========================================================================
    // PART 1: Successful Parsing
    // =========================================================================
    runner.section("Successful Parsing");

    showcase_simple_parsing(&mut runner);
    showcase_attached_short_value(&mut runner);
    showcase_bool_equals_value(&mut runner);
    showcase_short_flag_chaining(&mut runner);
    showcase_subcommand_parsing(&mut runner);
    showcase_nested_subcommands(&mut runner);

    // =========================================================================
    // PART 2: Help Generation
    // =========================================================================
    runner.section("Help Generation");

    showcase_help_simple(&mut runner);
    showcase_help_auto_detection(&mut runner);
    showcase_help_subcommands(&mut runner);

    // =========================================================================
    // PART 3: Shell Completions
    // =========================================================================
    runner.section("Shell Completions");

    showcase_completions_bash(&mut runner);
    showcase_completions_zsh(&mut runner);
    showcase_completions_fish(&mut runner);

    // =========================================================================
    // PART 4: Error Diagnostics
    // =========================================================================
    runner.section("Error Diagnostics");

    scenario_unknown_flag(&mut runner);
    scenario_unknown_flag_suggestion(&mut runner);
    scenario_invalid_short_flag_in_chain(&mut runner);
    scenario_triple_dash(&mut runner);
    scenario_single_dash_long_name(&mut runner);
    scenario_missing_value(&mut runner);
    scenario_missing_required_arg(&mut runner);
    scenario_unexpected_positional(&mut runner);
    scenario_unknown_subcommand(&mut runner);
    scenario_missing_subcommand(&mut runner);
    scenario_missing_nested_subcommand_arg(&mut runner);
    scenario_invalid_value(&mut runner);

    runner.footer();
}

// =============================================================================
// Successful Parsing Scenarios
// =============================================================================

fn showcase_simple_parsing(runner: &mut ShowcaseRunner) {
    runner
        .scenario("Simple Arguments")
        .description("Parse a struct with flags, options, and positional arguments.")
        .target_type::<SimpleArgs>()
        .input(
            Language::Rust,
            "from_slice(&[\"-v\", \"-j\", \"4\", \"input.txt\", \"output.txt\"])",
        )
        .args_result(&args::from_slice::<SimpleArgs>(&[
            "-v",
            "-j",
            "4",
            "input.txt",
            "output.txt",
        ]))
        .finish();
}

fn showcase_attached_short_value(runner: &mut ShowcaseRunner) {
    runner
        .scenario("Attached Short Flag Value")
        .description("Short flags can have their values attached directly without a space.")
        .target_type::<SimpleArgs>()
        .input(Language::Rust, "from_slice(&[\"-j4\", \"input.txt\"])")
        .args_result(&args::from_slice::<SimpleArgs>(&["-j4", "input.txt"]))
        .finish();
}

fn showcase_bool_equals_value(runner: &mut ShowcaseRunner) {
    runner
        .scenario("Boolean Flag with Explicit Value")
        .description("Boolean flags can be explicitly set to true or false using `=`.")
        .target_type::<SimpleArgs>()
        .input(
            Language::Rust,
            "from_slice(&[\"--verbose=true\", \"input.txt\"])",
        )
        .args_result(&args::from_slice::<SimpleArgs>(&[
            "--verbose=true",
            "input.txt",
        ]))
        .finish();
}

fn showcase_subcommand_parsing(runner: &mut ShowcaseRunner) {
    runner
        .scenario("Subcommands")
        .description("Parse a CLI with subcommands, each with their own arguments.")
        .target_type::<GitLikeArgs>()
        .input(
            Language::Rust,
            "from_slice(&[\"clone\", \"--branch\", \"main\", \"https://github.com/user/repo\"])",
        )
        .args_result(&args::from_slice::<GitLikeArgs>(&[
            "clone",
            "--branch",
            "main",
            "https://github.com/user/repo",
        ]))
        .finish();
}

fn showcase_nested_subcommands(runner: &mut ShowcaseRunner) {
    runner
        .scenario("Nested Subcommands")
        .description("Parse deeply nested subcommands like `git remote add`.")
        .target_type::<GitLikeArgs>()
        .input(
            Language::Rust,
            "from_slice(&[\"remote\", \"add\", \"origin\", \"https://github.com/user/repo\"])",
        )
        .args_result(&args::from_slice::<GitLikeArgs>(&[
            "remote",
            "add",
            "origin",
            "https://github.com/user/repo",
        ]))
        .finish();
}

fn showcase_short_flag_chaining(runner: &mut ShowcaseRunner) {
    runner
        .scenario("Short Flag Chaining")
        .description(
            "Multiple boolean short flags can be combined: `-sb` is equivalent to `-s -b`.",
        )
        .target_type::<GitLikeArgs>()
        .input(Language::Rust, "from_slice(&[\"status\", \"-sb\"])")
        .args_result(&args::from_slice::<GitLikeArgs>(&["status", "-sb"]))
        .finish();
}

// =============================================================================
// Help Generation Scenarios
// =============================================================================

fn showcase_help_simple(runner: &mut ShowcaseRunner) {
    let config = args::HelpConfig {
        program_name: Some("mytool".to_string()),
        version: Some("1.0.0".to_string()),
        ..Default::default()
    };
    let help = args::generate_help::<SimpleArgs>(&config);

    runner
        .scenario("Simple Help")
        .description("Auto-generated help text from struct definition and doc comments.")
        .target_type::<SimpleArgs>()
        .ansi_output(&help)
        .finish();
}

fn showcase_help_auto_detection(runner: &mut ShowcaseRunner) {
    let result: Result<SimpleArgs, _> = args::from_slice(&["--help"]);

    runner
        .scenario("Automatic --help Detection")
        .description("When `-h`, `--help`, `-help`, or `/?` is the first argument, help is automatically generated and returned.")
        .target_type::<SimpleArgs>()
        .input(Language::Rust, "from_slice(&[\"--help\"])")
        .args_result(&result)
        .finish();
}

fn showcase_help_subcommands(runner: &mut ShowcaseRunner) {
    let config = args::HelpConfig {
        program_name: Some("git".to_string()),
        version: Some("2.40.0".to_string()),
        ..Default::default()
    };
    let help = args::generate_help::<GitLikeArgs>(&config);

    runner
        .scenario("Help with Subcommands")
        .description("Help text automatically lists available subcommands with descriptions.")
        .target_type::<GitLikeArgs>()
        .ansi_output(&help)
        .finish();
}

// =============================================================================
// Shell Completion Scenarios
// =============================================================================

fn showcase_completions_bash(runner: &mut ShowcaseRunner) {
    let completions = args::generate_completions::<BuildArgs>(args::Shell::Bash, "cargo-build");

    runner
        .scenario("Bash Completions")
        .description("Generated Bash completion script for tab-completion support.")
        .target_type::<BuildArgs>()
        .serialized_output(Language::Plain, &completions)
        .finish();
}

fn showcase_completions_zsh(runner: &mut ShowcaseRunner) {
    let completions = args::generate_completions::<BuildArgs>(args::Shell::Zsh, "cargo-build");

    runner
        .scenario("Zsh Completions")
        .description("Generated Zsh completion script with argument descriptions.")
        .target_type::<BuildArgs>()
        .serialized_output(Language::Plain, &completions)
        .finish();
}

fn showcase_completions_fish(runner: &mut ShowcaseRunner) {
    let completions = args::generate_completions::<BuildArgs>(args::Shell::Fish, "cargo-build");

    runner
        .scenario("Fish Completions")
        .description("Generated Fish shell completion script.")
        .target_type::<BuildArgs>()
        .serialized_output(Language::Plain, &completions)
        .finish();
}

// =============================================================================
// Error Diagnostic Scenarios
// =============================================================================

fn scenario_unknown_flag(runner: &mut ShowcaseRunner) {
    let result: Result<SimpleArgs, _> = args::from_slice(&["--verbos", "input.txt"]);

    runner
        .scenario("Unknown Flag")
        .description("Error when an unrecognized flag is provided.")
        .target_type::<SimpleArgs>()
        .input(Language::Rust, "from_slice(&[\"--verbos\", \"input.txt\"])")
        .args_result(&result)
        .finish();
}

fn scenario_unknown_flag_suggestion(runner: &mut ShowcaseRunner) {
    let result: Result<BuildArgs, _> = args::from_slice(&["--releas"]);

    runner
        .scenario("Unknown Flag with Suggestion")
        .description("When the flag name is close to a valid one, a suggestion is offered.")
        .target_type::<BuildArgs>()
        .input(Language::Rust, "from_slice(&[\"--releas\"])")
        .args_result(&result)
        .finish();
}

fn scenario_invalid_short_flag_in_chain(runner: &mut ShowcaseRunner) {
    let result: Result<SimpleArgs, _> = args::from_slice(&["-vxyz", "input.txt"]);

    runner
        .scenario("Invalid Short Flag in Chain")
        .description(
            "When chaining short flags, an unknown flag is reported with available options.",
        )
        .target_type::<SimpleArgs>()
        .input(Language::Rust, "from_slice(&[\"-vxyz\", \"input.txt\"])")
        .args_result(&result)
        .finish();
}

fn scenario_triple_dash(runner: &mut ShowcaseRunner) {
    let result: Result<SimpleArgs, _> = args::from_slice(&["---verbose", "input.txt"]);

    runner
        .scenario("Triple Dash Flag")
        .description("Flags with too many dashes are rejected.")
        .target_type::<SimpleArgs>()
        .input(
            Language::Rust,
            "from_slice(&[\"---verbose\", \"input.txt\"])",
        )
        .args_result(&result)
        .finish();
}

fn scenario_single_dash_long_name(runner: &mut ShowcaseRunner) {
    let result: Result<SimpleArgs, _> = args::from_slice(&["-verbose", "input.txt"]);

    runner
        .scenario("Single Dash with Long Name")
        .description("Long flag names require double dashes.")
        .target_type::<SimpleArgs>()
        .input(Language::Rust, "from_slice(&[\"-verbose\", \"input.txt\"])")
        .args_result(&result)
        .finish();
}

fn scenario_missing_value(runner: &mut ShowcaseRunner) {
    let result: Result<SimpleArgs, _> = args::from_slice(&["-j"]);

    runner
        .scenario("Missing Value")
        .description("Error when a flag that requires a value doesn't get one.")
        .target_type::<SimpleArgs>()
        .input(Language::Rust, "from_slice(&[\"-j\"])")
        .args_result(&result)
        .finish();
}

fn scenario_missing_required_arg(runner: &mut ShowcaseRunner) {
    let result: Result<SimpleArgs, _> = args::from_slice(&["-v"]);

    runner
        .scenario("Missing Required Argument")
        .description("Error when a required positional argument is not provided.")
        .target_type::<SimpleArgs>()
        .input(Language::Rust, "from_slice(&[\"-v\"])")
        .args_result(&result)
        .finish();
}

fn scenario_unexpected_positional(runner: &mut ShowcaseRunner) {
    let result: Result<BuildArgs, _> = args::from_slice(&["extra", "--release"]);

    runner
        .scenario("Unexpected Positional Argument")
        .description("Error when a positional argument is provided but not expected.")
        .target_type::<BuildArgs>()
        .input(Language::Rust, "from_slice(&[\"extra\", \"--release\"])")
        .args_result(&result)
        .finish();
}

fn scenario_unknown_subcommand(runner: &mut ShowcaseRunner) {
    let result: Result<GitLikeArgs, _> = args::from_slice(&["clon", "https://example.com"]);

    runner
        .scenario("Unknown Subcommand")
        .description(
            "Error when an unrecognized subcommand is provided, with available options listed.",
        )
        .target_type::<GitLikeArgs>()
        .input(
            Language::Rust,
            "from_slice(&[\"clon\", \"https://example.com\"])",
        )
        .args_result(&result)
        .finish();
}

fn scenario_missing_subcommand(runner: &mut ShowcaseRunner) {
    let result: Result<GitLikeArgs, _> = args::from_slice(&["--version"]);

    runner
        .scenario("Missing Subcommand")
        .description("Error when a required subcommand is not provided.")
        .target_type::<GitLikeArgs>()
        .input(Language::Rust, "from_slice(&[\"--version\"])")
        .args_result(&result)
        .finish();
}

fn scenario_missing_nested_subcommand_arg(runner: &mut ShowcaseRunner) {
    let result: Result<GitLikeArgs, _> = args::from_slice(&["remote", "add", "origin"]);

    runner
        .scenario("Missing Nested Subcommand Argument")
        .description("Error when a required argument in a nested subcommand is missing.")
        .target_type::<GitLikeArgs>()
        .input(
            Language::Rust,
            "from_slice(&[\"remote\", \"add\", \"origin\"])",
        )
        .args_result(&result)
        .finish();
}

fn scenario_invalid_value(runner: &mut ShowcaseRunner) {
    let result: Result<SimpleArgs, _> = args::from_slice(&["-j", "not-a-number", "input.txt"]);

    runner
        .scenario("Invalid Value Type")
        .description("Error when a value cannot be parsed as the expected type.")
        .target_type::<SimpleArgs>()
        .input(
            Language::Rust,
            "from_slice(&[\"-j\", \"not-a-number\", \"input.txt\"])",
        )
        .args_result(&result)
        .finish();
}
