#![allow(dead_code)]

use facet::Facet;
use facet_args as args;

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

    // Test "init" subcommand
    let cmd: Command = facet_args::from_slice(&["init", "my-project"]).unwrap();
    assert_eq!(
        cmd,
        Command::Init {
            name: "my-project".to_string()
        }
    );

    // Test "build" subcommand
    let cmd: Command = facet_args::from_slice(&["build", "--release"]).unwrap();
    assert_eq!(cmd, Command::Build { release: true });

    // Test "build" subcommand without flag
    let cmd: Command = facet_args::from_slice(&["build"]).unwrap();
    assert_eq!(cmd, Command::Build { release: false });
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

    // Variant names should be converted to kebab-case
    let cmd: Command = facet_args::from_slice(&["run-tests", "--verbose"]).unwrap();
    assert_eq!(cmd, Command::RunTests { verbose: true });

    let cmd: Command = facet_args::from_slice(&["clean-build"]).unwrap();
    assert_eq!(cmd, Command::CleanBuild);
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
    let cmd: Command = facet_args::from_slice(&["clone", "https://example.com/repo"]).unwrap();
    assert_eq!(
        cmd,
        Command::Clone {
            url: "https://example.com/repo".to_string()
        }
    );

    // Nested subcommand
    let cmd: Command =
        facet_args::from_slice(&["remote", "add", "origin", "https://example.com/repo"]).unwrap();
    assert_eq!(
        cmd,
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
        Start,
        Stop,
    }

    let result: Result<Command, _> = facet_args::from_slice(&["unknown"]);
    assert!(result.is_err());
}

/// Test error when required subcommand is missing
#[test]
fn test_missing_subcommand_error() {
    #[derive(Facet, Debug)]
    #[repr(u8)]
    enum Command {
        Start,
        Stop,
    }

    #[derive(Facet, Debug)]
    struct Args {
        #[facet(args::subcommand)]
        command: Command,
    }

    let result: Result<Args, _> = facet_args::from_slice(&[]);
    assert!(result.is_err());
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

    let cmd: Command = facet_args::from_slice(&["ls"]).unwrap();
    assert_eq!(cmd, Command::List);

    let cmd: Command = facet_args::from_slice(&["rm", "file.txt"]).unwrap();
    assert_eq!(
        cmd,
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

    let cmd: Command = facet_args::from_slice(&["status"]).unwrap();
    assert_eq!(cmd, Command::Status);

    let cmd: Command = facet_args::from_slice(&["version"]).unwrap();
    assert_eq!(cmd, Command::Version);
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
