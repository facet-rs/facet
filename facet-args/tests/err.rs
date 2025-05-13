use facet::Facet;
use facet_testhelpers::test;

#[test]
fn test_error_non_struct_type_not_supported() {
    #[derive(Facet, Debug)]
    #[repr(u8)]
    #[allow(dead_code)]
    enum Args {
        Something,
        Else,
    }
    let args: Result<Args, _> = facet_args::from_slice(&["error", "wrong", "type"]);
    let err = args.unwrap_err();
    insta::assert_snapshot!(err);
    // Args error: Expected struct type
}

#[test]
fn test_error_missing_value_for_argument() {
    #[derive(Facet, Debug)]
    struct Args {
        #[facet(named, short = 'j')]
        concurrency: usize,
    }
    let args: Result<Args, _> = facet_args::from_slice(&["--concurrency"]);
    let err = args.unwrap_err();
    insta::assert_snapshot!(err);
    // assert_eq!(
    //     err.message(),
    //     "Args error: expected value after argument `concurrency`"
    // );
}

#[test]
fn test_error_missing_value_for_argument_short_missed() {
    #[derive(Facet, Debug)]
    struct Args {
        #[facet(named, short = 'j')]
        concurrency: usize,
        #[facet(named, short = 'v')]
        verbose: bool,
    }
    let args: Result<Args, _> = facet_args::from_slice(&["-j", "-v"]);
    let err = args.unwrap_err();
    insta::assert_snapshot!(err);
    // assert_eq!(
    //     err.message(),
    //     "Args error: expected value after argument `j`"
    // );
}

#[test]
fn test_error_missing_value_for_argument_short_eof() {
    #[derive(Facet, Debug)]
    struct Args {
        #[facet(named, short = 'j')]
        concurrency: usize,
    }
    let args: Result<Args, _> = facet_args::from_slice(&["-j"]);
    let err = args.unwrap_err();
    insta::assert_snapshot!(err);
    // assert_eq!(
    //     err.message(),
    //     "Args error: expected value after argument `j`"
    // );
}

#[test]
fn test_error_unknown_argument() {
    #[derive(Facet, Debug)]
    struct Args {
        #[facet(named, short = 'j')]
        concurrency: usize,
    }
    let args: Result<Args, _> = facet_args::from_slice(&["--c0ncurrency"]);
    let err = args.unwrap_err();
    insta::assert_snapshot!(err);
    // assert_eq!(err.message(), "Args error: Unknown argument `c0ncurrency`");
}
