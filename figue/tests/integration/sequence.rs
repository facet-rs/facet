use crate::assert_diag_snapshot;
use facet::Facet;
use figue as args;

#[test]
fn test_simplest_value_singleton_list_named() {
    #[derive(Facet, Debug, PartialEq)]
    struct Args {
        /// Names to greet
        #[facet(args::named, args::short = 'n')]
        names: Vec<String>,
    }

    // Test with multiple values (no delimiters)
    let args: Args = figue::from_slice(&["-n", "joe", "-n", "le", "-n", "rigolo"]).unwrap();

    assert_eq!(args.names, vec!["joe", "le", "rigolo"]);
}

#[test]
fn test_simplest_value_singleton_list_positional() {
    #[derive(Facet, Debug, PartialEq)]
    struct Args {
        /// Names to greet
        #[facet(args::positional)]
        names: Vec<String>,
    }

    // Test with multiple values (no delimiters)
    let args: Args = figue::from_slice(&["joe", "le", "rigolo"]).unwrap();

    assert_eq!(args.names, vec!["joe", "le", "rigolo"]);
}

#[test]
fn test_noargs_single_positional() {
    #[derive(Facet, Debug, PartialEq)]
    struct Args {
        /// The input file to process
        #[facet(args::positional)]
        input: String,
    }
    let err = figue::from_slice::<Args>(&[]).unwrap_err();
    assert_diag_snapshot!(err);
}

#[test]
fn test_noargs_vec_positional_default() {
    #[derive(Facet, Debug, PartialEq)]
    struct Args {
        /// Files to process
        #[facet(args::positional, default)]
        files: Vec<String>,
    }
    let args = figue::from_slice::<Args>(&[]).unwrap();
    assert!(args.files.is_empty());
}

#[test]
fn test_noargs_vec_positional_no_default() {
    #[derive(Facet, Debug, PartialEq)]
    struct Args {
        /// Files to process (at least one required)
        #[facet(args::positional)]
        files: Vec<String>,
    }
    let err = figue::from_slice::<Args>(&[]).unwrap_err();
    assert_diag_snapshot!(err);
}

#[test]
fn test_doubledash_nothing() {
    #[derive(Facet, Debug, PartialEq)]
    struct Args {}

    let _args = figue::from_slice::<Args>(&["--"]).unwrap();
}

#[test]
fn test_doubledash_flags_before_dd() {
    #[derive(Facet, Debug, PartialEq)]
    struct Args {
        /// Enable verbose output
        #[facet(args::named, default)]
        foo: bool,

        /// Run in debug mode
        #[facet(args::named, default)]
        bar: bool,

        /// Additional arguments
        #[facet(args::positional, default)]
        args: Vec<String>,
    }

    let err = figue::from_slice::<Args>(&["--foo", "--bar", "--baz"]).unwrap_err();
    assert_diag_snapshot!(err);
}

#[test]
fn test_doubledash_flags_across_dd() {
    #[derive(Facet, Debug, PartialEq)]
    struct Args {
        /// Enable verbose output
        #[facet(args::named, default)]
        foo: bool,

        /// Run in debug mode
        #[facet(args::named, default)]
        bar: bool,

        /// Additional arguments
        #[facet(args::positional, default)]
        args: Vec<String>,
    }

    let args = figue::from_slice::<Args>(&["--foo", "--bar", "--", "--baz"]).unwrap();
    assert_eq!(
        args,
        Args {
            foo: true,
            bar: true,
            args: vec!["--baz".to_string()],
        }
    );
}

#[test]
fn test_doubledash_flags_after_dd() {
    #[derive(Facet, Debug, PartialEq)]
    struct Args {
        /// Enable verbose output
        #[facet(args::named, default)]
        foo: bool,

        /// Run in debug mode
        #[facet(args::named, default)]
        bar: bool,

        /// Additional arguments
        #[facet(args::positional, default)]
        args: Vec<String>,
    }

    let args = figue::from_slice::<Args>(&["--", "--foo", "--bar", "--baz"]).unwrap();
    assert_eq!(
        args,
        Args {
            foo: false,
            bar: false,
            args: vec![
                "--foo".to_string(),
                "--bar".to_string(),
                "--baz".to_string()
            ],
        }
    );
}

/// Reproduces <https://github.com/facet-rs/facet/issues/1486>
/// facet-args treats arguments after -- as unexpected positional
#[test]
fn test_doubledash_with_positional_after_named() {
    #[derive(Facet, Debug, PartialEq)]
    struct Args {
        #[facet(args::short = 'r', args::named, default)]
        release: bool,

        /// Arguments to pass to the target program
        #[facet(args::positional, default)]
        ddc_args: Vec<String>,
    }

    // Simulate: xtask run -- serve --port 8888
    // (after the subcommand "run" has been consumed)
    let args = figue::from_slice::<Args>(&["--", "serve", "--port", "8888"]).unwrap();
    assert_eq!(
        args,
        Args {
            release: false,
            ddc_args: vec![
                "serve".to_string(),
                "--port".to_string(),
                "8888".to_string()
            ],
        }
    );
}

/// Reproduces <https://github.com/facet-rs/facet/issues/1486> with subcommand
/// facet-args treats arguments after -- as unexpected positional
#[test]
fn test_doubledash_with_subcommand_and_trailing_args() {
    #[derive(Facet, Debug, PartialEq)]
    struct RunArgs {
        #[facet(args::short = 'r', args::named, default)]
        release: bool,

        /// Arguments to pass to ddc
        #[facet(args::positional, default)]
        ddc_args: Vec<String>,
    }

    #[derive(Facet, Debug, PartialEq)]
    #[repr(u8)]
    enum Command {
        Run(RunArgs),
    }

    #[derive(Facet, Debug, PartialEq)]
    struct Args {
        #[facet(args::subcommand)]
        command: Command,
    }

    // Simulate: xtask run -- serve --port 8888
    let args = figue::from_slice::<Args>(&["run", "--", "serve", "--port", "8888"]).unwrap();
    assert_eq!(
        args,
        Args {
            command: Command::Run(RunArgs {
                release: false,
                ddc_args: vec![
                    "serve".to_string(),
                    "--port".to_string(),
                    "8888".to_string()
                ],
            }),
        }
    );
}
