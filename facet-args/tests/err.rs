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
}

#[test]
fn test_error_wrong_type_for_argument() {
    #[derive(Facet, Debug)]
    struct Args {
        #[facet(named, short = 'j')]
        concurrency: usize,
    }
    let args: Result<Args, _> = facet_args::from_slice(&["--concurrency", "yes"]);
    let err = args.unwrap_err();
    insta::assert_snapshot!(err);
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
    eprintln!("{}", err);
    insta::assert_snapshot!(err);
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
}

#[test]
fn test_error_number_outside_range() {
    #[derive(Facet, Debug)]
    struct Args {
        #[facet(named)]
        small: u8,
    }
    let args: Result<Args, _> = facet_args::from_slice(&["--small", "1000"]);
    let err = args.unwrap_err();
    insta::assert_snapshot!(err);
}

#[test]
fn test_error_negative_value_for_unsigned() {
    #[derive(Facet, Debug)]
    struct Args {
        #[facet(named)]
        count: usize,
    }
    let args: Result<Args, _> = facet_args::from_slice(&["--count", "-10"]);
    let err = args.unwrap_err();
    insta::assert_snapshot!(err);
}

#[test]
fn test_error_out_of_range() {
    #[derive(Facet, Debug)]
    struct Args {
        #[facet(named)]
        byte: u8,
    }
    let args: Result<Args, _> = facet_args::from_slice(&["--byte", "1000"]);
    let err = args.unwrap_err();
    insta::assert_snapshot!(err);
}

#[test]
fn test_error_bool_with_invalid_value_positional() {
    #[derive(Facet, Debug)]
    struct Args {
        #[facet(named)]
        enable: bool,
    }
    let args: Result<Args, _> = facet_args::from_slice(&["--enable", "maybe"]);
    let err = args.unwrap_err();
    insta::assert_snapshot!(err);
}

#[test]
fn test_error_char_with_multiple_chars() {
    #[derive(Facet, Debug)]
    struct Args {
        #[facet(named)]
        letter: char,
    }
    let args: Result<Args, _> = facet_args::from_slice(&["--letter", "abc"]);
    let err = args.unwrap_err();
    insta::assert_snapshot!(err);
}

#[test]
fn test_error_nested_struct_with_scalar() {
    #[derive(Facet, Debug)]
    struct Config {
        port: u16,
    }

    #[derive(Facet, Debug)]
    struct Args {
        #[facet(named)]
        config: Config,
    }
    let args: Result<Args, _> = facet_args::from_slice(&["--config", "simple"]);
    let err = args.unwrap_err();
    insta::assert_snapshot!(err);
}

#[test]
fn test_error_option_with_multiple_values() {
    #[derive(Facet, Debug)]
    struct Args {
        #[facet(named)]
        maybe: Option<String>,
    }
    // Try to provide a list where an Option is expected
    let args: Result<Args, _> = facet_args::from_slice(&["--maybe", "value1", "value2"]);
    let err = args.unwrap_err();
    insta::assert_snapshot!(err);
}

#[test]
fn test_error_vec_with_incompatible_types() {
    #[derive(Facet, Debug)]
    struct Args {
        #[facet(named)]
        numbers: Vec<u32>,
    }
    // Mix of valid numbers and non-numbers
    let args: Result<Args, _> = facet_args::from_slice(&["--numbers", "1", "two", "3"]);
    let err = args.unwrap_err();
    insta::assert_snapshot!(err);
}

#[test]
fn test_error_tuple_struct_field_access() {
    #[derive(Facet, Debug)]
    struct Point(u32, u32);

    #[derive(Facet, Debug)]
    struct Args {
        #[facet(named)]
        point: Point,
    }
    // Try to access tuple struct fields by name
    let args: Result<Args, _> = facet_args::from_slice(&["--point.0", "10", "--point.1", "20"]);
    let err = args.unwrap_err();
    insta::assert_snapshot!(err);
}

#[test]
fn test_error_unexpected_positional_arg() {
    #[derive(Facet, Debug)]
    struct Args {
        #[facet(named)]
        name: String,
    }
    // Provide a positional arg when none is expected
    let args: Result<Args, _> = facet_args::from_slice(&["unexpected", "--name", "value"]);
    let err = args.unwrap_err();
    insta::assert_snapshot!(err);
}

#[test]
fn test_error_invalid_ip_address() {
    use std::net::IpAddr;

    #[derive(Facet, Debug)]
    struct Args {
        #[facet(named)]
        address: IpAddr,
    }
    // Provide an invalid IP address
    let args: Result<Args, _> = facet_args::from_slice(&["--address", "not-an-ip"]);
    let err = args.unwrap_err();
    insta::assert_snapshot!(err);
}

#[test]
fn test_error_complex_nested_structure() {
    #[derive(Facet, Debug)]
    struct ServerConfig {
        port: u16,
        host: String,
    }

    #[derive(Facet, Debug)]
    struct DatabaseConfig {
        url: String,
        pool_size: usize,
    }

    #[derive(Facet, Debug)]
    struct AppConfig {
        server: ServerConfig,
        database: DatabaseConfig,
    }

    #[derive(Facet, Debug)]
    struct Args {
        #[facet(named)]
        config: AppConfig,
    }
    // Try to flatten complex structure incorrectly
    let args: Result<Args, _> = facet_args::from_slice(&[
        "--config",
        "{server={port=8080,host=localhost},database={url=postgresql://,pool_size=10}}",
    ]);
    let err = args.unwrap_err();
    insta::assert_snapshot!(err);
}
