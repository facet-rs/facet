#![cfg(feature = "bytes")]

use bytes::{Bytes, BytesMut};
use facet::Facet;
use facet_postcard::{from_slice, to_vec};

#[test]
fn test_bytes_roundtrip() {
    facet_testhelpers::setup();

    let data = Bytes::from_static(b"hello world");
    let encoded = to_vec(&data).unwrap();
    let decoded: Bytes = from_slice(&encoded).unwrap();
    assert_eq!(data, decoded);
}

#[test]
fn test_bytes_empty() {
    facet_testhelpers::setup();

    let data = Bytes::new();
    let encoded = to_vec(&data).unwrap();
    let decoded: Bytes = from_slice(&encoded).unwrap();
    assert_eq!(data, decoded);
}

#[test]
fn test_bytes_large() {
    facet_testhelpers::setup();

    let data = Bytes::from(vec![42u8; 1000]);
    let encoded = to_vec(&data).unwrap();
    let decoded: Bytes = from_slice(&encoded).unwrap();
    assert_eq!(data, decoded);
}

#[test]
fn test_bytes_in_struct() {
    facet_testhelpers::setup();

    #[derive(Facet, PartialEq, Debug)]
    struct Packet {
        header: String,
        payload: Bytes,
    }

    let original = Packet {
        header: "v1.0".to_string(),
        payload: Bytes::from_static(b"binary data"),
    };

    let encoded = to_vec(&original).unwrap();
    let decoded: Packet = from_slice(&encoded).unwrap();
    assert_eq!(original, decoded);
}

#[test]
fn test_bytes_in_option() {
    facet_testhelpers::setup();

    #[derive(Facet, PartialEq, Debug)]
    struct WithOptionalBytes {
        data: Option<Bytes>,
    }

    // Test Some variant
    let original = WithOptionalBytes {
        data: Some(Bytes::from_static(b"data")),
    };
    let encoded = to_vec(&original).unwrap();
    let decoded: WithOptionalBytes = from_slice(&encoded).unwrap();
    assert_eq!(original, decoded);

    // Test None variant
    let original = WithOptionalBytes { data: None };
    let encoded = to_vec(&original).unwrap();
    let decoded: WithOptionalBytes = from_slice(&encoded).unwrap();
    assert_eq!(original, decoded);
}

#[test]
fn test_bytes_in_vec() {
    facet_testhelpers::setup();

    #[derive(Facet, PartialEq, Debug)]
    struct BatchData {
        chunks: Vec<Bytes>,
    }

    let original = BatchData {
        chunks: vec![
            Bytes::from_static(b"chunk1"),
            Bytes::from_static(b"chunk2"),
            Bytes::from_static(b"chunk3"),
        ],
    };

    let encoded = to_vec(&original).unwrap();
    let decoded: BatchData = from_slice(&encoded).unwrap();
    assert_eq!(original, decoded);
}

#[test]
fn test_bytes_binary_data() {
    facet_testhelpers::setup();

    // Test with various binary patterns
    let data = Bytes::from(vec![0x00, 0xFF, 0x42, 0xAA, 0x55]);
    let encoded = to_vec(&data).unwrap();
    let decoded: Bytes = from_slice(&encoded).unwrap();
    assert_eq!(data, decoded);
}

#[test]
fn test_bytesmut_roundtrip() {
    facet_testhelpers::setup();

    let mut data = BytesMut::with_capacity(16);
    data.extend_from_slice(b"hello world");
    let encoded = to_vec(&data).unwrap();
    let decoded: BytesMut = from_slice(&encoded).unwrap();
    assert_eq!(data, decoded);
}

#[test]
fn test_bytesmut_empty() {
    facet_testhelpers::setup();

    let data = BytesMut::new();
    let encoded = to_vec(&data).unwrap();
    let decoded: BytesMut = from_slice(&encoded).unwrap();
    assert_eq!(data, decoded);
}

#[test]
fn test_bytesmut_in_struct() {
    facet_testhelpers::setup();

    #[derive(Facet, PartialEq, Debug)]
    struct Buffer {
        name: String,
        data: BytesMut,
    }

    let mut bytes_mut = BytesMut::with_capacity(32);
    bytes_mut.extend_from_slice(b"buffer data");

    let original = Buffer {
        name: "mybuffer".to_string(),
        data: bytes_mut,
    };

    let encoded = to_vec(&original).unwrap();
    let decoded: Buffer = from_slice(&encoded).unwrap();
    assert_eq!(original, decoded);
}

#[test]
fn test_bytes_and_bytesmut_together() {
    facet_testhelpers::setup();

    #[derive(Facet, PartialEq, Debug)]
    struct Mixed {
        immutable: Bytes,
        mutable: BytesMut,
    }

    let mut bytes_mut = BytesMut::new();
    bytes_mut.extend_from_slice(b"mutable");

    let original = Mixed {
        immutable: Bytes::from_static(b"immutable"),
        mutable: bytes_mut,
    };

    let encoded = to_vec(&original).unwrap();
    let decoded: Mixed = from_slice(&encoded).unwrap();
    assert_eq!(original, decoded);
}

#[test]
fn test_bare_bytes() {
    facet_testhelpers::setup();

    let data = Bytes::from_static(b"standalone bytes");
    let encoded = to_vec(&data).unwrap();
    let decoded: Bytes = from_slice(&encoded).unwrap();
    assert_eq!(data, decoded);
}

#[test]
fn test_bare_bytesmut() {
    facet_testhelpers::setup();

    let mut data = BytesMut::new();
    data.extend_from_slice(b"standalone");
    let encoded = to_vec(&data).unwrap();
    let decoded: BytesMut = from_slice(&encoded).unwrap();
    assert_eq!(data, decoded);
}

#[test]
fn test_bytes_serialization_format() {
    facet_testhelpers::setup();

    // Verify that Bytes is serialized the same way as Vec<u8>
    #[derive(Facet, PartialEq, Debug)]
    struct WithVec {
        data: Vec<u8>,
    }

    #[derive(Facet, PartialEq, Debug)]
    struct WithBytes {
        data: Bytes,
    }

    let test_data = b"test data";
    let with_vec = WithVec {
        data: test_data.to_vec(),
    };
    let with_bytes = WithBytes {
        data: Bytes::from_static(test_data),
    };

    let vec_encoded = to_vec(&with_vec).unwrap();
    let bytes_encoded = to_vec(&with_bytes).unwrap();

    // The bytes should be identical since Bytes is just a wrapper around byte sequences
    assert_eq!(vec_encoded, bytes_encoded);
}
