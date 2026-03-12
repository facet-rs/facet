//! Test case for issue #1474: zero-copy deserialization fails for borrowed types.
//!
//! This test verifies that borrowed types like `Cow<'a, str>` and `&'a [u8]`
//! can be properly serialized and deserialized with facet-postcard.

#![cfg(feature = "jit")]

use facet::Facet;
use facet_postcard::{from_slice, from_slice_borrowed, to_vec};
use std::borrow::Cow;

/// Test Cow<'a, str> deserialization
#[derive(Debug, PartialEq, Facet)]
struct CowOnly<'a> {
    message: Cow<'a, str>,
    count: u32,
}

/// Test &'a [u8] deserialization
#[derive(Debug, PartialEq, Facet)]
struct BytesOnly<'a> {
    data: &'a [u8],
    count: u32,
}

#[test]
fn test_cow_str() {
    facet_testhelpers::setup();

    let original = CowOnly {
        message: Cow::Borrowed("hello"),
        count: 42,
    };
    let bytes = to_vec(&original).expect("serialization should succeed");
    eprintln!("Serialized bytes: {:?}", bytes);

    let decoded: CowOnly<'_> = from_slice(&bytes).expect("deserialization should succeed");
    assert_eq!(decoded.message, "hello");
    assert_eq!(decoded.count, 42);
}

#[test]
fn test_bytes_slice() {
    facet_testhelpers::setup();

    let original = BytesOnly {
        data: b"hello",
        count: 42,
    };
    let bytes = to_vec(&original).expect("serialization should succeed");
    eprintln!("Serialized bytes: {:?}", bytes);

    // &[u8] requires from_slice_borrowed since it cannot be owned
    let decoded: BytesOnly<'_> =
        from_slice_borrowed(&bytes).expect("deserialization should succeed");
    assert_eq!(decoded.data, b"hello");
    assert_eq!(decoded.count, 42);
}

// Additional test: Cow::Owned variant
#[test]
fn test_cow_str_owned() {
    facet_testhelpers::setup();

    let original = CowOnly {
        message: Cow::Owned("hello world".to_string()),
        count: 100,
    };
    let bytes = to_vec(&original).expect("serialization should succeed");
    eprintln!("Serialized bytes: {:?}", bytes);

    let decoded: CowOnly<'_> = from_slice(&bytes).expect("deserialization should succeed");
    assert_eq!(decoded.message, "hello world");
    assert_eq!(decoded.count, 100);
}

/// Test Cow<'a, [u8]> deserialization
#[derive(Debug, PartialEq, Facet)]
struct CowBytes<'a> {
    data: Cow<'a, [u8]>,
    count: u32,
}

#[test]
fn test_cow_bytes() {
    facet_testhelpers::setup();

    let original = CowBytes {
        data: Cow::Borrowed(b"hello"),
        count: 42,
    };
    let bytes = to_vec(&original).expect("serialization should succeed");
    eprintln!("Serialized bytes: {:?}", bytes);

    let decoded: CowBytes<'_> = from_slice(&bytes).expect("deserialization should succeed");
    assert_eq!(decoded.data.as_ref(), b"hello");
    assert_eq!(decoded.count, 42);
}

#[test]
fn test_cow_bytes_owned() {
    facet_testhelpers::setup();

    let original = CowBytes {
        data: Cow::Owned(vec![1, 2, 3, 4, 5]),
        count: 100,
    };
    let bytes = to_vec(&original).expect("serialization should succeed");
    eprintln!("Serialized bytes: {:?}", bytes);

    let decoded: CowBytes<'_> = from_slice(&bytes).expect("deserialization should succeed");
    assert_eq!(decoded.data.as_ref(), &[1, 2, 3, 4, 5]);
    assert_eq!(decoded.count, 100);
}

// Test that deserialized Cow<str> can borrow from input (zero-copy)
#[test]
fn test_cow_str_zero_copy() {
    facet_testhelpers::setup();

    let original = CowOnly {
        message: Cow::Borrowed("zero-copy test"),
        count: 1,
    };
    let bytes = to_vec(&original).expect("serialization should succeed");

    let decoded: CowOnly<'_> = from_slice(&bytes).expect("deserialization should succeed");
    // The decoded Cow should be Borrowed when data is not escaped
    assert!(matches!(decoded.message, Cow::Borrowed(_)));
    assert_eq!(decoded.message, "zero-copy test");
}

// Test that deserialized &[u8] can borrow from input (zero-copy)
#[test]
fn test_bytes_slice_zero_copy() {
    facet_testhelpers::setup();

    let original = BytesOnly {
        data: b"zero-copy bytes",
        count: 2,
    };
    let bytes = to_vec(&original).expect("serialization should succeed");

    // &[u8] requires from_slice_borrowed for zero-copy deserialization
    let decoded: BytesOnly<'_> =
        from_slice_borrowed(&bytes).expect("deserialization should succeed");
    assert_eq!(decoded.data, b"zero-copy bytes");
    assert_eq!(decoded.count, 2);
}
