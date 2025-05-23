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
