//! Regression test for https://github.com/facet-rs/facet/issues/1644
//!
//! Table with flatten nested in an array fails to deserialize.
//! The error was: "must call begin_map() before begin_key()"

use facet_testhelpers::test;

use std::collections::HashMap;

use facet::Facet;
use facet_value::Value;

#[derive(Facet, Debug)]
struct Root {
    pub item: Vec<Item>,
}

#[derive(Facet, Debug)]
struct Item {
    pub nested_item: NestedItem,
}

#[derive(Facet, Debug)]
struct NestedItem {
    #[facet(flatten)]
    pub extra: HashMap<String, Value>,
}

#[test]
fn table_in_array_with_flatten() {
    let toml = r#"
        [[item]]

        [item.nested_item]
        foo = 1
        bar = 2
    "#;

    let result: Root = facet_toml::from_str(toml).unwrap();
    assert_eq!(result.item.len(), 1);
    assert_eq!(result.item[0].nested_item.extra.len(), 2);
    assert!(result.item[0].nested_item.extra.contains_key("foo"));
    assert!(result.item[0].nested_item.extra.contains_key("bar"));
}

/// Test with multiple array elements
#[test]
fn multiple_array_elements_with_flatten() {
    let toml = r#"
        [[item]]

        [item.nested_item]
        foo = 1
        bar = 2

        [[item]]

        [item.nested_item]
        baz = 3
    "#;

    let result: Root = facet_toml::from_str(toml).unwrap();
    assert_eq!(result.item.len(), 2);
    assert_eq!(result.item[0].nested_item.extra.len(), 2);
    assert_eq!(result.item[1].nested_item.extra.len(), 1);
}

/// Test with a single key (reportedly this works)
#[test]
fn flatten_with_single_key_works() {
    let toml = r#"
        [[item]]

        [item.nested_item]
        foo = 1
    "#;

    let result: Root = facet_toml::from_str(toml).unwrap();
    assert_eq!(result.item.len(), 1);
    assert_eq!(result.item[0].nested_item.extra.len(), 1);
}
