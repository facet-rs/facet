//! Tests for scalar values.

use core::net::{IpAddr, Ipv4Addr, Ipv6Addr};

use eyre::Result;
use facet::Facet;
use facet_toml::error::TomlErrorKind;

#[cfg(feature = "std")]
#[test]
fn test_string() -> Result<()> {
    facet_testhelpers::setup();

    #[derive(Debug, Facet, PartialEq)]
    struct Root {
        value: String,
    }

    assert_eq!(
        facet_toml::from_str::<Root>("value = 'string'")?,
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

    Ok(())
}

#[cfg(feature = "std")]
#[test]
fn test_cow_string() -> Result<()> {
    facet_testhelpers::setup();

    #[derive(Debug, Facet, PartialEq)]
    struct Root {
        value: std::borrow::Cow<'static, str>,
    }

    assert_eq!(
        facet_toml::from_str::<Root>("value = 'string'")?,
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

    Ok(())
}

#[test]
fn test_bool() -> Result<()> {
    facet_testhelpers::setup();

    #[derive(Debug, Facet, PartialEq)]
    struct Root {
        value: bool,
    }

    assert_eq!(
        facet_toml::from_str::<Root>("value = true")?,
        Root { value: true },
    );
    assert_eq!(
        facet_toml::from_str::<Root>("value = false")?,
        Root { value: false },
    );
    assert_eq!(
        facet_toml::from_str::<Root>("value = 1").unwrap_err().kind,
        TomlErrorKind::ExpectedType {
            expected: "boolean",
            got: "integer"
        }
    );
    assert_eq!(
        facet_toml::from_str::<Root>("value = {a = 1}")
            .unwrap_err()
            .kind,
        TomlErrorKind::ExpectedType {
            expected: "boolean",
            got: "inline table"
        }
    );
    assert_eq!(
        facet_toml::from_str::<Root>("[value]").unwrap_err().kind,
        TomlErrorKind::ExpectedType {
            expected: "value",
            got: "table"
        }
    );

    Ok(())
}

#[cfg(feature = "std")]
#[test]
fn test_socket_addr() -> Result<()> {
    facet_testhelpers::setup();

    #[derive(Debug, Facet, PartialEq)]
    struct Root {
        value: std::net::SocketAddr,
    }

    assert_eq!(
        facet_toml::from_str::<Root>("value = '127.0.0.1:8000'")?,
        Root {
            value: "127.0.0.1:8000".parse().unwrap()
        },
    );

    Ok(())
}

#[test]
fn test_ip_addr() -> Result<()> {
    facet_testhelpers::setup();

    #[derive(Debug, Facet, PartialEq)]
    struct Root {
        value: IpAddr,
    }

    assert_eq!(
        facet_toml::from_str::<Root>("value = '127.0.0.1'")?,
        Root {
            value: "127.0.0.1".parse().unwrap()
        },
    );
    assert_eq!(
        facet_toml::from_str::<Root>("value = '::1'")?,
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

    Ok(())
}

#[test]
fn test_ipv4_addr() -> Result<()> {
    facet_testhelpers::setup();

    #[derive(Debug, Facet, PartialEq)]
    struct Root {
        value: Ipv4Addr,
    }

    assert_eq!(
        facet_toml::from_str::<Root>("value = '127.0.0.1'")?,
        Root {
            value: "127.0.0.1".parse().unwrap()
        },
    );

    Ok(())
}

#[test]
fn test_ipv6_addr() -> Result<()> {
    facet_testhelpers::setup();

    #[derive(Debug, Facet, PartialEq)]
    struct Root {
        value: Ipv6Addr,
    }

    assert_eq!(
        facet_toml::from_str::<Root>("value = '::1'")?,
        Root {
            value: "::1".parse().unwrap()
        },
    );

    Ok(())
}

#[test]
fn test_f64() -> Result<()> {
    facet_testhelpers::setup();

    #[derive(Debug, Facet, PartialEq)]
    struct Root {
        value: f64,
    }

    assert_eq!(
        facet_toml::from_str::<Root>("value = 1")?,
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

    Ok(())
}

#[test]
fn test_f32() -> Result<()> {
    facet_testhelpers::setup();

    #[derive(Debug, Facet, PartialEq)]
    struct Root {
        value: f32,
    }

    assert_eq!(
        facet_toml::from_str::<Root>("value = 1")?,
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

    Ok(())
}

#[test]
fn test_usize() -> Result<()> {
    facet_testhelpers::setup();

    #[derive(Debug, Facet, PartialEq)]
    struct Root {
        value: usize,
    }

    assert_eq!(
        facet_toml::from_str::<Root>("value = 1")?,
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

    Ok(())
}

#[test]
fn test_u64() -> Result<()> {
    facet_testhelpers::setup();

    #[derive(Debug, Facet, PartialEq)]
    struct Root {
        value: u64,
    }

    assert_eq!(
        facet_toml::from_str::<Root>("value = 1")?,
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

    Ok(())
}

#[test]
fn test_u32() -> Result<()> {
    facet_testhelpers::setup();

    #[derive(Debug, Facet, PartialEq)]
    struct Root {
        value: u32,
    }

    assert_eq!(
        facet_toml::from_str::<Root>("value = 1")?,
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

    Ok(())
}

#[test]
fn test_u16() -> Result<()> {
    facet_testhelpers::setup();

    #[derive(Debug, Facet, PartialEq)]
    struct Root {
        value: u16,
    }

    assert_eq!(
        facet_toml::from_str::<Root>("value = 1")?,
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

    Ok(())
}

#[test]
fn test_u8() -> Result<()> {
    facet_testhelpers::setup();

    #[derive(Debug, Facet, PartialEq)]
    struct Root {
        value: u8,
    }

    assert_eq!(
        facet_toml::from_str::<Root>("value = 1")?,
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

    Ok(())
}

#[test]
fn test_isize() -> Result<()> {
    facet_testhelpers::setup();

    #[derive(Debug, Facet, PartialEq)]
    struct Root {
        value: isize,
    }

    assert_eq!(
        facet_toml::from_str::<Root>("value = 1")?,
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

    Ok(())
}

#[test]
fn test_i64() -> Result<()> {
    facet_testhelpers::setup();

    #[derive(Debug, Facet, PartialEq)]
    struct Root {
        value: i64,
    }

    assert_eq!(
        facet_toml::from_str::<Root>("value = 1")?,
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

    Ok(())
}

#[test]
fn test_i32() -> Result<()> {
    facet_testhelpers::setup();

    #[derive(Debug, Facet, PartialEq)]
    struct Root {
        value: i32,
    }

    assert_eq!(
        facet_toml::from_str::<Root>("value = 1")?,
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

    Ok(())
}

#[test]
fn test_i16() -> Result<()> {
    facet_testhelpers::setup();

    #[derive(Debug, Facet, PartialEq)]
    struct Root {
        value: i16,
    }

    assert_eq!(
        facet_toml::from_str::<Root>("value = 1")?,
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

    Ok(())
}

#[test]
fn test_i8() -> Result<()> {
    facet_testhelpers::setup();

    #[derive(Debug, Facet, PartialEq)]
    struct Root {
        value: i8,
    }

    assert_eq!(
        facet_toml::from_str::<Root>("value = 1")?,
        Root { value: 1 },
    );
    assert_eq!(
        facet_toml::from_str::<Root>("value = 300.0")
            .unwrap_err()
            .kind,
        TomlErrorKind::FailedTypeConversion {
            toml_type_name: "float",
            rust_type: i8::SHAPE,
            reason: None
        }
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

    Ok(())
}

#[test]
fn test_unparsable_scalar() {
    facet_testhelpers::setup();

    assert_eq!(
        facet_toml::from_str::<()>("value = 1").unwrap_err().kind,
        TomlErrorKind::UnrecognizedScalar(<()>::SHAPE)
    );
}
