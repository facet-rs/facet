use facet::Facet;

#[test]
fn test_arg_parse() {
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
    }

    let args: Args = facet_args::from_slice(&["--verbose", "-j", "14", "example.rs", "test.rs"]);
    assert!(args.verbose);
    assert_eq!(args.path, "example.rs");
    assert_eq!(args.path_borrow, "test.rs");
    assert_eq!(args.concurrency, 14);
}
