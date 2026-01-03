//! Regression test for https://github.com/facet-rs/facet/issues/1604
//!
//! Tables nested in arrays should deserialize correctly.

use facet::Facet;

#[derive(Facet, Debug, PartialEq)]
struct Root {
    pub item: Vec<Item>,
}

#[derive(Facet, Debug, PartialEq)]
struct Item {
    pub foo: u8,
    pub nested_item: NestedItem,
}

#[derive(Facet, Debug, PartialEq)]
struct NestedItem {
    pub foo: u8,
    pub bar: u8,
}

#[test]
fn table_nested_in_array() {
    let toml = r#"
        [[item]]
        foo = 1

        [item.nested_item]
        foo = 1
        bar = 3
    "#;

    let result: Root = facet_toml::from_str(toml).unwrap();
    assert_eq!(result.item.len(), 1);
    assert_eq!(result.item[0].foo, 1);
    assert_eq!(result.item[0].nested_item.foo, 1);
    assert_eq!(result.item[0].nested_item.bar, 3);
}

#[test]
fn multiple_array_elements_with_nested_tables() {
    let toml = r#"
        [[item]]
        foo = 1

        [item.nested_item]
        foo = 10
        bar = 11

        [[item]]
        foo = 2

        [item.nested_item]
        foo = 20
        bar = 21
    "#;

    let result: Root = facet_toml::from_str(toml).unwrap();
    assert_eq!(result.item.len(), 2);

    assert_eq!(result.item[0].foo, 1);
    assert_eq!(result.item[0].nested_item.foo, 10);
    assert_eq!(result.item[0].nested_item.bar, 11);

    assert_eq!(result.item[1].foo, 2);
    assert_eq!(result.item[1].nested_item.foo, 20);
    assert_eq!(result.item[1].nested_item.bar, 21);
}

#[derive(Facet, Debug, PartialEq)]
struct WithNestedArray {
    pub items: Vec<ItemWithSubarray>,
}

#[derive(Facet, Debug, PartialEq)]
struct ItemWithSubarray {
    pub name: String,
    pub subitems: Vec<SubItem>,
}

#[derive(Facet, Debug, PartialEq)]
struct SubItem {
    pub value: u32,
}

#[test]
fn nested_array_tables() {
    // Test [[items.subitems]] syntax for nested arrays
    let toml = r#"
        [[items]]
        name = "first"

        [[items.subitems]]
        value = 1

        [[items.subitems]]
        value = 2

        [[items]]
        name = "second"

        [[items.subitems]]
        value = 3
    "#;

    let result: WithNestedArray = facet_toml::from_str(toml).unwrap();
    assert_eq!(result.items.len(), 2);

    assert_eq!(result.items[0].name, "first");
    assert_eq!(result.items[0].subitems.len(), 2);
    assert_eq!(result.items[0].subitems[0].value, 1);
    assert_eq!(result.items[0].subitems[1].value, 2);

    assert_eq!(result.items[1].name, "second");
    assert_eq!(result.items[1].subitems.len(), 1);
    assert_eq!(result.items[1].subitems[0].value, 3);
}
