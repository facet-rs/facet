use facet::Facet;
use facet_json::from_str;

#[derive(Debug, Facet, PartialEq)]
struct SimpleArray {
    values: [u64; 3], // JSON parser seems to default to u64 for integers
}

#[test]
fn test_simple_array() {
    let json = r#"{"values": [1, 2, 3]}"#;

    let result = from_str::<SimpleArray>(json);
    match result {
        Ok(data) => {
            assert_eq!(data.values, [1u64, 2u64, 3u64]);
        }
        Err(e) => {
            panic!("Failed to parse simple array: {}", e);
        }
    }
}

#[test]
fn test_array_overflow() {
    let json = r#"{"values": [1, 2, 3, 4]}"#;

    let result = from_str::<SimpleArray>(json);
    assert!(result.is_err());
    if let Err(e) = result {
        let error_msg = format!("{}", e);
        assert!(
            error_msg.contains("Too many elements") || error_msg.contains("maximum 3 elements")
        );
    }
}

#[derive(Debug, Facet, PartialEq)]
struct NestedArraySimple {
    matrix: [[u64; 2]; 2],
}

#[test]
fn test_nested_array_simple() {
    let json = r#"{"matrix": [[1, 2], [3, 4]]}"#;

    let result = from_str::<NestedArraySimple>(json);
    match result {
        Ok(data) => {
            assert_eq!(data.matrix, [[1, 2], [3, 4]]);
        }
        Err(e) => {
            panic!("Failed to parse nested array: {}", e);
        }
    }
}
