use facet::Facet;
use facet_json::{from_str, to_string};

#[derive(Debug, Facet, PartialEq)]
struct U64Array {
    values: [u64; 3],
}

#[test]
fn test_serialize_u64_array() {
    facet_testhelpers::setup();

    let json = r#"{"values": [1, 2, 3]}"#;

    let result = from_str::<U64Array>(json);
    match result {
        Ok(data) => {
            assert_eq!(data.values, [1, 2, 3]);
        }
        Err(e) => {
            panic!("Failed to parse u64 array: {}", e);
        }
    }
}

#[derive(Debug, Facet, PartialEq)]
struct NestedU64Array {
    matrix: [[u64; 2]; 2],
}

#[test]
fn test_serialize_nested_u64_array() {
    facet_testhelpers::setup();

    let json = r#"{"matrix": [[1, 2], [3, 4]]}"#;

    let result = from_str::<NestedU64Array>(json);
    match result {
        Ok(data) => {
            assert_eq!(data.matrix, [[1, 2], [3, 4]]);
        }
        Err(e) => {
            panic!("Failed to parse nested u64 array: {}", e);
        }
    }
}

#[test]
fn test_serialize_u64_array_overflow() {
    facet_testhelpers::setup();

    let json = r#"{"values": [1, 2, 3, 4]}"#;

    let result = from_str::<U64Array>(json);
    assert!(result.is_err());
    if let Err(e) = result {
        let error_msg = format!("{}", e);
        assert!(
            error_msg.contains("Too many elements") || error_msg.contains("maximum 3 elements")
        );
    }
}

#[test]
fn test_deserialize_u64_array() {
    facet_testhelpers::setup();

    let s = U64Array { values: [1, 2, 3] };
    let _ = to_string(&s);
}

#[test]
fn test_deserialize_u8_array() {
    facet_testhelpers::setup();

    #[derive(Facet)]
    struct U8Array {
        values: [u8; 2],
    }

    let p = U8Array { values: [1, 2] };
    let _ = to_string(&p);
}
