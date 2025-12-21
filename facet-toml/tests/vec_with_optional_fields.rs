//! Test that Vec<T> works with optional fields in T
//!
//! KNOWN ISSUE: Optional fields within array-of-tables items are not properly
//! handled when missing. This is tracked as a separate issue and is not
//! related to the Option<Vec<T>> bug fixed in issue #1341.

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

    let result = facet_toml::from_str::<Root>(toml);

    // This is a known bug - optional fields within array-of-tables items
    // are not properly handled when missing
    assert!(
        result.is_err(),
        "Currently fails due to known bug with optional fields in array items"
    );

    let err = result.unwrap_err();
    let err_str = err.to_string();
    assert!(
        err_str.contains("Field 'Item::desc' was not initialized"),
        "Error should mention uninitialized field, got: {err_str}"
    );
}
