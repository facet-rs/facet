use facet::Facet;
use serde::{Deserialize, Serialize};

#[derive(Debug, PartialEq, Clone, Facet, Serialize, Deserialize)]
struct SimpleString {
    text: String,
}

#[test]
fn test_string_serialization() {
    let test = SimpleString {
        text: "Test string".to_string(),
    };

    // Test with facet-json (returns String directly)
    let facet_str = facet_json::to_string(&test);
    println!("facet_json result: {:?}", facet_str);

    // Test with serde_json (returns Result<String, Error>)
    let serde_result = serde_json::to_string(&test);
    println!("serde_json result: {:?}", serde_result);

    // Verify serde serialization was successful
    assert!(
        serde_result.is_ok(),
        "serde_json serialization should succeed"
    );
    let serde_str = serde_result.unwrap();

    // Check that both libraries produce equivalent JSON
    assert_eq!(
        serde_json::from_str::<serde_json::Value>(&facet_str).unwrap(),
        serde_json::from_str::<serde_json::Value>(&serde_str).unwrap(),
        "The JSON produced by both libraries should be equivalent"
    );
}
