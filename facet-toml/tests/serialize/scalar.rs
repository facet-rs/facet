//! Tests for scalar values.

use alloc::borrow::Cow;
use core::net::{IpAddr, Ipv4Addr, Ipv6Addr};
use facet_toml::TomlSerError;

use facet::Facet;
use facet_testhelpers::test;

use crate::assert_serialize;

#[test]
fn test_string() {
    #[derive(Debug, Facet, PartialEq)]
    struct Root {
        value: String,
    }

    assert_serialize!(
        Root,
        Root {
            value: "string".to_string()
        }
    );
}

#[test]
#[ignore = "Must be fixed in facet-serialize"]
fn test_cow_string() {
    #[derive(Debug, Facet, PartialEq)]
    struct Root {
        value: Cow<'static, str>,
    }

    assert_serialize!(
        Root,
        Root {
            value: Cow::Borrowed("string")
        }
    );
}

#[test]
fn test_bool() {
    #[derive(Debug, Facet, PartialEq)]
    struct Root {
        value: bool,
    }

    assert_serialize!(Root, Root { value: true });
    assert_serialize!(Root, Root { value: false });
}

#[test]
#[ignore = "Must be fixed in facet-serialize"]
fn test_socket_addr() {
    #[derive(Debug, Facet, PartialEq)]
    struct Root {
        value: std::net::SocketAddr,
    }

    assert_serialize!(
        Root,
        Root {
            value: "127.0.0.1:8000".parse()?
        }
    );
}

#[test]
#[ignore = "Must be fixed in facet-serialize"]
fn test_ip_addr() {
    #[derive(Debug, Facet, PartialEq)]
    struct Root {
        value: IpAddr,
    }

    assert_serialize!(
        Root,
        Root {
            value: "127.0.0.1".parse()?
        },
    );
    assert_serialize!(
        Root,
        Root {
            value: "::1".parse()?
        },
    );
}

#[test]
#[ignore = "Must be fixed in facet-serialize"]
fn test_ipv4_addr() {
    #[derive(Debug, Facet, PartialEq)]
    struct Root {
        value: Ipv4Addr,
    }

    assert_serialize!(
        Root,
        Root {
            value: "127.0.0.1".parse()?
        },
    );
}

#[test]
#[ignore = "Must be fixed in facet-serialize"]
fn test_ipv6_addr() {
    #[derive(Debug, Facet, PartialEq)]
    struct Root {
        value: Ipv6Addr,
    }

    assert_serialize!(
        Root,
        Root {
            value: "::1".parse()?
        },
    );
}

#[test]
fn test_f64() {
    #[derive(Debug, Facet, PartialEq)]
    struct Root {
        value: f64,
    }

    assert_serialize!(Root, Root { value: 1.0 },);
}

#[test]
fn test_f32() {
    #[derive(Debug, Facet, PartialEq)]
    struct Root {
        value: f32,
    }

    assert_serialize!(Root, Root { value: 1.0 },);
}

#[test]
fn test_usize() {
    #[derive(Debug, Facet, PartialEq)]
    struct Root {
        value: usize,
    }

    assert_serialize!(Root, Root { value: 1 },);
}

#[test]
fn test_u64() {
    #[derive(Debug, Facet, PartialEq)]
    struct Root {
        value: u64,
    }

    assert_serialize!(Root, Root { value: 1 },);
}

#[test]
fn test_u32() {
    #[derive(Debug, Facet, PartialEq)]
    struct Root {
        value: u32,
    }

    assert_serialize!(Root, Root { value: 1 },);
}

#[test]
fn test_u16() {
    #[derive(Debug, Facet, PartialEq)]
    struct Root {
        value: u16,
    }

    assert_serialize!(Root, Root { value: 1 },);
}

#[test]
fn test_u8() {
    #[derive(Debug, Facet, PartialEq)]
    struct Root {
        value: u8,
    }

    assert_serialize!(Root, Root { value: 1 },);
}

#[test]
fn test_isize() {
    #[derive(Debug, Facet, PartialEq)]
    struct Root {
        value: isize,
    }

    assert_serialize!(Root, Root { value: 1 },);
}

#[test]
fn test_i64() {
    #[derive(Debug, Facet, PartialEq)]
    struct Root {
        value: i64,
    }

    assert_serialize!(Root, Root { value: 1 },);
}

#[test]
fn test_i32() {
    #[derive(Debug, Facet, PartialEq)]
    struct Root {
        value: i32,
    }

    assert_serialize!(Root, Root { value: 1 },);
}

#[test]
fn test_i16() {
    #[derive(Debug, Facet, PartialEq)]
    struct Root {
        value: i16,
    }

    assert_serialize!(Root, Root { value: 1 },);
}

#[test]
fn test_i8() {
    #[derive(Debug, Facet, PartialEq)]
    struct Root {
        value: i8,
    }

    assert_serialize!(Root, Root { value: 1 },);
}

#[test]
fn test_optional_scalar() {
    #[derive(Debug, Facet, PartialEq)]
    struct Root {
        value: Option<i8>,
    }

    assert_serialize!(Root, Root { value: None });
    assert_serialize!(Root, Root { value: Some(1) });
}

#[test]
fn test_nested_optional_scalar() {
    #[derive(Debug, Facet, PartialEq)]
    struct Root {
        value: Option<Option<Option<i8>>>,
    }

    assert_serialize!(
        Root,
        Root {
            value: Some(Some(Some(1)))
        }
    );
    assert_serialize!(Root, Root { value: None });
}

#[test]
fn test_unit() {
    #[derive(Debug, Facet, PartialEq)]
    struct Root {
        unit: (),
    }

    assert_serialize!(Root, Root { unit: () });
}

#[test]
fn test_unit_option() {
    #[derive(Debug, Facet, PartialEq)]
    struct Root {
        unit: Option<()>,
    }

    assert_serialize!(Root, Root { unit: None });
}

#[test]
fn test_u64_out_of_range() {
    #[derive(Debug, Facet, PartialEq)]
    struct Root {
        value: u64,
    }

    assert!(matches!(
        facet_toml::to_string(&Root { value: u64::MAX }).unwrap_err(),
        TomlSerError::InvalidNumberToI64Conversion { .. }
    ));
}

#[test]
fn test_u128_out_of_range() {
    #[derive(Debug, Facet, PartialEq)]
    struct Root {
        value: u128,
    }

    assert!(matches!(
        facet_toml::to_string(&Root { value: u128::MAX }).unwrap_err(),
        TomlSerError::InvalidNumberToI64Conversion { .. }
    ));
}

#[test]
fn test_i128_out_of_range() {
    #[derive(Debug, Facet, PartialEq)]
    struct Root {
        value: i128,
    }

    assert!(matches!(
        facet_toml::to_string(&Root { value: i128::MAX }).unwrap_err(),
        TomlSerError::InvalidNumberToI64Conversion { .. }
    ));
}
