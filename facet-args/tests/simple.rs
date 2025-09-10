use facet::Facet;
use facet_testhelpers::test;

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
    ])
    .unwrap();
    assert!(args.verbose);
    assert_eq!(args.path, "example.rs");
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

    let args: Args = facet_args::from_slice(&["-x", "1", "-y", "2", "-z", "3"]).unwrap();
    assert_eq!(args.x, 1);
    assert_eq!(args.y, 2);
    assert_eq!(args.zzz, 3.0);
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
    let args: Args = facet_args::from_slice(&["absence_is_falsey.rs"]).unwrap();
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

    let args: Args = facet_args::from_slice(&["-p", "absence_uses_default.rs"]).unwrap();
    assert_eq!(args.answer, 42);
    assert_eq!(args.path, "absence_uses_default.rs".to_string());

    let args: Args =
        facet_args::from_slice(&["100", "-p", "presence_overrides_default.rs"]).unwrap();
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

    let args: Args = facet_args::from_slice(&["-p", "absence_uses_default_fn.rs"]).unwrap();
    assert_eq!(args.path, "absence_uses_default_fn.rs".to_string());
    assert_eq!(args.concurrency, 4);

    let args: Args =
        facet_args::from_slice(&["-p", "presence_overrides_default_fn.rs", "-j", "2"]).unwrap();
    assert_eq!(args.path, "presence_overrides_default_fn.rs".to_string());
    assert_eq!(args.concurrency, 2);
}

#[test]
fn test_inf_float_parsing() {
    #[derive(Facet, Debug)]
    struct Args {
        #[facet(named)]
        rate: f64,
    }
    let args: Args = facet_args::from_slice(&["--rate", "infinity"]).unwrap();
    assert_eq!(args.rate, f64::INFINITY);
}

#[test]
fn test_short_rename() {
    #[derive(Facet, Debug)]
    struct Args {
        #[facet(named, short, rename = "j")]
        concurrency: i64,
    }
    let args: Args = facet_args::from_slice(&["-j", "4"]).unwrap();
    assert_eq!(args.concurrency, 4);
}

#[test]
fn test_bool_str_before() {
    #[derive(Facet, Debug)]
    struct Args {
        #[facet(named)]
        foo: bool,
        #[facet(named)]
        hello: String,
    }
    let args: Args = facet_args::from_slice(&["--foo", "--hello", "world"]).unwrap();
    assert!(args.foo);
    assert_eq!(args.hello, "world".to_string());
}
