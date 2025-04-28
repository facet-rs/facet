use facet::Facet;

use eyre::{Ok, Result};

#[test]
fn test_arg_parse() -> Result<()> {
    facet_testhelpers::setup();

    #[derive(Facet)]
    struct Args<'a> {
        #[facet(positional)]
        path: String,

        #[facet(positional)]
        path_borrow: &'a str,

        #[facet(named, short = 'v')]
        verbose: bool,

        #[facet(named, short = 'j')]
        concurrency: usize,

        #[facet(named, short = 'x')]
        consider_casing: usize,
    }

    let args: Args = facet_args::from_slice(&[
        "--verbose",
        "-j",
        "14",
        "--consider-casing",
        "0",
        "example.rs",
        "test.rs",
    ])?;
    assert!(args.verbose);
    assert_eq!(args.path, "example.rs");
    assert_eq!(args.path_borrow, "test.rs");
    assert_eq!(args.concurrency, 14);
    assert_eq!(args.consider_casing, 0);
    Ok(())
}

#[test]
fn test_missing_bool_is_false() -> Result<()> {
    facet_testhelpers::setup();

    #[derive(Facet)]
    struct Args {
        #[facet(named, short = 'v')]
        verbose: bool,
        #[facet(positional)]
        path: String,
    }
    let args: Args = facet_args::from_slice(&["absence_is_falsey.rs"])?;
    assert!(!args.verbose);
    Ok(())
}

#[test]
fn test_error_non_struct_type_not_supported() -> Result<()> {
    facet_testhelpers::setup();

    #[derive(Facet, Debug)]
    #[repr(u8)]
    #[allow(dead_code)]
    enum Args {
        Something,
        Else,
    }
    let args: Result<Args, _> = facet_args::from_slice(&["error", "wrong", "type"]);
    let err = args.unwrap_err();
    assert_eq!(err.message(), "Args error: Expected struct defintion");

    Ok(())
}

#[test]
fn test_error_missing_value_for_argument() -> Result<()> {
    facet_testhelpers::setup();

    #[derive(Facet, Debug)]
    struct Args {
        #[facet(named, short = 'j')]
        concurrency: usize,
    }
    let args: Result<Args, _> = facet_args::from_slice(&["--concurrency"]);
    let err = args.unwrap_err();
    assert_eq!(
        err.message(),
        "Args error: expected value after argument `concurrency`"
    );

    Ok(())
}

#[test]
fn test_error_missing_value_for_argument_short() -> Result<()> {
    facet_testhelpers::setup();

    #[derive(Facet, Debug)]
    struct Args {
        #[facet(named, short = 'j')]
        concurrency: usize,
    }
    let args: Result<Args, _> = facet_args::from_slice(&["-j"]);
    let err = args.unwrap_err();
    assert_eq!(
        err.message(),
        "Args error: expected value after argument `j`"
    );

    Ok(())
}

#[test]
fn test_error_unknown_argument() -> Result<()> {
    facet_testhelpers::setup();

    #[derive(Facet, Debug)]
    struct Args {
        #[facet(named, short = 'j')]
        concurrency: usize,
    }
    let args: Result<Args, _> = facet_args::from_slice(&["--c0ncurrency"]);
    let err = args.unwrap_err();
    assert_eq!(err.message(), "Args error: Unknown argument `c0ncurrency`");

    Ok(())
}
