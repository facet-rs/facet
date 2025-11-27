use facet::Facet;

mod common;

#[test]
fn test_simplest_value_singleton_list_named() {
    #[derive(Facet, Debug, PartialEq)]
    struct Args {
        #[facet(named, short = "s")]
        strings: Vec<String>,
    }

    // Test with multiple values (no delimiters)
    let args_single: Args =
        facet_args::from_slice(&["-s", "joe", "-s", "le", "-s", "rigolo"]).unwrap();

    assert_eq!(args_single.strings, vec!["joe", "le", "rigolo"]);
}

#[test]
fn test_simplest_value_singleton_list_positional() {
    #[derive(Facet, Debug, PartialEq)]
    struct Args {
        #[facet(positional)]
        strings: Vec<String>,
    }

    // Test with multiple values (no delimiters)
    let args_single: Args = facet_args::from_slice(&["joe", "le", "rigolo"]).unwrap();

    assert_eq!(args_single.strings, vec!["joe", "le", "rigolo"]);
}

#[test]
fn test_noargs_single_positional() {
    #[derive(Facet, Debug, PartialEq)]
    struct Args {
        #[facet(positional)]
        strings: String,
    }
    let err = facet_args::from_slice::<Args>(&[]).unwrap_err();
    assert_diag_snapshot!(err);
}

#[test]
fn test_noargs_vec_positional_default() {
    #[derive(Facet, Debug, PartialEq)]
    struct Args {
        #[facet(positional, default)]
        strings: Vec<String>,
    }
    let args = facet_args::from_slice::<Args>(&[]).unwrap();
    assert!(args.strings.is_empty());
}

#[test]
fn test_noargs_vec_positional_no_default() {
    #[derive(Facet, Debug, PartialEq)]
    struct Args {
        #[facet(positional)]
        strings: Vec<String>,
    }
    let err = facet_args::from_slice::<Args>(&[]).unwrap_err();
    assert_diag_snapshot!(err);
}

#[test]
fn test_doubledash_nothing() {
    #[derive(Facet, Debug, PartialEq)]
    struct Args {}

    let _args = facet_args::from_slice::<Args>(&["--"]).unwrap();
}

#[test]
fn test_doubledash_flags_before_dd() {
    #[derive(Facet, Debug, PartialEq)]
    struct Args {
        #[facet(named, default)]
        foo: bool,

        #[facet(named, default)]
        bar: bool,

        #[facet(positional, default)]
        args: Vec<String>,
    }

    let err = facet_args::from_slice::<Args>(&["--foo", "--bar", "--baz"]).unwrap_err();
    assert_diag_snapshot!(err);
}

#[test]
fn test_doubledash_flags_across_dd() {
    #[derive(Facet, Debug, PartialEq)]
    struct Args {
        #[facet(named, default)]
        foo: bool,

        #[facet(named, default)]
        bar: bool,

        #[facet(positional, default)]
        args: Vec<String>,
    }

    let args = facet_args::from_slice::<Args>(&["--foo", "--bar", "--", "--baz"]).unwrap();
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
        #[facet(named, default)]
        foo: bool,

        #[facet(named, default)]
        bar: bool,

        #[facet(positional, default)]
        args: Vec<String>,
    }

    let args = facet_args::from_slice::<Args>(&["--", "--foo", "--bar", "--baz"]).unwrap();
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
