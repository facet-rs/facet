use facet::Facet;
use serde::{Deserialize, Serialize};

#[derive(Debug, PartialEq, Clone, Facet, Serialize, Deserialize)]
struct SimpleString {
    text: String,
}

fn main() {
    let test = SimpleString {
        text: "Test string".to_string(),
    };

    println!("Testing with facet-json...");
    let facet_result = facet_json::to_string(&test);
    println!("facet_json result: {:?}", facet_result);

    println!("Testing with serde_json...");
    let serde_result = serde_json::to_string(&test);
    println!("serde_json result: {:?}", serde_result);
}
