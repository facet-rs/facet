use bytes::Bytes;
use bytes::BytesMut;
use facet::Facet;
use facet_json::from_str;
use facet_json::to_string;
use facet_testhelpers::test;

#[test]
fn json_read_bytes() {
    #[derive(Facet, Debug, PartialEq)]
    struct FooBar {
        data: Bytes,
    }

    let json = r#"{"data":[1, 2, 3, 4, 255]}"#;

    let s: FooBar = from_str(json)?;
    assert_eq!(
        s,
        FooBar {
            data: Bytes::from_iter([1, 2, 3, 4, 255]),
        }
    );
}

#[test]
fn json_read_bytes_mut() {
    #[derive(Facet, Debug, PartialEq)]
    struct FooBar {
        data: BytesMut,
    }

    let json = r#"{"data":[1, 2, 3, 4, 255]}"#;

    let s: FooBar = from_str(json)?;
    assert_eq!(
        s,
        FooBar {
            data: BytesMut::from_iter([1, 2, 3, 4, 255]),
        }
    );
}

#[test]
fn json_write_bytes() {
    #[derive(Facet, Debug, PartialEq)]
    struct FooBar {
        data: Bytes,
    }

    let value = FooBar {
        data: Bytes::from_iter([1, 2, 3, 4, 255]),
    };

    let json = to_string(&value);
    assert_eq!(json, r#"{"data":[1,2,3,4,255]}"#);
}

#[test]
fn json_write_bytes_mut() {
    #[derive(Facet, Debug, PartialEq)]
    struct FooBar {
        data: BytesMut,
    }

    let value = FooBar {
        data: BytesMut::from_iter([1, 2, 3, 4, 255]),
    };

    let json = to_string(&value);
    assert_eq!(json, r#"{"data":[1,2,3,4,255]}"#);
}

#[test]
fn json_write_vec_u8() {
    #[derive(Facet, Debug, PartialEq)]
    struct FooBar {
        data: Vec<u8>,
    }

    let value = FooBar {
        data: vec![0, 128, 255, 42],
    };

    let json = to_string(&value);
    assert_eq!(json, r#"{"data":[0,128,255,42]}"#);
}

#[test]
fn json_read_vec_u8() {
    #[derive(Facet, Debug, PartialEq)]
    struct FooBar {
        data: Vec<u8>,
    }

    let json = r#"{"data":[0, 128, 255, 42]}"#;

    let s: FooBar = from_str(json)?;
    assert_eq!(
        s,
        FooBar {
            data: vec![0, 128, 255, 42],
        }
    );
}

#[test]
fn json_write_slice_u8() {
    let bytes: [u8; 5] = [10, 20, 30, 40, 250];
    let json = to_string(&bytes);
    assert_eq!(json, r#"[10,20,30,40,250]"#);
}

#[test]
fn json_write_slice_u8_in_struct() {
    #[derive(Facet, Debug, PartialEq)]
    struct Container<'a> {
        data: &'a [u8],
    }

    let bytes: [u8; 5] = [10, 20, 30, 40, 250];
    let container = Container { data: &bytes };
    let json = to_string(&container);
    assert_eq!(json, r#"{"data":[10,20,30,40,250]}"#);
}

#[test]
fn json_roundtrip_empty_bytes() {
    #[derive(Facet, Debug, PartialEq)]
    struct FooBar {
        data: Vec<u8>,
    }

    let value = FooBar { data: vec![] };
    let json = to_string(&value);
    assert_eq!(json, r#"{"data":[]}"#);

    let parsed: FooBar = from_str(&json).unwrap();
    assert_eq!(parsed, value);
}

#[test]
fn json_roundtrip_single_byte() {
    #[derive(Facet, Debug, PartialEq)]
    struct FooBar {
        data: Vec<u8>,
    }

    let value = FooBar { data: vec![123] };
    let json = to_string(&value);
    assert_eq!(json, r#"{"data":[123]}"#);

    let parsed: FooBar = from_str(&json).unwrap();
    assert_eq!(parsed, value);
}

#[test]
fn json_roundtrip_bytes_full_range() {
    #[derive(Facet, Debug, PartialEq)]
    struct FooBar {
        data: Vec<u8>,
    }

    // Test all possible byte values
    let value = FooBar {
        data: (0..=255).collect(),
    };
    let json = to_string(&value);

    let parsed: FooBar = from_str(&json).unwrap();
    assert_eq!(parsed, value);
}
