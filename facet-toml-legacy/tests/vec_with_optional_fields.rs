//! Test that Vec<T> works with optional fields in T
//!
//! This was previously a known issue but has been fixed as part of issue #1303.
//! Optional fields within array-of-tables items are now properly handled when missing.

use facet::Facet;

#[derive(Facet, Debug, PartialEq)]
struct Item {
    name: String,
    desc: Option<String>,
}

#[derive(Facet, Debug, PartialEq)]
struct Root {
    items: Vec<Item>,
}

#[test]
fn test_vec_with_some_optional_fields_missing() {
    let toml = r#"
[[items]]
name = "first"
desc = "has desc"

[[items]]
name = "second"
"#;

    let result = facet_toml_legacy::from_str::<Root>(toml);

    // Optional fields within array-of-tables items should work now (fixed in #1303)
    assert!(
        result.is_ok(),
        "Optional fields in array items should work, got error: {:?}",
        result.err()
    );

    let root = result.unwrap();
    assert_eq!(root.items.len(), 2);
    assert_eq!(root.items[0].name, "first");
    assert_eq!(root.items[0].desc, Some("has desc".to_string()));
    assert_eq!(root.items[1].name, "second");
    assert_eq!(root.items[1].desc, None);
}
