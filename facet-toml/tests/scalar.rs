//! Tests for scalar values.

use std::{
    net::{IpAddr, Ipv4Addr, Ipv6Addr, SocketAddr},
    num::NonZero,
};

use facet::Facet;

#[test]
fn test_string() {
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
}

// TODO: uncomment when lifetimes work properly
// #[test]
// fn test_cow_string() {
//     #[derive(Debug, Facet, PartialEq)]
//     struct Root {
//         value: Cow<'static, str>,
//     }

//     assert_eq!(
//         facet_toml::from_str::<Root>("value = 'string'").expect("Failed to parse TOML"),
//         Root {
//             value: Cow::Borrowed("string")
//         },
//     );
// }

#[test]
fn test_bool() {
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
}

#[test]
fn test_socket_addr() {
    #[derive(Debug, Facet, PartialEq)]
    struct Root {
        value: SocketAddr,
    }

    assert_eq!(
        facet_toml::from_str::<Root>("value = '127.0.0.1:8000'").expect("Failed to parse TOML"),
        Root {
            value: "127.0.0.1:8000".parse().unwrap()
        },
    );
    assert!(facet_toml::from_str::<Root>("value = 127.0.0.1").is_err());
}

#[test]
fn test_ip_addr() {
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
    assert!(facet_toml::from_str::<Root>("value = 127.0.0.1:8000").is_err());
}

#[test]
fn test_ipv4_addr() {
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
    assert!(facet_toml::from_str::<Root>("value = '::1'").is_err());
    assert!(facet_toml::from_str::<Root>("value = 127.0.0.1:8000").is_err());
}

#[test]
fn test_ipv6_addr() {
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
    assert!(facet_toml::from_str::<Root>("value = '127.0.0.1'").is_err());
    assert!(facet_toml::from_str::<Root>("value = ::1:8000").is_err());
}

#[test]
fn test_f64() {
    #[derive(Debug, Facet, PartialEq)]
    struct Root {
        value: f64,
    }

    assert_eq!(
        facet_toml::from_str::<Root>("value = 1").expect("Failed to parse TOML"),
        Root { value: 1.0 },
    );
}

#[test]
fn test_f32() {
    #[derive(Debug, Facet, PartialEq)]
    struct Root {
        value: f32,
    }

    assert_eq!(
        facet_toml::from_str::<Root>("value = 1").expect("Failed to parse TOML"),
        Root { value: 1.0 },
    );
}

#[test]
fn test_usize() {
    #[derive(Debug, Facet, PartialEq)]
    struct Root {
        value: usize,
    }

    assert_eq!(
        facet_toml::from_str::<Root>("value = 1").expect("Failed to parse TOML"),
        Root { value: 1 },
    );
    assert!(facet_toml::from_str::<Root>("value = -1").is_err());
}

#[test]
fn test_u128() {
    #[derive(Debug, Facet, PartialEq)]
    struct Root {
        value: u128,
    }

    assert_eq!(
        facet_toml::from_str::<Root>("value = 1").expect("Failed to parse TOML"),
        Root { value: 1 },
    );
    assert!(facet_toml::from_str::<Root>("value = -1").is_err());
}

#[test]
fn test_u64() {
    #[derive(Debug, Facet, PartialEq)]
    struct Root {
        value: u64,
    }

    assert_eq!(
        facet_toml::from_str::<Root>("value = 1").expect("Failed to parse TOML"),
        Root { value: 1 },
    );
    assert!(facet_toml::from_str::<Root>("value = -1").is_err());
}

#[test]
fn test_u32() {
    #[derive(Debug, Facet, PartialEq)]
    struct Root {
        value: u32,
    }

    assert_eq!(
        facet_toml::from_str::<Root>("value = 1").expect("Failed to parse TOML"),
        Root { value: 1 },
    );
    assert!(facet_toml::from_str::<Root>("value = -1").is_err());
}

#[test]
fn test_u16() {
    #[derive(Debug, Facet, PartialEq)]
    struct Root {
        value: u16,
    }

    assert_eq!(
        facet_toml::from_str::<Root>("value = 1").expect("Failed to parse TOML"),
        Root { value: 1 },
    );
    assert!(facet_toml::from_str::<Root>("value = -1").is_err());
}

#[test]
fn test_u8() {
    #[derive(Debug, Facet, PartialEq)]
    struct Root {
        value: u8,
    }

    assert_eq!(
        facet_toml::from_str::<Root>("value = 1").expect("Failed to parse TOML"),
        Root { value: 1 },
    );
    assert!(facet_toml::from_str::<Root>("value = -1").is_err());
}

#[test]
fn test_isize() {
    #[derive(Debug, Facet, PartialEq)]
    struct Root {
        value: isize,
    }

    assert_eq!(
        facet_toml::from_str::<Root>("value = 1").expect("Failed to parse TOML"),
        Root { value: 1 },
    );
}

#[test]
fn test_i128() {
    #[derive(Debug, Facet, PartialEq)]
    struct Root {
        value: i128,
    }

    assert_eq!(
        facet_toml::from_str::<Root>("value = 1").expect("Failed to parse TOML"),
        Root { value: 1 },
    );
}

#[test]
fn test_i64() {
    #[derive(Debug, Facet, PartialEq)]
    struct Root {
        value: i64,
    }

    assert_eq!(
        facet_toml::from_str::<Root>("value = 1").expect("Failed to parse TOML"),
        Root { value: 1 },
    );
}

#[test]
fn test_i32() {
    #[derive(Debug, Facet, PartialEq)]
    struct Root {
        value: i32,
    }

    assert_eq!(
        facet_toml::from_str::<Root>("value = 1").expect("Failed to parse TOML"),
        Root { value: 1 },
    );
}

#[test]
fn test_i16() {
    #[derive(Debug, Facet, PartialEq)]
    struct Root {
        value: i16,
    }

    assert_eq!(
        facet_toml::from_str::<Root>("value = 1").expect("Failed to parse TOML"),
        Root { value: 1 },
    );
}

#[test]
fn test_i8() {
    #[derive(Debug, Facet, PartialEq)]
    struct Root {
        value: i8,
    }

    assert_eq!(
        facet_toml::from_str::<Root>("value = 1").expect("Failed to parse TOML"),
        Root { value: 1 },
    );
}

#[test]
fn test_nonzero_usize() {
    #[derive(Debug, Facet, PartialEq)]
    struct Root {
        value: NonZero<usize>,
    }

    assert_eq!(
        facet_toml::from_str::<Root>("value = 1").expect("Failed to parse TOML"),
        Root {
            value: NonZero::new(1).unwrap()
        },
    );
    assert!(facet_toml::from_str::<Root>("value = -1").is_err());
}

#[test]
fn test_nonzero_u128() {
    #[derive(Debug, Facet, PartialEq)]
    struct Root {
        value: NonZero<u128>,
    }

    assert_eq!(
        facet_toml::from_str::<Root>("value = 1").expect("Failed to parse TOML"),
        Root {
            value: NonZero::new(1).unwrap()
        },
    );
    assert!(facet_toml::from_str::<Root>("value = -1").is_err());
}

#[test]
fn test_nonzero_u64() {
    #[derive(Debug, Facet, PartialEq)]
    struct Root {
        value: NonZero<u64>,
    }

    assert_eq!(
        facet_toml::from_str::<Root>("value = 1").expect("Failed to parse TOML"),
        Root {
            value: NonZero::new(1).unwrap()
        },
    );
    assert!(facet_toml::from_str::<Root>("value = -1").is_err());
}

#[test]
fn test_nonzero_u32() {
    #[derive(Debug, Facet, PartialEq)]
    struct Root {
        value: NonZero<u32>,
    }

    assert_eq!(
        facet_toml::from_str::<Root>("value = 1").expect("Failed to parse TOML"),
        Root {
            value: NonZero::new(1).unwrap()
        },
    );
    assert!(facet_toml::from_str::<Root>("value = -1").is_err());
}

#[test]
fn test_nonzero_u16() {
    #[derive(Debug, Facet, PartialEq)]
    struct Root {
        value: NonZero<u16>,
    }

    assert_eq!(
        facet_toml::from_str::<Root>("value = 1").expect("Failed to parse TOML"),
        Root {
            value: NonZero::new(1).unwrap()
        },
    );
    assert!(facet_toml::from_str::<Root>("value = -1").is_err());
}

#[test]
fn test_nonzero_u8() {
    #[derive(Debug, Facet, PartialEq)]
    struct Root {
        value: NonZero<u8>,
    }

    assert_eq!(
        facet_toml::from_str::<Root>("value = 1").expect("Failed to parse TOML"),
        Root {
            value: NonZero::new(1).unwrap()
        },
    );
    assert!(facet_toml::from_str::<Root>("value = -1").is_err());
}

#[test]
fn test_nonzero_isize() {
    #[derive(Debug, Facet, PartialEq)]
    struct Root {
        value: NonZero<isize>,
    }

    assert_eq!(
        facet_toml::from_str::<Root>("value = 1").expect("Failed to parse TOML"),
        Root {
            value: NonZero::new(1).unwrap()
        },
    );
}

#[test]
fn test_nonzero_i128() {
    #[derive(Debug, Facet, PartialEq)]
    struct Root {
        value: NonZero<i128>,
    }

    assert_eq!(
        facet_toml::from_str::<Root>("value = 1").expect("Failed to parse TOML"),
        Root {
            value: NonZero::new(1).unwrap()
        },
    );
}

#[test]
fn test_nonzero_i64() {
    #[derive(Debug, Facet, PartialEq)]
    struct Root {
        value: NonZero<i64>,
    }

    assert_eq!(
        facet_toml::from_str::<Root>("value = 1").expect("Failed to parse TOML"),
        Root {
            value: NonZero::new(1).unwrap()
        },
    );
}

#[test]
fn test_nonzero_i32() {
    #[derive(Debug, Facet, PartialEq)]
    struct Root {
        value: NonZero<i32>,
    }

    assert_eq!(
        facet_toml::from_str::<Root>("value = 1").expect("Failed to parse TOML"),
        Root {
            value: NonZero::new(1).unwrap()
        },
    );
}

#[test]
fn test_nonzero_i16() {
    #[derive(Debug, Facet, PartialEq)]
    struct Root {
        value: NonZero<i16>,
    }

    assert_eq!(
        facet_toml::from_str::<Root>("value = 1").expect("Failed to parse TOML"),
        Root {
            value: NonZero::new(1).unwrap()
        },
    );
}

#[test]
fn test_nonzero_i8() {
    #[derive(Debug, Facet, PartialEq)]
    struct Root {
        value: NonZero<i8>,
    }

    assert_eq!(
        facet_toml::from_str::<Root>("value = 1").expect("Failed to parse TOML"),
        Root {
            value: NonZero::new(1).unwrap()
        },
    );
}
