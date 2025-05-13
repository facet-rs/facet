use facet::Facet;
use facet_args::format::Cli;
use facet_testhelpers::test;

#[test]
fn test_cli_display() {
    // Create a Cli instance
    let cli = Cli;

    // Test the Display implementation
    let formatted = format!("{}", cli);
    assert_eq!(formatted, "Cli");
}

#[test]
fn test_arg_parse_easy() {
    #[derive(Facet)]
    struct Args {
        #[facet(positional)]
        path: String,

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
    ])?;
    assert!(args.verbose);
    assert_eq!(args.path, "example.rs");
    assert_eq!(args.concurrency, 14);
    assert_eq!(args.consider_casing, 0);
}

#[test]
fn test_arg_parse() {
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
}

#[test]
fn test_arg_parse_nums() {
    #[derive(Facet)]
    struct Args {
        #[facet(named, short)]
        x: i64,

        #[facet(named, short)]
        y: u64,

        #[facet(named, short = "z")]
        zzz: f64,
    }

    let args: Args = facet_args::from_slice(&["-x", "1", "-y", "2", "-z", "3"])?;
    assert_eq!(args.x, 1);
    assert_eq!(args.y, 2);
    assert_eq!(args.zzz, 3.0);
}

// Not yet supported
#[test]
#[ignore]
fn test_arg_parse_list() {
    // Define a struct with a list field
    #[derive(Facet)]
    struct Args {
        #[facet(named, short = "n")]
        nums: Vec<i64>,
    }

    // Test parsing string list
    let string_args: Args = facet_args::from_slice(&["-n", "1", "-n", "2", "-n", "3"])?;

    // Verify the integer list was parsed correctly
    assert_eq!(string_args.nums, vec![1, 2, 3]);
}

#[test]
fn test_missing_bool_is_false() {
    #[derive(Facet)]
    struct Args {
        #[facet(named, short = 'v')]
        verbose: bool,
        #[facet(positional)]
        path: String,
    }
    let args: Args = facet_args::from_slice(&["absence_is_falsey.rs"])?;
    assert!(!args.verbose);
}

#[test]
fn test_missing_default() {
    #[derive(Facet, Debug)]
    struct Args {
        #[facet(positional, default = 42)]
        answer: usize,
        #[facet(named, short = 'p')]
        path: String,
    }

    let args: Args = facet_args::from_slice(&["-p", "absence_uses_default.rs"])?;
    assert_eq!(args.answer, 42);
    assert_eq!(args.path, "absence_uses_default.rs".to_string());

    let args: Args = facet_args::from_slice(&["100", "-p", "presence_overrides_default.rs"])?;
    assert_eq!(args.answer, 100);
    assert_eq!(args.path, "presence_overrides_default.rs".to_string());
}

#[test]
fn test_missing_default_fn() {
    // Could be done e.g. using `num_cpus::get()`, but just mock it as 2 + 2 = 4
    fn default_concurrency() -> usize {
        2 + 2
    }

    #[derive(Facet, Debug)]
    struct Args {
        #[facet(named, short = 'p')]
        path: String,
        #[facet(named, short = 'j', default = default_concurrency())]
        concurrency: usize,
    }

    let args: Args = facet_args::from_slice(&["-p", "absence_uses_default_fn.rs"])?;
    assert_eq!(args.path, "absence_uses_default_fn.rs".to_string());
    assert_eq!(args.concurrency, 4);

    let args: Args =
        facet_args::from_slice(&["-p", "presence_overrides_default_fn.rs", "-j", "2"])?;
    assert_eq!(args.path, "presence_overrides_default_fn.rs".to_string());
    assert_eq!(args.concurrency, 2);
}
