use facet_pretty::FacetPretty;

#[test]
fn test_arg_parse() {
    facet_testhelpers::setup();
    use facet::Facet;

    #[derive(Facet)]
    struct Args {
        #[facet(positional)]
        path: String,

        #[facet(named, short = 'v')]
        verbose: bool,

        #[facet(named, short = 'j')]
        concurrency: usize,
    }

    let args: Args = facet_args::from_slice(&["--verbose", "--concurrency", "14", "example.rs"]);
    eprintln!("args: {}", args.pretty());
}
