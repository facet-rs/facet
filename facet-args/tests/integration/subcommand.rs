#![allow(dead_code)]

use crate::assert_diag_snapshot;
use facet::Facet;
use facet_args as args;

/// Test tuple variant subcommand with struct payload (like clap's automatic flattening)
/// When a subcommand variant is `Add(AddArgs)` instead of `Add { name: String }`,
/// the struct's fields should be automatically flattened as the subcommand's arguments.
#[test]
fn test_tuple_variant_subcommand_flattening() {
    /// Shared arguments for the bench command
    #[derive(Facet, Debug, PartialEq, Default)]
    struct BenchArgs {
        /// Filter to run only specific benchmark(s)
        #[facet(args::positional, default)]
        filter: Option<String>,

        /// Start HTTP server to view the report
        #[facet(args::named)]
        serve: bool,

        /// Skip running benchmarks
        #[facet(args::named)]
        no_run: bool,
    }

    #[derive(Facet, Debug, PartialEq)]
    #[repr(u8)]
    enum Command {
        /// Run benchmarks and generate report
        Bench(BenchArgs),
        /// Other command with inline fields
        Other {
            #[facet(args::positional)]
            name: String,
        },
    }

    #[derive(Facet, Debug)]
    struct Args {
        #[facet(args::subcommand)]
        command: Command,
    }

    // Test with all arguments
    let args: Args = facet_args::from_slice(&["bench", "booleans", "--serve", "--no-run"]).unwrap();
    assert_eq!(
        args.command,
        Command::Bench(BenchArgs {
            filter: Some("booleans".to_string()),
            serve: true,
            no_run: true,
        })
    );

    // Test with just the subcommand (defaults)
    let args: Args = facet_args::from_slice(&["bench"]).unwrap();
    assert_eq!(
        args.command,
        Command::Bench(BenchArgs {
            filter: None,
            serve: false,
            no_run: false,
        })
    );

    // Test flags in different order
    let args: Args = facet_args::from_slice(&["bench", "--serve", "booleans"]).unwrap();
    assert_eq!(
        args.command,
        Command::Bench(BenchArgs {
            filter: Some("booleans".to_string()),
            serve: true,
            no_run: false,
        })
    );

    // Test the other (struct variant) still works
    let args: Args = facet_args::from_slice(&["other", "test"]).unwrap();
    assert_eq!(
        args.command,
        Command::Other {
            name: "test".to_string()
        }
    );
}

/// Test basic subcommand parsing with an enum where each variant is a subcommand.
#[test]
fn test_subcommand_basic() {
    #[derive(Facet, Debug, PartialEq)]
    #[repr(u8)]
    enum Command {
        /// Initialize a new project
        Init {
            #[facet(args::positional)]
            name: String,
        },
        /// Build the project
        Build {
            #[facet(args::named, args::short = 'r')]
            release: bool,
        },
    }

    #[derive(Facet, Debug, PartialEq)]
    struct Args {
        #[facet(args::subcommand)]
        command: Command,
    }

    // Test "init" subcommand
    let args: Args = facet_args::from_slice(&["init", "my-project"]).unwrap();
    assert_eq!(
        args.command,
        Command::Init {
            name: "my-project".to_string()
        }
    );

    // Test "build" subcommand
    let args: Args = facet_args::from_slice(&["build", "--release"]).unwrap();
    assert_eq!(args.command, Command::Build { release: true });

    // Test "build" subcommand without flag
    let args: Args = facet_args::from_slice(&["build"]).unwrap();
    assert_eq!(args.command, Command::Build { release: false });
}

/// Test subcommand with kebab-case variant names
#[test]
fn test_subcommand_kebab_case() {
    #[derive(Facet, Debug, PartialEq)]
    #[repr(u8)]
    enum Command {
        /// Run all tests
        RunTests {
            #[facet(args::named, args::short = 'v')]
            verbose: bool,
        },
        /// Clean build artifacts
        CleanBuild,
    }

    #[derive(Facet, Debug, PartialEq)]
    struct Args {
        #[facet(args::subcommand)]
        command: Command,
    }

    // Variant names should be converted to kebab-case
    let args: Args = facet_args::from_slice(&["run-tests", "--verbose"]).unwrap();
    assert_eq!(args.command, Command::RunTests { verbose: true });

    let args: Args = facet_args::from_slice(&["clean-build"]).unwrap();
    assert_eq!(args.command, Command::CleanBuild);
}

/// Test struct with a subcommand field
#[test]
fn test_struct_with_subcommand() {
    #[derive(Facet, Debug, PartialEq)]
    #[repr(u8)]
    enum SubCommand {
        Add {
            #[facet(args::positional)]
            item: String,
        },
        Remove {
            #[facet(args::positional)]
            item: String,
        },
    }

    #[derive(Facet, Debug)]
    struct Args {
        #[facet(args::named, args::short = 'v')]
        verbose: bool,

        #[facet(args::subcommand)]
        command: SubCommand,
    }

    // Global flags before subcommand
    let args: Args = facet_args::from_slice(&["-v", "add", "foo"]).unwrap();
    assert!(args.verbose);
    assert_eq!(
        args.command,
        SubCommand::Add {
            item: "foo".to_string()
        }
    );

    // Subcommand without global flags
    let args: Args = facet_args::from_slice(&["remove", "bar"]).unwrap();
    assert!(!args.verbose);
    assert_eq!(
        args.command,
        SubCommand::Remove {
            item: "bar".to_string()
        }
    );
}

/// Test optional subcommand
#[test]
fn test_optional_subcommand() {
    #[derive(Facet, Debug, PartialEq)]
    #[repr(u8)]
    enum SubCommand {
        Status,
        Info,
    }

    #[derive(Facet, Debug)]
    struct Args {
        #[facet(args::named)]
        version: bool,

        #[facet(args::subcommand)]
        command: Option<SubCommand>,
    }

    // With subcommand
    let args: Args = facet_args::from_slice(&["status"]).unwrap();
    assert_eq!(args.command, Some(SubCommand::Status));

    // Without subcommand (just flags)
    let args: Args = facet_args::from_slice(&["--version"]).unwrap();
    assert!(args.version);
    assert_eq!(args.command, None);

    // Empty args
    let args: Args = facet_args::from_slice(&[]).unwrap();
    assert!(!args.version);
    assert_eq!(args.command, None);
}

/// Test nested subcommands
#[test]
fn test_nested_subcommands() {
    #[derive(Facet, Debug, PartialEq)]
    #[repr(u8)]
    enum RemoteCommand {
        Add {
            #[facet(args::positional)]
            name: String,
            #[facet(args::positional)]
            url: String,
        },
        Remove {
            #[facet(args::positional)]
            name: String,
        },
    }

    #[derive(Facet, Debug, PartialEq)]
    #[repr(u8)]
    enum Command {
        Clone {
            #[facet(args::positional)]
            url: String,
        },
        Remote {
            #[facet(args::subcommand)]
            action: RemoteCommand,
        },
    }

    // Top-level subcommand
    #[derive(Facet, Debug, PartialEq)]
    struct Args {
        #[facet(args::subcommand)]
        command: Command,
    }

    let args: Args = facet_args::from_slice(&["clone", "https://example.com/repo"]).unwrap();
    assert_eq!(
        args.command,
        Command::Clone {
            url: "https://example.com/repo".to_string()
        }
    );

    // Nested subcommand
    let args: Args =
        facet_args::from_slice(&["remote", "add", "origin", "https://example.com/repo"]).unwrap();
    assert_eq!(
        args.command,
        Command::Remote {
            action: RemoteCommand::Add {
                name: "origin".to_string(),
                url: "https://example.com/repo".to_string()
            }
        }
    );
}

/// Test error when unknown subcommand is provided
#[test]
fn test_unknown_subcommand_error() {
    #[derive(Facet, Debug)]
    #[repr(u8)]
    enum Command {
        /// Start the service
        Start,
        /// Stop the service
        Stop,
    }

    #[derive(Facet, Debug)]
    struct Args {
        #[facet(args::subcommand)]
        command: Command,
    }

    let result: Result<Args, _> = facet_args::from_slice(&["unknown"]);
    let err = result.unwrap_err();
    assert_diag_snapshot!(err);
}

/// Test error when required subcommand is missing
#[test]
fn test_missing_subcommand_error() {
    #[derive(Facet, Debug)]
    #[repr(u8)]
    enum Command {
        /// Start the service
        Start,
        /// Stop the service
        Stop,
    }

    #[derive(Facet, Debug)]
    struct Args {
        #[facet(args::subcommand)]
        command: Command,
    }

    let result: Result<Args, _> = facet_args::from_slice(&[]);
    let err = result.unwrap_err();
    assert_diag_snapshot!(err);
}

/// Test error when nested subcommand is missing (issue #1195 scenario)
#[test]
fn test_missing_nested_subcommand_error() {
    #[derive(Debug, Facet)]
    struct Args {
        #[facet(args::subcommand)]
        command: Command,
    }

    #[derive(Debug, Facet)]
    #[repr(u8)]
    enum Command {
        /// CI workflow management
        Ci {
            #[facet(args::subcommand)]
            action: CiAction,
        },
    }

    #[derive(Debug, Facet)]
    #[repr(u8)]
    enum CiAction {
        /// Generate CI workflow files from Rust code
        Generate {
            /// Check if files are up to date instead of generating
            #[facet(args::named, default)]
            check: bool,
        },
    }

    // Test missing nested subcommand - should show helpful error
    let result: Result<Args, _> = facet_args::from_slice(&["ci"]);
    let err = result.unwrap_err();
    assert_diag_snapshot!(err);
}

/// Test error when wrong argument style is used for nested subcommand (issue #1195 scenario)
#[test]
fn test_wrong_argument_style_for_nested_subcommand() {
    #[derive(Debug, Facet)]
    struct Args {
        #[facet(args::subcommand)]
        command: Command,
    }

    #[derive(Debug, Facet)]
    #[repr(u8)]
    enum Command {
        /// CI workflow management
        Ci {
            #[facet(args::subcommand)]
            action: CiAction,
        },
    }

    #[derive(Debug, Facet)]
    #[repr(u8)]
    enum CiAction {
        /// Generate CI workflow files from Rust code
        Generate {
            /// Check if files are up to date instead of generating
            #[facet(args::named, default)]
            check: bool,
        },
    }

    // Test wrong argument style - should suggest correct subcommand usage
    let result: Result<Args, _> = facet_args::from_slice(&["ci", "--action", "gen"]);
    let err = result.unwrap_err();
    assert_diag_snapshot!(err);
}

/// Test error when unknown nested subcommand is provided
#[test]
fn test_unknown_nested_subcommand_error() {
    #[derive(Debug, Facet)]
    struct Args {
        #[facet(args::subcommand)]
        command: Command,
    }

    #[derive(Debug, Facet)]
    #[repr(u8)]
    enum Command {
        /// CI workflow management
        Ci {
            #[facet(args::subcommand)]
            action: CiAction,
        },
    }

    #[derive(Debug, Facet)]
    #[repr(u8)]
    enum CiAction {
        /// Generate CI workflow files from Rust code
        Generate {
            /// Check if files are up to date instead of generating
            #[facet(args::named, default)]
            check: bool,
        },
    }

    // Test unknown nested subcommand
    let result: Result<Args, _> = facet_args::from_slice(&["ci", "unknown"]);
    let err = result.unwrap_err();
    assert_diag_snapshot!(err);
}

/// Test subcommand with renamed variant
#[test]
fn test_subcommand_rename() {
    #[derive(Facet, Debug, PartialEq)]
    #[repr(u8)]
    enum Command {
        #[facet(rename = "ls")]
        List,
        #[facet(rename = "rm")]
        Remove {
            #[facet(args::positional)]
            path: String,
        },
    }

    #[derive(Facet, Debug, PartialEq)]
    struct Args {
        #[facet(args::subcommand)]
        command: Command,
    }

    let args: Args = facet_args::from_slice(&["ls"]).unwrap();
    assert_eq!(args.command, Command::List);

    let args: Args = facet_args::from_slice(&["rm", "file.txt"]).unwrap();
    assert_eq!(
        args.command,
        Command::Remove {
            path: "file.txt".to_string()
        }
    );
}

/// Test unit variant subcommand (no fields)
#[test]
fn test_unit_variant_subcommand() {
    #[derive(Facet, Debug, PartialEq)]
    #[repr(u8)]
    enum Command {
        Status,
        Version,
        Help,
    }

    #[derive(Facet, Debug, PartialEq)]
    struct Args {
        #[facet(args::subcommand)]
        command: Command,
    }

    let args: Args = facet_args::from_slice(&["status"]).unwrap();
    assert_eq!(args.command, Command::Status);

    let args: Args = facet_args::from_slice(&["version"]).unwrap();
    assert_eq!(args.command, Command::Version);
}

/// Regression test for issue #1193: enum variant with String default value
#[test]
fn test_enum_variant_string_default() {
    #[derive(Facet, Debug, PartialEq)]
    #[repr(u8)]
    enum Command {
        E2e {
            // String literals work directly via Into<String>
            #[facet(args::named, default = "0.0.0-test")]
            version: String,
        },
    }

    #[derive(Facet, Debug, PartialEq)]
    struct Args {
        #[facet(args::subcommand)]
        command: Command,
    }

    // Without --version, should use the default
    let args: Args = facet_args::from_slice(&["e2e"]).unwrap();
    assert_eq!(
        args.command,
        Command::E2e {
            version: "0.0.0-test".to_string()
        }
    );

    // With --version, should override the default
    let args: Args = facet_args::from_slice(&["e2e", "--version", "1.2.3"]).unwrap();
    assert_eq!(
        args.command,
        Command::E2e {
            version: "1.2.3".to_string()
        }
    );
}

/// Test nested subcommands wrapped in a struct (bug reproduction)
/// This is different from test_nested_subcommands because the outer type is a struct, not an enum
#[test]
fn test_nested_subcommands_in_struct() {
    #[derive(Facet, Debug, PartialEq)]
    #[repr(u8)]
    enum GrammarsAction {
        Vendor {
            #[facet(args::positional)]
            url: String,
        },
        Update {
            #[facet(default, args::positional)]
            name: Option<String>,
        },
        Generate {
            #[facet(default, args::positional)]
            name: Option<String>,
        },
    }

    #[derive(Facet, Debug, PartialEq)]
    #[repr(u8)]
    enum Command {
        Grammars {
            #[facet(args::subcommand)]
            action: GrammarsAction,
        },
    }

    #[derive(Facet, Debug)]
    struct Args {
        #[facet(args::subcommand)]
        command: Command,
    }

    // This should work: struct -> subcommand enum -> variant with nested subcommand -> subcommand enum
    let args: Args = facet_args::from_slice(&["grammars", "generate"]).unwrap();
    match args.command {
        Command::Grammars { action } => {
            assert_eq!(action, GrammarsAction::Generate { name: None });
        }
    }

    // With positional argument
    let args: Args =
        facet_args::from_slice(&["grammars", "vendor", "https://example.com"]).unwrap();
    match args.command {
        Command::Grammars { action } => {
            assert_eq!(
                action,
                GrammarsAction::Vendor {
                    url: "https://example.com".to_string()
                }
            );
        }
    }
}
