use facet::Facet;
use facet_args as args;
use facet_args::HelpConfig;

#[test]
fn test_counted_short_chain() {
    #[derive(Facet, Debug)]
    struct Args {
        #[facet(args::named, args::short = 'v', args::counted)]
        verbose: u8,
    }

    let args: Args = facet_args::from_slice(&["-vvv"]).unwrap();
    assert_eq!(args.verbose, 3);
}

#[test]
fn test_counted_short_separate() {
    #[derive(Facet, Debug)]
    struct Args {
        #[facet(args::named, args::short = 'v', args::counted)]
        verbose: u8,
    }

    let args: Args = facet_args::from_slice(&["-v", "-v"]).unwrap();
    assert_eq!(args.verbose, 2);
}

#[test]
fn test_counted_long_flags() {
    #[derive(Facet, Debug)]
    struct Args {
        #[facet(args::named, args::counted)]
        verbose: u8,
    }

    let args: Args = facet_args::from_slice(&["--verbose", "--verbose", "--verbose"]).unwrap();
    assert_eq!(args.verbose, 3);
}

#[test]
fn test_counted_mixed_short_long() {
    #[derive(Facet, Debug)]
    struct Args {
        #[facet(args::named, args::short = 'v', args::counted)]
        verbose: u8,
    }

    let args: Args = facet_args::from_slice(&["-v", "--verbose", "-v"]).unwrap();
    assert_eq!(args.verbose, 3);
}

#[test]
fn test_counted_default_zero() {
    #[derive(Facet, Debug)]
    struct Args {
        #[facet(args::named, args::short = 'v', args::counted)]
        verbose: u8,

        #[facet(args::positional)]
        path: String,
    }

    let args: Args = facet_args::from_slice(&["file.txt"]).unwrap();
    assert_eq!(args.verbose, 0);
    assert_eq!(args.path, "file.txt");
}

#[test]
fn test_counted_usize() {
    #[derive(Facet, Debug)]
    struct Args {
        #[facet(args::named, args::short = 'v', args::counted)]
        verbose: usize,
    }

    let args: Args = facet_args::from_slice(&["-vvvv"]).unwrap();
    assert_eq!(args.verbose, 4);
}

#[test]
fn test_counted_i32() {
    #[derive(Facet, Debug)]
    struct Args {
        #[facet(args::named, args::short = 'v', args::counted)]
        verbose: i32,
    }

    let args: Args = facet_args::from_slice(&["-vv"]).unwrap();
    assert_eq!(args.verbose, 2);
}

#[test]
fn test_counted_with_other_flags() {
    #[derive(Facet, Debug)]
    struct Args {
        #[facet(args::named, args::short = 'v', args::counted)]
        verbose: u8,

        #[facet(args::named, args::short = 'q')]
        quiet: bool,

        #[facet(args::named, args::short = 'j')]
        jobs: usize,

        #[facet(args::positional)]
        path: String,
    }

    let args: Args = facet_args::from_slice(&["-vvv", "-q", "-j", "4", "file.txt"]).unwrap();
    assert_eq!(args.verbose, 3);
    assert!(args.quiet);
    assert_eq!(args.jobs, 4);
    assert_eq!(args.path, "file.txt");
}

#[test]
fn test_counted_chain_with_bool() {
    #[derive(Facet, Debug)]
    struct Args {
        #[facet(args::named, args::short = 'v', args::counted)]
        verbose: u8,

        #[facet(args::named, args::short = 'q')]
        quiet: bool,
    }

    let args: Args = facet_args::from_slice(&["-vvq"]).unwrap();
    assert_eq!(args.verbose, 2);
    assert!(args.quiet);
}

#[test]
fn test_counted_chain_bool_then_counted() {
    #[derive(Facet, Debug)]
    struct Args {
        #[facet(args::named, args::short = 'q')]
        quiet: bool,

        #[facet(args::named, args::short = 'v', args::counted)]
        verbose: u8,
    }

    let args: Args = facet_args::from_slice(&["-qvvv"]).unwrap();
    assert!(args.quiet);
    assert_eq!(args.verbose, 3);
}

#[test]
fn test_counted_in_subcommand() {
    #[derive(Facet, Debug)]
    #[repr(u8)]
    #[allow(dead_code)]
    enum Command {
        Build {
            #[facet(args::named, args::short = 'v', args::counted)]
            verbose: u8,
        },
    }

    #[derive(Facet, Debug)]
    struct Args {
        #[facet(args::subcommand)]
        command: Command,
    }

    let args: Args = facet_args::from_slice(&["build", "-vvv"]).unwrap();
    match args.command {
        Command::Build { verbose } => {
            assert_eq!(verbose, 3);
        }
    }
}

#[test]
fn test_multiple_counted_fields() {
    #[derive(Facet, Debug)]
    struct Args {
        #[facet(args::named, args::short = 'v', args::counted)]
        verbose: u8,

        #[facet(args::named, args::short = 'd', args::counted)]
        debug: u8,
    }

    let args: Args = facet_args::from_slice(&["-vvv", "-dd"]).unwrap();
    assert_eq!(args.verbose, 3);
    assert_eq!(args.debug, 2);
}

#[test]
fn test_counted_interleaved() {
    #[derive(Facet, Debug)]
    struct Args {
        #[facet(args::named, args::short = 'v', args::counted)]
        verbose: u8,

        #[facet(args::named, args::short = 'd', args::counted)]
        debug: u8,
    }

    let args: Args = facet_args::from_slice(&["-v", "-d", "-v", "-d", "-v"]).unwrap();
    assert_eq!(args.verbose, 3);
    assert_eq!(args.debug, 2);
}

#[test]
fn test_counted_help_shows_repeated_hint() {
    #[derive(Facet, Debug)]
    struct Args {
        #[facet(args::named, args::short = 'v', args::counted)]
        verbose: u8,

        #[facet(args::named, args::short = 'q')]
        quiet: bool,
    }

    let config = HelpConfig::default();
    let help = args::generate_help::<Args>(&config);

    assert!(help.contains("[can be repeated]"));
    assert!(help.contains("--verbose"));
    assert!(!help.contains("<U8>"));
}
