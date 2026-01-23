//! Tests for Ariadne pretty error reporting.
//!
//! Run with: cargo test -p facet-args --features ariadne

#![cfg(feature = "ariadne")]

use facet::Facet;
use facet_args as args;

/// Strip ANSI color codes for stable snapshot testing.
/// ANSI escape sequences follow the pattern: ESC [ <params> m
fn strip_ansi(s: &str) -> String {
    let mut result = String::with_capacity(s.len());
    let mut chars = s.chars().peekable();

    while let Some(c) = chars.next() {
        if c == '\x1B' {
            // Check for '[' (CSI sequence)
            if chars.peek() == Some(&'[') {
                chars.next(); // consume '['
                // Skip until we see 'm' (end of SGR sequence)
                while let Some(&next) = chars.peek() {
                    chars.next();
                    if next == 'm' {
                        break;
                    }
                }
            } else {
                result.push(c);
            }
        } else {
            result.push(c);
        }
    }

    result
}

#[test]
fn test_ariadne_unknown_flag_with_suggestion() {
    #[derive(Facet, Debug)]
    struct Args {
        #[facet(args::named, args::short = 'j')]
        concurrency: usize,
    }
    let result: Result<Args, _> = facet_args::from_slice(&["--c0ncurrency", "4"]);
    let err = result.unwrap_err();

    let ariadne_output = strip_ansi(&err.to_ariadne_string());
    insta::assert_snapshot!("ariadne_unknown_flag_with_suggestion", ariadne_output);
}

#[test]
fn test_ariadne_missing_argument() {
    #[derive(Facet, Debug)]
    struct Args {
        #[facet(args::named)]
        required_field: String,
    }
    let result: Result<Args, _> = facet_args::from_slice(&[]);
    let err = result.unwrap_err();

    let ariadne_output = strip_ansi(&err.to_ariadne_string());
    insta::assert_snapshot!("ariadne_missing_argument", ariadne_output);
}

#[test]
fn test_ariadne_invalid_value() {
    #[derive(Facet, Debug)]
    struct Args {
        #[facet(args::named)]
        count: usize,
    }
    let result: Result<Args, _> = facet_args::from_slice(&["--count", "not-a-number"]);
    let err = result.unwrap_err();

    let ariadne_output = strip_ansi(&err.to_ariadne_string());
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

    let result: Result<Args, _> = facet_args::from_slice(&["buidl"]);
    let err = result.unwrap_err();

    let ariadne_output = strip_ansi(&err.to_ariadne_string());
    insta::assert_snapshot!("ariadne_unknown_subcommand", ariadne_output);
}

#[test]
fn test_ariadne_unexpected_positional() {
    #[derive(Facet, Debug)]
    struct Args {
        #[facet(args::named)]
        name: String,
    }
    let result: Result<Args, _> = facet_args::from_slice(&["unexpected", "--name", "value"]);
    let err = result.unwrap_err();

    let ariadne_output = strip_ansi(&err.to_ariadne_string());
    insta::assert_snapshot!("ariadne_unexpected_positional", ariadne_output);
}

#[test]
fn test_ariadne_expected_value_got_eof() {
    #[derive(Facet, Debug)]
    struct Args {
        #[facet(args::named, args::short = 'j')]
        concurrency: usize,
    }
    let result: Result<Args, _> = facet_args::from_slice(&["--concurrency"]);
    let err = result.unwrap_err();

    let ariadne_output = strip_ansi(&err.to_ariadne_string());
    insta::assert_snapshot!("ariadne_expected_value_got_eof", ariadne_output);
}
