//! Tests for scalar values.

use alloc::borrow::Cow;
use core::net::{IpAddr, Ipv4Addr, Ipv6Addr};

use eyre::Result;
use facet::Facet;

use crate::assert_serialize;

#[test]
fn test_string() -> Result<()> {
    facet_testhelpers::setup();

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

    Ok(())
}

#[test]
#[ignore = "Must be fixed in facet-serialize"]
fn test_cow_string() -> Result<()> {
    facet_testhelpers::setup();

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

    Ok(())
}

#[test]
fn test_bool() -> Result<()> {
    facet_testhelpers::setup();

    #[derive(Debug, Facet, PartialEq)]
    struct Root {
        value: bool,
    }

    assert_serialize!(Root, Root { value: true });
    assert_serialize!(Root, Root { value: false });

    Ok(())
}

#[test]
#[ignore = "Must be fixed in facet-serialize"]
fn test_socket_addr() -> Result<()> {
    facet_testhelpers::setup();

    #[derive(Debug, Facet, PartialEq)]
    struct Root {
        value: std::net::SocketAddr,
    }

    assert_serialize!(
        Root,
        Root {
            value: "127.0.0.1:8000".parse().unwrap()
        }
    );

    Ok(())
}

#[test]
#[ignore = "Must be fixed in facet-serialize"]
fn test_ip_addr() -> Result<()> {
    facet_testhelpers::setup();

    #[derive(Debug, Facet, PartialEq)]
    struct Root {
        value: IpAddr,
    }

    assert_serialize!(
        Root,
        Root {
            value: "127.0.0.1".parse().unwrap()
        },
    );
    assert_serialize!(
        Root,
        Root {
            value: "::1".parse().unwrap()
        },
    );

    Ok(())
}

#[test]
#[ignore = "Must be fixed in facet-serialize"]
fn test_ipv4_addr() -> Result<()> {
    facet_testhelpers::setup();

    #[derive(Debug, Facet, PartialEq)]
    struct Root {
        value: Ipv4Addr,
    }

    assert_serialize!(
        Root,
        Root {
            value: "127.0.0.1".parse().unwrap()
        },
    );

    Ok(())
}

#[test]
#[ignore = "Must be fixed in facet-serialize"]
fn test_ipv6_addr() -> Result<()> {
    facet_testhelpers::setup();

    #[derive(Debug, Facet, PartialEq)]
    struct Root {
        value: Ipv6Addr,
    }

    assert_serialize!(
        Root,
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

    assert_serialize!(Root, Root { value: 1.0 },);

    Ok(())
}

#[test]
fn test_f32() -> Result<()> {
    facet_testhelpers::setup();

    #[derive(Debug, Facet, PartialEq)]
    struct Root {
        value: f32,
    }

    assert_serialize!(Root, Root { value: 1.0 },);

    Ok(())
}

#[test]
fn test_usize() -> Result<()> {
    facet_testhelpers::setup();

    #[derive(Debug, Facet, PartialEq)]
    struct Root {
        value: usize,
    }

    assert_serialize!(Root, Root { value: 1 },);

    Ok(())
}

#[test]
fn test_u64() -> Result<()> {
    facet_testhelpers::setup();

    #[derive(Debug, Facet, PartialEq)]
    struct Root {
        value: u64,
    }

    assert_serialize!(Root, Root { value: 1 },);

    Ok(())
}

#[test]
fn test_u32() -> Result<()> {
    facet_testhelpers::setup();

    #[derive(Debug, Facet, PartialEq)]
    struct Root {
        value: u32,
    }

    assert_serialize!(Root, Root { value: 1 },);

    Ok(())
}

#[test]
fn test_u16() -> Result<()> {
    facet_testhelpers::setup();

    #[derive(Debug, Facet, PartialEq)]
    struct Root {
        value: u16,
    }

    assert_serialize!(Root, Root { value: 1 },);

    Ok(())
}

#[test]
fn test_u8() -> Result<()> {
    facet_testhelpers::setup();

    #[derive(Debug, Facet, PartialEq)]
    struct Root {
        value: u8,
    }

    assert_serialize!(Root, Root { value: 1 },);

    Ok(())
}

#[test]
fn test_isize() -> Result<()> {
    facet_testhelpers::setup();

    #[derive(Debug, Facet, PartialEq)]
    struct Root {
        value: isize,
    }

    assert_serialize!(Root, Root { value: 1 },);

    Ok(())
}

#[test]
fn test_i64() -> Result<()> {
    facet_testhelpers::setup();

    #[derive(Debug, Facet, PartialEq)]
    struct Root {
        value: i64,
    }

    assert_serialize!(Root, Root { value: 1 },);

    Ok(())
}

#[test]
fn test_i32() -> Result<()> {
    facet_testhelpers::setup();

    #[derive(Debug, Facet, PartialEq)]
    struct Root {
        value: i32,
    }

    assert_serialize!(Root, Root { value: 1 },);

    Ok(())
}

#[test]
fn test_i16() -> Result<()> {
    facet_testhelpers::setup();

    #[derive(Debug, Facet, PartialEq)]
    struct Root {
        value: i16,
    }

    assert_serialize!(Root, Root { value: 1 },);

    Ok(())
}

#[test]
fn test_i8() -> Result<()> {
    facet_testhelpers::setup();

    #[derive(Debug, Facet, PartialEq)]
    struct Root {
        value: i8,
    }

    assert_serialize!(Root, Root { value: 1 },);

    Ok(())
}

#[test]
fn test_optional_scalar() -> Result<()> {
    facet_testhelpers::setup();

    #[derive(Debug, Facet, PartialEq)]
    struct Root {
        value: Option<i8>,
    }

    assert_serialize!(Root, Root { value: None });
    assert_serialize!(Root, Root { value: Some(1) });

    Ok(())
}

#[test]
fn test_nested_optional_scalar() -> Result<()> {
    facet_testhelpers::setup();

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

    Ok(())
}
