use crate::assert_diag_snapshot;
use facet::Facet;
use figue as args;

#[test]
fn test_error_non_struct_type_not_supported() {
    #[derive(Facet, Debug)]
    #[repr(u8)]
    #[allow(dead_code)]
    enum Args {
        Something,
        Else,
    }
    let err = figue::from_slice::<Args>(&["error", "wrong", "type"]).unwrap_err();
    assert_diag_snapshot!(err);
}

#[test]
fn test_error_missing_value_for_argument() {
    #[derive(Facet, Debug)]
    struct Args {
        #[facet(args::named, args::short = 'j')]
        concurrency: usize,
    }
    let err = figue::from_slice::<Args>(&["--concurrency"]).unwrap_err();
    assert_diag_snapshot!(err);
}

#[test]
fn test_error_wrong_type_for_argument() {
    #[derive(Facet, Debug)]
    struct Args {
        #[facet(args::named, args::short = 'j')]
        concurrency: usize,
    }
    let err = figue::from_slice::<Args>(&["--concurrency", "yes"]).unwrap_err();
    assert_diag_snapshot!(err);
}

#[test]
fn test_error_missing_value_for_argument_short_missed() {
    #[derive(Facet, Debug)]
    struct Args {
        #[facet(args::named, args::short = 'j')]
        concurrency: usize,
        #[facet(args::named, args::short = 'v')]
        verbose: bool,
    }
    let err = figue::from_slice::<Args>(&["-j", "-v"]).unwrap_err();
    assert_diag_snapshot!(err);
}

#[test]
fn test_error_missing_value_for_argument_short_eof() {
    #[derive(Facet, Debug)]
    struct Args {
        #[facet(args::named, args::short = 'j')]
        concurrency: usize,
    }
    let err = figue::from_slice::<Args>(&["-j"]).unwrap_err();
    assert_diag_snapshot!(err);
}

#[test]
fn test_error_unknown_argument() {
    #[derive(Facet, Debug)]
    struct Args {
        #[facet(args::named, args::short = 'j')]
        concurrency: usize,
    }
    let err = figue::from_slice::<Args>(&["--c0ncurrency"]).unwrap_err();
    assert_diag_snapshot!(err);
}

#[test]
fn test_error_number_outside_range() {
    #[derive(Facet, Debug)]
    struct Args {
        #[facet(args::named)]
        small: u8,
    }
    let err = figue::from_slice::<Args>(&["--small", "1000"]).unwrap_err();
    assert_diag_snapshot!(err);
}

#[test]
fn test_error_negative_value_for_unsigned() {
    #[derive(Facet, Debug)]
    struct Args {
        #[facet(args::named)]
        count: usize,
    }
    let err = figue::from_slice::<Args>(&["--count", "-10"]).unwrap_err();
    assert_diag_snapshot!(err);
}

#[test]
fn test_error_out_of_range() {
    #[derive(Facet, Debug)]
    struct Args {
        #[facet(args::named)]
        byte: u8,
    }
    let err = figue::from_slice::<Args>(&["--byte", "1000"]).unwrap_err();
    assert_diag_snapshot!(err);
}

#[test]
fn test_error_bool_with_invalid_value_positional() {
    #[derive(Facet, Debug)]
    struct Args {
        #[facet(args::named)]
        enable: bool,
    }
    let err = figue::from_slice::<Args>(&["--enable", "maybe"]).unwrap_err();
    assert_diag_snapshot!(err);
}

#[test]
fn test_error_char_with_multiple_chars() {
    #[derive(Facet, Debug)]
    struct Args {
        #[facet(args::named)]
        letter: char,
    }
    let err = figue::from_slice::<Args>(&["--letter", "abc"]).unwrap_err();
    assert_diag_snapshot!(err);
}

#[test]
fn test_error_option_with_multiple_values() {
    #[derive(Facet, Debug)]
    struct Args {
        #[facet(args::named)]
        maybe: Option<String>,
    }
    // Try to provide a list where an Option is expected
    let err = figue::from_slice::<Args>(&["--maybe", "value1", "value2"]).unwrap_err();
    assert_diag_snapshot!(err);
}

#[test]
fn test_error_unexpected_positional_arg() {
    #[derive(Facet, Debug)]
    struct Args {
        #[facet(args::named)]
        name: String,
    }
    // Provide a positional arg when none is expected
    let err = figue::from_slice::<Args>(&["unexpected", "--name", "value"]).unwrap_err();
    assert_diag_snapshot!(err);
}

#[test]
fn test_error_invalid_ip_address() {
    use std::net::IpAddr;

    #[derive(Facet, Debug)]
    struct Args {
        #[facet(args::named)]
        address: IpAddr,
    }
    // Provide an invalid IP address
    let err = figue::from_slice::<Args>(&["--address", "not-an-ip"]).unwrap_err();
    assert_diag_snapshot!(err);
}
