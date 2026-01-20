//! Test @one-of schema constraint validation.

use facet_styx::{from_str, validate, SchemaFile};
use styx_tree::parse;

fn main() {
    // Schema with @one-of
    let schema_src = r#"
meta {
    id test
    version 1.0.0
}

schema {
    @ @object{
        level @one-of(@string (debug info warn error))
    }
}
"#;

    // Parse schema
    let schema: SchemaFile = from_str(schema_src).expect("failed to parse schema");
    println!("Schema parsed successfully!");

    // Valid document
    let valid_doc = parse("level info").expect("failed to parse valid doc");
    let result = validate(&valid_doc, &schema);
    println!("Validating 'level info': valid={}", result.is_valid());
    assert!(result.is_valid(), "expected 'info' to be valid");

    // Invalid document
    let invalid_doc = parse("level trace").expect("failed to parse invalid doc");
    let result = validate(&invalid_doc, &schema);
    println!("Validating 'level trace': valid={}", result.is_valid());
    for err in &result.errors {
        println!("  Error: {}", err.message);
    }
    assert!(!result.is_valid(), "expected 'trace' to be invalid");

    // Another valid value
    let valid_doc2 = parse("level error").expect("failed to parse valid doc");
    let result = validate(&valid_doc2, &schema);
    println!("Validating 'level error': valid={}", result.is_valid());
    assert!(result.is_valid(), "expected 'error' to be valid");

    println!("\nAll tests passed!");
}
