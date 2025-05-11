use facet::Facet;

#[derive(Facet, Debug, PartialEq)]
struct TestStruct {
    string_field: String,
}

#[test]
fn test_string_parsing_with_memchr() {
    // Create a JSON string with a moderately long string to test the memchr optimization
    let json = r#"{"string_field": "This is a moderately long string that should benefit from memchr optimizations for quicker string boundary detection"}"#;

    // Parse the JSON string
    let result: TestStruct = facet_json::from_str(json).unwrap();

    // Verify the result is correct
    assert_eq!(
        result.string_field,
        "This is a moderately long string that should benefit from memchr optimizations for quicker string boundary detection"
    );
}

#[test]
fn test_string_with_escapes() {
    // Create a JSON string with escapes to test both paths
    let json = r#"{"string_field": "String with escapes: \", \\, \/, \b, \f, \n, \r, \t and unicode: \u0041\u0042\u0043"}"#;

    // Parse the JSON string
    let result: TestStruct = facet_json::from_str(json).unwrap();

    // Verify the result is correct
    assert_eq!(
        result.string_field,
        "String with escapes: \", \\, /, \x08, \x0C, \n, \r, \t and unicode: ABC"
    );
}
