use crate::assert_diag_snapshot;
use facet::Facet;
use facet_args as args;

/// Test chaining simple boolean flags
#[test]
fn test_bool_chain_simple() {
    #[derive(Facet)]
    struct Args {
        #[facet(args::named, args::short = 'a')]
        flag_a: bool,

        #[facet(args::named, args::short = 'b')]
        flag_b: bool,

        #[facet(args::named, args::short = 'c')]
        flag_c: bool,
    }

    // Test `-abc` → `-a -b -c`
    let args: Args = facet_args::from_slice(&["-abc"]).unwrap();
    assert!(args.flag_a);
    assert!(args.flag_b);
    assert!(args.flag_c);
}

/// Test repeated boolean flags for Vec
#[test]
fn test_bool_repeated() {
    #[derive(Facet)]
    struct Args {
        #[facet(args::named, args::short = 'v')]
        verbose: Vec<bool>,
    }

    // Test `-vvv` → three true values
    let args: Args = facet_args::from_slice(&["-vvv"]).unwrap();
    assert_eq!(args.verbose, vec![true, true, true]);
}

/// Test chaining bool flags followed by a value flag
#[test]
fn test_bool_chain_with_value() {
    #[derive(Facet)]
    struct Args {
        #[facet(args::named, args::short = 'a')]
        flag_a: bool,

        #[facet(args::named, args::short = 'b')]
        flag_b: bool,

        #[facet(args::named, args::short = 'j')]
        jobs: usize,
    }

    // Test `-abj4` → `-a -b -j 4`
    let args: Args = facet_args::from_slice(&["-abj4"]).unwrap();
    assert!(args.flag_a);
    assert!(args.flag_b);
    assert_eq!(args.jobs, 4);
}

/// Test backward compatibility: non-bool flag with attached value
#[test]
fn test_backward_compat_attached_value() {
    #[derive(Facet)]
    struct Args {
        #[facet(args::named, args::short = 'j')]
        jobs: usize,

        #[facet(args::positional)]
        path: String,
    }

    // Test `-j4` still works (no bool before it)
    let args: Args = facet_args::from_slice(&["-j4", "file.txt"]).unwrap();
    assert_eq!(args.jobs, 4);
    assert_eq!(args.path, "file.txt");
}

/// Test chaining with separated value flag
#[test]
fn test_bool_chain_with_separated_value() {
    #[derive(Facet)]
    struct Args {
        #[facet(args::named, args::short = 'a')]
        flag_a: bool,

        #[facet(args::named, args::short = 'b')]
        flag_b: bool,

        #[facet(args::named, args::short = 'j')]
        jobs: usize,
    }

    // Test `-ab -j 4` (space-separated value)
    let args: Args = facet_args::from_slice(&["-ab", "-j", "4"]).unwrap();
    assert!(args.flag_a);
    assert!(args.flag_b);
    assert_eq!(args.jobs, 4);
}

/// Test error when unknown flag is in chain
#[test]
fn test_error_unknown_in_chain() {
    #[derive(Facet, Debug)]
    struct Args {
        #[facet(args::named, args::short = 'a')]
        flag_a: bool,

        #[facet(args::named, args::short = 'c')]
        flag_c: bool,
    }

    // Test `-axc` where 'x' doesn't exist
    let result: Result<Args, _> = facet_args::from_slice(&["-axc"]);
    let err = result.unwrap_err();
    assert_diag_snapshot!(err);
}

/// Test mixing long flags with chained short flags
#[test]
fn test_mixed_long_and_chained() {
    #[derive(Facet)]
    struct Args {
        #[facet(args::named)]
        verbose: bool,

        #[facet(args::named, args::short = 'a')]
        flag_a: bool,

        #[facet(args::named, args::short = 'b')]
        flag_b: bool,
    }

    // Test `--verbose -ab`
    let args: Args = facet_args::from_slice(&["--verbose", "-ab"]).unwrap();
    assert!(args.verbose);
    assert!(args.flag_a);
    assert!(args.flag_b);
}

/// Test chaining with equals syntax
#[test]
fn test_chain_with_equals() {
    #[derive(Facet)]
    struct Args {
        #[facet(args::named, args::short = 'a')]
        flag_a: bool,
    }

    // Test `-a=true` (should still work)
    let args: Args = facet_args::from_slice(&["-a=true"]).unwrap();
    assert!(args.flag_a);
}

/// Test chaining in enum variant fields
#[test]
fn test_chain_in_variant() {
    #[derive(Facet, Debug, PartialEq)]
    #[repr(u8)]
    enum Command {
        #[allow(dead_code)]
        Build {
            #[facet(args::named, args::short = 'r')]
            release: bool,

            #[facet(args::named, args::short = 'v')]
            verbose: bool,
        },
    }

    // Test `build -rv`
    #[derive(Facet, Debug, PartialEq)]
    struct Args {
        #[facet(args::subcommand)]
        command: Command,
    }

    let args: Args = facet_args::from_slice(&["build", "-rv"]).unwrap();
    match args.command {
        Command::Build { release, verbose } => {
            assert!(release);
            assert!(verbose);
        }
    }
}

/// Test single char chain
#[test]
fn test_single_char_chain() {
    #[derive(Facet)]
    struct Args {
        #[facet(args::named, args::short = 'v')]
        verbose: bool,
    }

    // Test `-v` still works as simple flag
    let args: Args = facet_args::from_slice(&["-v"]).unwrap();
    assert!(args.verbose);
}

/// Test chaining with implicit short flags (first char of field name)
#[test]
fn test_chain_implicit_short() {
    #[derive(Facet)]
    struct Args {
        #[facet(args::named, args::short)]
        alpha: bool,

        #[facet(args::named, args::short)]
        beta: bool,
    }

    // Test `-ab` using implicit shorts (a from alpha, b from beta)
    let args: Args = facet_args::from_slice(&["-ab"]).unwrap();
    assert!(args.alpha);
    assert!(args.beta);
}

/// Test long chain of boolean flags
#[test]
fn test_long_chain() {
    #[derive(Facet)]
    struct Args {
        #[facet(args::named, args::short = 'a')]
        flag_a: bool,

        #[facet(args::named, args::short = 'b')]
        flag_b: bool,

        #[facet(args::named, args::short = 'c')]
        flag_c: bool,

        #[facet(args::named, args::short = 'd')]
        flag_d: bool,

        #[facet(args::named, args::short = 'e')]
        flag_e: bool,
    }

    // Test `-abcde`
    let args: Args = facet_args::from_slice(&["-abcde"]).unwrap();
    assert!(args.flag_a);
    assert!(args.flag_b);
    assert!(args.flag_c);
    assert!(args.flag_d);
    assert!(args.flag_e);
}

/// Test chaining with positional arguments
#[test]
fn test_chain_with_positional() {
    #[derive(Facet)]
    struct Args {
        #[facet(args::named, args::short = 'a')]
        flag_a: bool,

        #[facet(args::named, args::short = 'b')]
        flag_b: bool,

        #[facet(args::positional)]
        path: String,
    }

    // Test `-ab file.txt`
    let args: Args = facet_args::from_slice(&["-ab", "file.txt"]).unwrap();
    assert!(args.flag_a);
    assert!(args.flag_b);
    assert_eq!(args.path, "file.txt");
}
