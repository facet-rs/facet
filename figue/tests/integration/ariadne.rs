//! Tests for Ariadne pretty error reporting.
//!
//! Run with: cargo test -p facet-args

use facet::Facet;
use figue as args;

#[test]
fn test_ariadne_unknown_flag_with_suggestion() {
    #[derive(Facet, Debug)]
    struct Args {
        #[facet(args::named, args::short = 'j')]
        concurrency: usize,
    }
    let err = figue::from_slice::<Args>(&["--c0ncurrency", "4"]).unwrap_err();

    let ariadne_output = strip_ansi_escapes::strip_str(err.to_string());
    insta::assert_snapshot!("ariadne_unknown_flag_with_suggestion", ariadne_output);
}

#[test]
fn test_ariadne_missing_argument() {
    #[derive(Facet, Debug)]
    struct Args {
        #[facet(args::named)]
        required_field: String,
    }
    let err = figue::from_slice::<Args>(&[]).unwrap_err();

    let ariadne_output = strip_ansi_escapes::strip_str(err.to_string());
    insta::assert_snapshot!("ariadne_missing_argument", ariadne_output);
}

#[test]
fn test_ariadne_invalid_value() {
    #[derive(Facet, Debug)]
    struct Args {
        #[facet(args::named)]
        count: usize,
    }
    let err = figue::from_slice::<Args>(&["--count", "not-a-number"]).unwrap_err();

    let ariadne_output = strip_ansi_escapes::strip_str(err.to_string());
    insta::assert_snapshot!("ariadne_invalid_value", ariadne_output);
}

#[test]
fn test_ariadne_unknown_subcommand() {
    #[derive(Facet, Debug)]
    #[repr(u8)]
    #[allow(dead_code)]
    enum Command {
        Build,
        Test,
        Run,
    }

    #[derive(Facet, Debug)]
    struct Args {
        #[facet(args::subcommand)]
        command: Command,
    }

    let err = figue::from_slice::<Args>(&["buidl"]).unwrap_err();

    let ariadne_output = strip_ansi_escapes::strip_str(err.to_string());
    insta::assert_snapshot!("ariadne_unknown_subcommand", ariadne_output);
}

#[test]
fn test_ariadne_unexpected_positional() {
    #[derive(Facet, Debug)]
    struct Args {
        #[facet(args::named)]
        name: String,
    }
    let err = figue::from_slice::<Args>(&["unexpected", "--name", "value"]).unwrap_err();

    let ariadne_output = strip_ansi_escapes::strip_str(err.to_string());
    insta::assert_snapshot!("ariadne_unexpected_positional", ariadne_output);
}

#[test]
fn test_ariadne_expected_value_got_eof() {
    #[derive(Facet, Debug)]
    struct Args {
        #[facet(args::named, args::short = 'j')]
        concurrency: usize,
    }
    let err = figue::from_slice::<Args>(&["--concurrency"]).unwrap_err();

    let ariadne_output = strip_ansi_escapes::strip_str(err.to_string());
    insta::assert_snapshot!("ariadne_expected_value_got_eof", ariadne_output);
}
