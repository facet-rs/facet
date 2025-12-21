//! Test for Vec<Struct> deserialization

use std::collections::HashMap;

use facet::Facet;
use facet_testhelpers::test;

#[derive(Debug, Facet, PartialEq)]
struct Person {
    name: String,
    age: u64,
}

#[derive(Debug, Facet, PartialEq)]
struct Root {
    people: Vec<Person>,
}

#[test]
fn test_deserialize_vec_struct() {
    assert_eq!(
        facet_toml::from_str::<Root>(
            r#"
            [[people]]
            name = "Alice"
            age = 30

            [[people]]
            name = "Bob"
            age = 25
            "#
        )
        .unwrap(),
        Root {
            people: vec![
                Person {
                    name: "Alice".to_string(),
                    age: 30,
                },
                Person {
                    name: "Bob".to_string(),
                    age: 25,
                },
            ],
        }
    );
}

// Issue #1303: regression tests for nested tables and default HashMap in array tables

#[derive(Debug, Facet, PartialEq)]
struct NestedInner {
    tag: String,
}

#[derive(Debug, Facet, PartialEq)]
struct NestedOuter {
    name: String,
    inner: NestedInner,
}

#[derive(Debug, Facet, PartialEq)]
struct NestedRoot {
    nested: Vec<NestedOuter>,
}

#[test]
fn nested_inline() {
    assert_eq!(
        facet_toml::from_str::<NestedRoot>(
            r#"
            [[nested]]
            name = "inline"
            inner = { tag = "inline tag" }
            "#
        )
        .unwrap(),
        NestedRoot {
            nested: vec![NestedOuter {
                name: "inline".to_string(),
                inner: NestedInner {
                    tag: "inline tag".to_string(),
                },
            },],
        }
    );
}

#[test]
fn nested_separate() {
    assert_eq!(
        facet_toml::from_str::<NestedRoot>(
            r#"
            [[nested]]
            name = "separate"

            [nested.inner]
            tag = "not an inline tag"
            "#
        )
        .unwrap(),
        NestedRoot {
            nested: vec![NestedOuter {
                name: "separate".to_string(),
                inner: NestedInner {
                    tag: "not an inline tag".to_string(),
                },
            },],
        }
    );
}

#[derive(Debug, Facet, PartialEq)]
struct Mapped {
    name: String,
    #[facet(default = HashMap::<String, String>::new())]
    features: HashMap<String, String>,
}

#[derive(Debug, Facet, PartialEq)]
struct MappedRoot {
    items: Vec<Mapped>,
}

#[test]
fn mapped_full() {
    assert_eq!(
        facet_toml::from_str::<MappedRoot>(
            r#"
            [[items]]
            name = "full"
            features = {"minimal" = "not at all"}
            "#
        )
        .unwrap(),
        MappedRoot {
            items: vec![Mapped {
                name: "full".to_string(),
                features: [("minimal".to_string(), "not at all".to_string())].into(),
            },],
        }
    );
}

#[test]
fn mapped_separate() {
    assert_eq!(
        facet_toml::from_str::<MappedRoot>(
            r#"
            [[items]]
            name = "separate"

            [items.features]
            separate = "below"
            "#
        )
        .unwrap(),
        MappedRoot {
            items: vec![Mapped {
                name: "separate".to_string(),
                features: [("separate".to_string(), "below".to_string())].into(),
            },],
        }
    );
}

#[test]
fn mapped_minimal() {
    assert_eq!(
        facet_toml::from_str::<MappedRoot>(
            r#"
            [[items]]
            name = "minimal"
            "#
        )
        .unwrap(),
        MappedRoot {
            items: vec![Mapped {
                name: "minimal".to_string(),
                features: HashMap::new(),
            },],
        }
    );
}
