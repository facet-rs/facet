//! Basic tests for facet-xdr

use facet::Facet;
use facet_xdr::{from_slice, to_vec};

#[derive(Facet, Debug, PartialEq)]
struct Point {
    x: i32,
    y: i32,
}

#[test]
fn test_roundtrip_point() {
    let point = Point { x: 10, y: 20 };
    let bytes = to_vec(&point).unwrap();

    // XDR: two i32s in big-endian = 8 bytes
    assert_eq!(bytes.len(), 8);
    assert_eq!(&bytes[0..4], &[0, 0, 0, 10]); // x = 10
    assert_eq!(&bytes[4..8], &[0, 0, 0, 20]); // y = 20

    let decoded: Point = from_slice(&bytes).unwrap();
    assert_eq!(decoded, point);
}

#[derive(Facet, Debug, PartialEq)]
struct SimpleStruct {
    a: u32,
    b: u32,
    c: u32,
}

#[test]
fn test_roundtrip_simple_struct() {
    let s = SimpleStruct { a: 1, b: 2, c: 3 };
    let bytes = to_vec(&s).unwrap();
    let decoded: SimpleStruct = from_slice(&bytes).unwrap();
    assert_eq!(decoded, s);
}

#[test]
fn test_u32() {
    let val: u32 = 0x12345678;
    let bytes = to_vec(&val).unwrap();
    assert_eq!(bytes, &[0x12, 0x34, 0x56, 0x78]);
    let decoded: u32 = from_slice(&bytes).unwrap();
    assert_eq!(decoded, val);
}

#[test]
fn test_u32_xdr_codec_compat() {
    use xdr_codec::Pack;

    let val: u32 = 0x12345678;

    // Our encoding
    let our_bytes = to_vec(&val).unwrap();

    // xdr-codec encoding
    let mut xdr_bytes = Vec::new();
    val.pack(&mut xdr_bytes).unwrap();

    assert_eq!(our_bytes, xdr_bytes, "u32 encoding should match xdr-codec");
}

#[test]
fn test_i32_positive() {
    let val: i32 = 42;
    let bytes = to_vec(&val).unwrap();
    assert_eq!(bytes, &[0, 0, 0, 42]);
    let decoded: i32 = from_slice(&bytes).unwrap();
    assert_eq!(decoded, val);
}

#[test]
fn test_i32_negative() {
    let val: i32 = -1;
    let bytes = to_vec(&val).unwrap();
    assert_eq!(bytes, &[0xFF, 0xFF, 0xFF, 0xFF]);
    let decoded: i32 = from_slice(&bytes).unwrap();
    assert_eq!(decoded, val);
}

#[test]
fn test_i32_xdr_codec_compat() {
    use xdr_codec::Pack;

    for val in [-1i32, 0, 1, i32::MIN, i32::MAX, 42, -42] {
        let our_bytes = to_vec(&val).unwrap();

        let mut xdr_bytes = Vec::new();
        val.pack(&mut xdr_bytes).unwrap();

        assert_eq!(
            our_bytes, xdr_bytes,
            "i32 encoding for {} should match xdr-codec",
            val
        );
    }
}

#[test]
fn test_bool_true() {
    let val: bool = true;
    let bytes = to_vec(&val).unwrap();
    assert_eq!(bytes, &[0, 0, 0, 1]);
    let decoded: bool = from_slice(&bytes).unwrap();
    assert_eq!(decoded, val);
}

#[test]
fn test_bool_false() {
    let val: bool = false;
    let bytes = to_vec(&val).unwrap();
    assert_eq!(bytes, &[0, 0, 0, 0]);
    let decoded: bool = from_slice(&bytes).unwrap();
    assert_eq!(decoded, val);
}

#[test]
fn test_bool_xdr_codec_compat() {
    use xdr_codec::Pack;

    for val in [true, false] {
        let our_bytes = to_vec(&val).unwrap();

        let mut xdr_bytes = Vec::new();
        val.pack(&mut xdr_bytes).unwrap();

        assert_eq!(
            our_bytes, xdr_bytes,
            "bool encoding for {} should match xdr-codec",
            val
        );
    }
}

#[test]
fn test_string() {
    let val = String::from("hello");
    let bytes = to_vec(&val).unwrap();

    // XDR string: length (4 bytes) + data (5 bytes) + padding (3 bytes) = 12 bytes
    assert_eq!(bytes.len(), 12);
    assert_eq!(&bytes[0..4], &[0, 0, 0, 5]); // length = 5
    assert_eq!(&bytes[4..9], b"hello");
    assert_eq!(&bytes[9..12], &[0, 0, 0]); // padding

    let decoded: String = from_slice(&bytes).unwrap();
    assert_eq!(decoded, val);
}

#[test]
fn test_string_no_padding_needed() {
    let val = String::from("test"); // 4 bytes, no padding needed
    let bytes = to_vec(&val).unwrap();

    assert_eq!(bytes.len(), 8);
    assert_eq!(&bytes[0..4], &[0, 0, 0, 4]); // length = 4
    assert_eq!(&bytes[4..8], b"test");

    let decoded: String = from_slice(&bytes).unwrap();
    assert_eq!(decoded, val);
}

#[test]
fn test_string_xdr_codec_compat() {
    use xdr_codec::Pack;

    for val in ["", "a", "ab", "abc", "abcd", "hello", "hello world"] {
        let our_bytes = to_vec(&val.to_string()).unwrap();

        let mut xdr_bytes = Vec::new();
        val.pack(&mut xdr_bytes).unwrap();

        assert_eq!(
            our_bytes, xdr_bytes,
            "string encoding for {:?} should match xdr-codec",
            val
        );
    }
}

#[test]
fn test_f32() {
    let val: f32 = 1.5;
    let bytes = to_vec(&val).unwrap();
    assert_eq!(bytes.len(), 4);
    let decoded: f32 = from_slice(&bytes).unwrap();
    assert_eq!(decoded, val);
}

#[test]
fn test_f32_xdr_codec_compat() {
    use xdr_codec::Pack;

    for val in [
        0.0f32,
        1.0,
        -1.0,
        1.5,
        std::f32::consts::PI,
        f32::MIN,
        f32::MAX,
    ] {
        let our_bytes = to_vec(&val).unwrap();

        let mut xdr_bytes = Vec::new();
        val.pack(&mut xdr_bytes).unwrap();

        assert_eq!(
            our_bytes, xdr_bytes,
            "f32 encoding for {} should match xdr-codec",
            val
        );
    }
}

#[test]
fn test_f64() {
    let val: f64 = std::f64::consts::PI;
    let bytes = to_vec(&val).unwrap();
    assert_eq!(bytes.len(), 8);
    let decoded: f64 = from_slice(&bytes).unwrap();
    assert_eq!(decoded, val);
}

#[test]
fn test_f64_xdr_codec_compat() {
    use xdr_codec::Pack;

    for val in [
        0.0f64,
        1.0,
        -1.0,
        1.5,
        std::f64::consts::PI,
        f64::MIN,
        f64::MAX,
    ] {
        let our_bytes = to_vec(&val).unwrap();

        let mut xdr_bytes = Vec::new();
        val.pack(&mut xdr_bytes).unwrap();

        assert_eq!(
            our_bytes, xdr_bytes,
            "f64 encoding for {} should match xdr-codec",
            val
        );
    }
}

#[test]
fn test_u64() {
    let val: u64 = 0x123456789ABCDEF0;
    let bytes = to_vec(&val).unwrap();
    assert_eq!(bytes.len(), 8);
    let decoded: u64 = from_slice(&bytes).unwrap();
    assert_eq!(decoded, val);
}

#[test]
fn test_u64_xdr_codec_compat() {
    use xdr_codec::Pack;

    for val in [0u64, 1, u64::MAX, 0x123456789ABCDEF0] {
        let our_bytes = to_vec(&val).unwrap();

        let mut xdr_bytes = Vec::new();
        val.pack(&mut xdr_bytes).unwrap();

        assert_eq!(
            our_bytes, xdr_bytes,
            "u64 encoding for {} should match xdr-codec",
            val
        );
    }
}

#[test]
fn test_i64() {
    let val: i64 = -0x123456789ABCDEF0;
    let bytes = to_vec(&val).unwrap();
    assert_eq!(bytes.len(), 8);
    let decoded: i64 = from_slice(&bytes).unwrap();
    assert_eq!(decoded, val);
}

#[test]
fn test_i64_xdr_codec_compat() {
    use xdr_codec::Pack;

    for val in [0i64, 1, -1, i64::MIN, i64::MAX] {
        let our_bytes = to_vec(&val).unwrap();

        let mut xdr_bytes = Vec::new();
        val.pack(&mut xdr_bytes).unwrap();

        assert_eq!(
            our_bytes, xdr_bytes,
            "i64 encoding for {} should match xdr-codec",
            val
        );
    }
}

#[derive(Facet, Debug, PartialEq)]
struct WithString {
    id: u32,
    name: String,
}

#[test]
fn test_struct_with_string() {
    let s = WithString {
        id: 42,
        name: "test".to_string(),
    };
    let bytes = to_vec(&s).unwrap();
    let decoded: WithString = from_slice(&bytes).unwrap();
    assert_eq!(decoded, s);
}

#[test]
fn test_vec_u32() {
    let val: Vec<u32> = vec![1, 2, 3];
    let bytes = to_vec(&val).unwrap();

    // XDR: length (4 bytes) + 3 * u32 (12 bytes) = 16 bytes
    assert_eq!(bytes.len(), 16);

    let decoded: Vec<u32> = from_slice(&bytes).unwrap();
    assert_eq!(decoded, val);
}

#[test]
fn test_vec_xdr_codec_compat() {
    use xdr_codec::Pack;

    let val: Vec<u32> = vec![1, 2, 3];

    let our_bytes = to_vec(&val).unwrap();

    let mut xdr_bytes = Vec::new();
    val.pack(&mut xdr_bytes).unwrap();

    assert_eq!(
        our_bytes, xdr_bytes,
        "Vec<u32> encoding should match xdr-codec"
    );
}

#[test]
fn test_option_some_xdr_codec_compat() {
    use xdr_codec::Pack;

    let val: Option<u32> = Some(42);

    let our_bytes = to_vec(&val).unwrap();

    let mut xdr_bytes = Vec::new();
    val.pack(&mut xdr_bytes).unwrap();

    assert_eq!(
        our_bytes, xdr_bytes,
        "Option<u32> Some encoding should match xdr-codec"
    );
}

#[test]
fn test_option_none_xdr_codec_compat() {
    use xdr_codec::Pack;

    let val: Option<u32> = None;

    let our_bytes = to_vec(&val).unwrap();

    let mut xdr_bytes = Vec::new();
    val.pack(&mut xdr_bytes).unwrap();

    assert_eq!(
        our_bytes, xdr_bytes,
        "Option<u32> None encoding should match xdr-codec"
    );
}
