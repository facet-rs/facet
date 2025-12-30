//! Test that structs with multiple String fields work with facet-format-postcard

use facet::Facet;

#[derive(Debug, Clone, PartialEq, Facet)]
struct Field {
    name: String,
    value: String,
}

#[test]
fn test_struct_with_two_strings() {
    let field = Field {
        name: "test".to_string(),
        value: "data".to_string(),
    };

    let bytes = rapace::facet_postcard::to_vec(&field).expect("serialize");
    println!("Serialized {} bytes: {:?}", bytes.len(), bytes);

    let decoded: Field = rapace::facet_postcard::from_slice(&bytes).expect("deserialize");
    println!("Decoded: {:?}", decoded);
    assert_eq!(decoded, field);
}

#[test]
fn test_vec_of_fields() {
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

    let bytes = rapace::facet_postcard::to_vec(&fields).expect("serialize");
    println!("Serialized {} bytes", bytes.len());

    let decoded: Vec<Field> = rapace::facet_postcard::from_slice(&bytes).expect("deserialize");
    println!("Decoded {} fields", decoded.len());
    assert_eq!(decoded, fields);
}
