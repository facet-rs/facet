use facet::Facet;

#[test]
fn test_arg_parse() {
    facet_testhelpers::setup();

    #[derive(Facet)]
    struct Args {
        #[facet(positional)]
        path: String,

        #[facet(named, short = 'v')]
        verbose: bool,

        #[facet(named, short = 'j')]
        concurrency: usize,
    }

    let args: Args = facet_args::from_slice(&["--verbose", "-j", "14", "example.rs"]);
    assert!(args.verbose);
    assert_eq!(args.path, "example.rs");
    assert_eq!(args.concurrency, 14);
}
