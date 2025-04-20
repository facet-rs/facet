//! Tests for scalar values.

use core::net::{IpAddr, Ipv4Addr, Ipv6Addr};

use facet::Facet;
use facet_toml::error::TomlErrorKind;

#[cfg(feature = "std")]
#[test]
fn test_string() {
    facet_testhelpers::setup();

    #[derive(Debug, Facet, PartialEq)]
    struct Root {
        value: String,
    }

    assert_eq!(
        facet_toml::from_str::<Root>("value = 'string'").expect("Failed to parse TOML"),
        Root {
            value: "string".to_string()
        },
    );
    assert_eq!(
        facet_toml::from_str::<Root>("value = 1").unwrap_err().kind,
        TomlErrorKind::ExpectedType {
            expected: "string",
            got: "integer"
        }
    );
}

#[cfg(feature = "std")]
#[test]
fn test_cow_string() {
    facet_testhelpers::setup();

    #[derive(Debug, Facet, PartialEq)]
    struct Root {
        value: std::borrow::Cow<'static, str>,
    }

    assert_eq!(
        facet_toml::from_str::<Root>("value = 'string'").expect("Failed to parse TOML"),
        Root {
            value: std::borrow::Cow::Borrowed("string")
        },
    );
    assert_eq!(
        facet_toml::from_str::<Root>("value = 1").unwrap_err().kind,
        TomlErrorKind::ExpectedType {
            expected: "string",
            got: "integer"
        }
    );
}

#[test]
fn test_bool() {
    facet_testhelpers::setup();

    #[derive(Debug, Facet, PartialEq)]
    struct Root {
        value: bool,
    }

    assert_eq!(
        facet_toml::from_str::<Root>("value = true").expect("Failed to parse TOML"),
        Root { value: true },
    );
    assert_eq!(
        facet_toml::from_str::<Root>("value = false").expect("Failed to parse TOML"),
        Root { value: false },
    );
    assert_eq!(
        facet_toml::from_str::<Root>("value = 1").unwrap_err().kind,
        TomlErrorKind::ExpectedType {
            expected: "boolean",
            got: "integer"
        }
    );
}

#[cfg(feature = "std")]
#[test]
fn test_socket_addr() {
    facet_testhelpers::setup();

    #[derive(Debug, Facet, PartialEq)]
    struct Root {
        value: std::net::SocketAddr,
    }

    assert_eq!(
        facet_toml::from_str::<Root>("value = '127.0.0.1:8000'").expect("Failed to parse TOML"),
        Root {
            value: "127.0.0.1:8000".parse().unwrap()
        },
    );
}

#[test]
fn test_ip_addr() {
    facet_testhelpers::setup();

    #[derive(Debug, Facet, PartialEq)]
    struct Root {
        value: IpAddr,
    }

    assert_eq!(
        facet_toml::from_str::<Root>("value = '127.0.0.1'").expect("Failed to parse TOML"),
        Root {
            value: "127.0.0.1".parse().unwrap()
        },
    );
    assert_eq!(
        facet_toml::from_str::<Root>("value = '::1'").expect("Failed to parse TOML"),
        Root {
            value: "::1".parse().unwrap()
        },
    );
    assert_eq!(
        dbg!(facet_toml::from_str::<Root>("value = '127.0.0.1:8000'").unwrap_err()).kind,
        TomlErrorKind::FailedTypeConversion {
            toml_type_name: "string",
            rust_type: core::net::IpAddr::SHAPE,
            reason: None
        }
    );
    assert_eq!(
        facet_toml::from_str::<Root>("value = true")
            .unwrap_err()
            .kind,
        TomlErrorKind::ExpectedType {
            expected: "string",
            got: "boolean"
        }
    );
}

#[test]
fn test_ipv4_addr() {
    facet_testhelpers::setup();

    #[derive(Debug, Facet, PartialEq)]
    struct Root {
        value: Ipv4Addr,
    }

    assert_eq!(
        facet_toml::from_str::<Root>("value = '127.0.0.1'").expect("Failed to parse TOML"),
        Root {
            value: "127.0.0.1".parse().unwrap()
        },
    );
}

#[test]
fn test_ipv6_addr() {
    facet_testhelpers::setup();

    #[derive(Debug, Facet, PartialEq)]
    struct Root {
        value: Ipv6Addr,
    }

    assert_eq!(
        facet_toml::from_str::<Root>("value = '::1'").expect("Failed to parse TOML"),
        Root {
            value: "::1".parse().unwrap()
        },
    );
}

#[test]
fn test_f64() {
    facet_testhelpers::setup();

    #[derive(Debug, Facet, PartialEq)]
    struct Root {
        value: f64,
    }

    assert_eq!(
        facet_toml::from_str::<Root>("value = 1").expect("Failed to parse TOML"),
        Root { value: 1.0 },
    );
    assert_eq!(
        facet_toml::from_str::<Root>("value = true")
            .unwrap_err()
            .kind,
        TomlErrorKind::ExpectedType {
            expected: "number",
            got: "boolean"
        }
    );
}

#[test]
fn test_f32() {
    facet_testhelpers::setup();

    #[derive(Debug, Facet, PartialEq)]
    struct Root {
        value: f32,
    }

    assert_eq!(
        facet_toml::from_str::<Root>("value = 1").expect("Failed to parse TOML"),
        Root { value: 1.0 },
    );
    assert_eq!(
        facet_toml::from_str::<Root>("value = true")
            .unwrap_err()
            .kind,
        TomlErrorKind::ExpectedType {
            expected: "number",
            got: "boolean"
        }
    );
}

#[test]
fn test_usize() {
    facet_testhelpers::setup();

    #[derive(Debug, Facet, PartialEq)]
    struct Root {
        value: usize,
    }

    assert_eq!(
        facet_toml::from_str::<Root>("value = 1").expect("Failed to parse TOML"),
        Root { value: 1 },
    );
    assert!(facet_toml::from_str::<Root>("value = -1").is_err());
    assert_eq!(
        facet_toml::from_str::<Root>("value = true")
            .unwrap_err()
            .kind,
        TomlErrorKind::ExpectedType {
            expected: "number",
            got: "boolean"
        }
    );
}

#[test]
fn test_u64() {
    facet_testhelpers::setup();

    #[derive(Debug, Facet, PartialEq)]
    struct Root {
        value: u64,
    }

    assert_eq!(
        facet_toml::from_str::<Root>("value = 1").expect("Failed to parse TOML"),
        Root { value: 1 },
    );
    assert!(facet_toml::from_str::<Root>("value = -1").is_err());
    assert_eq!(
        facet_toml::from_str::<Root>("value = true")
            .unwrap_err()
            .kind,
        TomlErrorKind::ExpectedType {
            expected: "number",
            got: "boolean"
        }
    );
}

#[test]
fn test_u32() {
    facet_testhelpers::setup();

    #[derive(Debug, Facet, PartialEq)]
    struct Root {
        value: u32,
    }

    assert_eq!(
        facet_toml::from_str::<Root>("value = 1").expect("Failed to parse TOML"),
        Root { value: 1 },
    );
    assert!(facet_toml::from_str::<Root>("value = -1").is_err());
    assert_eq!(
        facet_toml::from_str::<Root>("value = true")
            .unwrap_err()
            .kind,
        TomlErrorKind::ExpectedType {
            expected: "number",
            got: "boolean"
        }
    );
}

#[test]
fn test_u16() {
    facet_testhelpers::setup();

    #[derive(Debug, Facet, PartialEq)]
    struct Root {
        value: u16,
    }

    assert_eq!(
        facet_toml::from_str::<Root>("value = 1").expect("Failed to parse TOML"),
        Root { value: 1 },
    );
    assert!(facet_toml::from_str::<Root>("value = -1").is_err());
    assert_eq!(
        facet_toml::from_str::<Root>("value = true")
            .unwrap_err()
            .kind,
        TomlErrorKind::ExpectedType {
            expected: "number",
            got: "boolean"
        }
    );
}

#[test]
fn test_u8() {
    facet_testhelpers::setup();

    #[derive(Debug, Facet, PartialEq)]
    struct Root {
        value: u8,
    }

    assert_eq!(
        facet_toml::from_str::<Root>("value = 1").expect("Failed to parse TOML"),
        Root { value: 1 },
    );
    assert!(facet_toml::from_str::<Root>("value = -1").is_err());
    assert_eq!(
        facet_toml::from_str::<Root>("value = true")
            .unwrap_err()
            .kind,
        TomlErrorKind::ExpectedType {
            expected: "number",
            got: "boolean"
        }
    );
}

#[test]
fn test_isize() {
    facet_testhelpers::setup();

    #[derive(Debug, Facet, PartialEq)]
    struct Root {
        value: isize,
    }

    assert_eq!(
        facet_toml::from_str::<Root>("value = 1").expect("Failed to parse TOML"),
        Root { value: 1 },
    );
    assert_eq!(
        facet_toml::from_str::<Root>("value = true")
            .unwrap_err()
            .kind,
        TomlErrorKind::ExpectedType {
            expected: "number",
            got: "boolean"
        }
    );
}

#[test]
fn test_i64() {
    facet_testhelpers::setup();

    #[derive(Debug, Facet, PartialEq)]
    struct Root {
        value: i64,
    }

    assert_eq!(
        facet_toml::from_str::<Root>("value = 1").expect("Failed to parse TOML"),
        Root { value: 1 },
    );
    assert_eq!(
        facet_toml::from_str::<Root>("value = true")
            .unwrap_err()
            .kind,
        TomlErrorKind::ExpectedType {
            expected: "number",
            got: "boolean"
        }
    );
}

#[test]
fn test_i32() {
    facet_testhelpers::setup();

    #[derive(Debug, Facet, PartialEq)]
    struct Root {
        value: i32,
    }

    assert_eq!(
        facet_toml::from_str::<Root>("value = 1").expect("Failed to parse TOML"),
        Root { value: 1 },
    );
    assert_eq!(
        facet_toml::from_str::<Root>("value = true")
            .unwrap_err()
            .kind,
        TomlErrorKind::ExpectedType {
            expected: "number",
            got: "boolean"
        }
    );
}

#[test]
fn test_i16() {
    facet_testhelpers::setup();

    #[derive(Debug, Facet, PartialEq)]
    struct Root {
        value: i16,
    }

    assert_eq!(
        facet_toml::from_str::<Root>("value = 1").expect("Failed to parse TOML"),
        Root { value: 1 },
    );
    assert_eq!(
        facet_toml::from_str::<Root>("value = true")
            .unwrap_err()
            .kind,
        TomlErrorKind::ExpectedType {
            expected: "number",
            got: "boolean"
        }
    );
}

#[test]
fn test_i8() {
    facet_testhelpers::setup();

    #[derive(Debug, Facet, PartialEq)]
    struct Root {
        value: i8,
    }

    assert_eq!(
        facet_toml::from_str::<Root>("value = 1").expect("Failed to parse TOML"),
        Root { value: 1 },
    );
    assert_eq!(
        facet_toml::from_str::<Root>("value = true")
            .unwrap_err()
            .kind,
        TomlErrorKind::ExpectedType {
            expected: "number",
            got: "boolean"
        }
    );
}
