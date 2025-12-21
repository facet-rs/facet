use facet::Facet;
use facet_args as args;

#[test]
fn test_arg_parse_easy() {
    #[derive(Facet)]
    struct Args {
        #[facet(args::positional)]
        path: String,

        #[facet(args::named, args::short = 'v')]
        verbose: bool,

        #[facet(args::named, args::short = 'j')]
        concurrency: usize,

        #[facet(args::named, args::short = 'x')]
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
        #[facet(args::named, args::short)]
        x: i64,

        #[facet(args::named, args::short)]
        y: u64,

        #[facet(args::named, args::short = 'z')]
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
        #[facet(args::named, args::short = 'v')]
        verbose: bool,
        #[facet(args::positional)]
        path: String,
    }
    let args: Args = facet_args::from_slice(&["absence_is_falsey.rs"]).unwrap();
    assert!(!args.verbose);
}

#[test]
fn test_missing_default() {
    #[derive(Facet, Debug)]
    struct Args {
        #[facet(args::positional, default = 42)]
        answer: usize,
        #[facet(args::named, args::short = 'p')]
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
        #[facet(args::named, args::short = 'p')]
        path: String,
        #[facet(args::named, args::short = 'j', default = default_concurrency())]
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
        #[facet(args::named)]
        rate: f64,
    }
    let args: Args = facet_args::from_slice(&["--rate", "infinity"]).unwrap();
    assert_eq!(args.rate, f64::INFINITY);
}

#[test]
fn test_short_rename() {
    #[derive(Facet, Debug)]
    struct Args {
        #[facet(args::named, args::short, rename = "j")]
        concurrency: i64,
    }
    let args: Args = facet_args::from_slice(&["-j", "4"]).unwrap();
    assert_eq!(args.concurrency, 4);
}

#[test]
fn test_bool_str_before() {
    #[derive(Facet, Debug)]
    struct Args {
        #[facet(args::named)]
        foo: bool,
        #[facet(args::named)]
        hello: String,
    }
    let args: Args = facet_args::from_slice(&["--foo", "--hello", "world"]).unwrap();
    assert!(args.foo);
    assert_eq!(args.hello, "world".to_string());
}

#[test]
fn test_option_string_positional() {
    // Repro case for Option<String> positional argument
    #[derive(Facet, Debug)]
    struct Args {
        #[facet(args::positional, default)]
        path: Option<String>,
    }

    // Test with a value
    let args: Args = facet_args::from_slice(&["."]).unwrap();
    assert_eq!(args.path, Some(".".to_string()));

    // Test without a value (should default to None)
    let args: Args = facet_args::from_slice(&[]).unwrap();
    assert_eq!(args.path, None);
}

/// Regression test for issue #1193: String field defaults should work with string literals
#[test]
fn test_string_default_value() {
    #[derive(Facet, Debug)]
    struct Args {
        #[facet(args::named)]
        name: String,
        // String literals work directly via Into<String>
        #[facet(args::named, default = "0.0.0-test")]
        version: String,
    }

    // Test that when version is not provided, it uses the default
    let args: Args = facet_args::from_slice(&["--name", "myapp"]).unwrap();
    assert_eq!(args.name, "myapp");
    assert_eq!(args.version, "0.0.0-test");

    // Test that when version is provided, it overrides the default
    let args: Args = facet_args::from_slice(&["--name", "myapp", "--version", "1.0.0"]).unwrap();
    assert_eq!(args.name, "myapp");
    assert_eq!(args.version, "1.0.0");
}

/// Test that default values work for IP addresses
#[test]
fn test_ip_address_default_value() {
    use std::net::{IpAddr, Ipv4Addr};

    #[derive(Facet, Debug)]
    struct ServerConfig {
        #[facet(args::named)]
        name: String,
        // IP address from string literal
        #[facet(args::named, default = "127.0.0.1")]
        bind_addr: IpAddr,
    }

    // Test that when bind_addr is not provided, it uses the default
    let config: ServerConfig = facet_args::from_slice(&["--name", "myserver"]).unwrap();
    assert_eq!(config.name, "myserver");
    assert_eq!(config.bind_addr, IpAddr::V4(Ipv4Addr::new(127, 0, 0, 1)));

    // Test that when bind_addr is provided, it overrides the default
    let config: ServerConfig =
        facet_args::from_slice(&["--name", "myserver", "--bind-addr", "0.0.0.0"]).unwrap();
    assert_eq!(config.bind_addr, IpAddr::V4(Ipv4Addr::new(0, 0, 0, 0)));
}

/// Test that default values work for PathBuf
#[test]
fn test_pathbuf_default_value() {
    use std::path::PathBuf;

    #[derive(Facet, Debug)]
    struct FileConfig {
        #[facet(args::named)]
        name: String,
        #[facet(args::named, default = "/tmp/default.log")]
        log_path: PathBuf,
    }

    // Test that when log_path is not provided, it uses the default
    let config: FileConfig = facet_args::from_slice(&["--name", "myapp"]).unwrap();
    assert_eq!(config.name, "myapp");
    assert_eq!(config.log_path, PathBuf::from("/tmp/default.log"));

    // Test that when log_path is provided, it overrides the default
    let config: FileConfig =
        facet_args::from_slice(&["--name", "myapp", "--log-path", "/var/log/app.log"]).unwrap();
    assert_eq!(config.name, "myapp");
    assert_eq!(config.log_path, PathBuf::from("/var/log/app.log"));
}

/// Regression test for issue #1348: Option<T> with args::named should be optional
#[test]
fn test_option_named_struct() {
    #[derive(Facet, Debug)]
    struct Args {
        #[facet(args::named)]
        name: String,
        #[facet(args::named)]
        filter: Option<String>,
    }

    // Test without the --filter flag (should succeed with None)
    let args: Args = facet_args::from_slice(&["--name", "test"]).unwrap();
    assert_eq!(args.name, "test");
    assert_eq!(args.filter, None);

    // Test with the --filter flag (should succeed with Some)
    let args: Args = facet_args::from_slice(&["--name", "test", "--filter", "active"]).unwrap();
    assert_eq!(args.name, "test");
    assert_eq!(args.filter, Some("active".to_string()));
}

/// Regression test for issue #1348: Option<String> with args::named in enum variant
#[test]
fn test_option_named_enum_variant() {
    #[derive(Facet, Debug)]
    #[repr(u8)]
    enum CliCommand {
        Explain {
            /// Explain only this specific node ID
            #[facet(args::named)]
            node: Option<String>,
        },
    }

    #[derive(Facet, Debug)]
    struct Args {
        #[facet(args::subcommand)]
        command: CliCommand,
    }

    // Test without the --node flag (should succeed with None)
    let args: Args = facet_args::from_slice(&["explain"]).unwrap();
    match args.command {
        CliCommand::Explain { node } => {
            assert_eq!(node, None);
        }
    }

    // Test with the --node flag (should succeed with Some)
    let args: Args = facet_args::from_slice(&["explain", "--node", "42"]).unwrap();
    match args.command {
        CliCommand::Explain { node } => {
            assert_eq!(node, Some("42".to_string()));
        }
    }
}
