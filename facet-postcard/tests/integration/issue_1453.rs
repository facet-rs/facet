//! Regression test for issue #1453: JIT deserialization fails for Vec<Struct>
//! with multiple String fields.

#![cfg(feature = "jit")]

use facet::Facet;
use facet_postcard::{from_slice, to_vec};

#[derive(Debug, Clone, PartialEq, Facet)]
struct Field {
    name: String,
    value: String,
}

#[test]
fn test_single_struct() {
    // ✅ This should work - single struct with multiple strings
    let field = Field {
        name: "test".to_string(),
        value: "data".to_string(),
    };
    let bytes = to_vec(&field).unwrap();
    let decoded: Field = from_slice(&bytes).unwrap();
    assert_eq!(decoded, field);
}

#[test]
fn test_vec_of_structs() {
    // ❌ This was failing before the fix
    let fields = vec![
        Field {
            name: "user".to_string(),
            value: "bob".to_string(),
        },
        Field {
            name: "count".to_string(),
            value: "42".to_string(),
        },
    ];
    let bytes = to_vec(&fields).unwrap();

    // Was failing with: Parser(PostcardError { code: -100, pos: 3, message: "unexpected end of input" })
    let decoded: Vec<Field> = from_slice(&bytes).unwrap();
    assert_eq!(decoded, fields);
}

#[test]
fn test_vec_of_structs_roundtrip() {
    // More comprehensive test
    let fields = vec![
        Field {
            name: "a".to_string(),
            value: "b".to_string(),
        },
        Field {
            name: "longer_name".to_string(),
            value: "longer_value".to_string(),
        },
        Field {
            name: "".to_string(),
            value: "".to_string(),
        },
    ];

    let encoded = to_vec(&fields).expect("should serialize");
    let decoded: Vec<Field> = from_slice(&encoded).expect("should deserialize");
    assert_eq!(decoded, fields);
}
