//! Regression test for <https://github.com/facet-rs/facet/issues/2010>
//!
//! Bug: When a struct with #[facet(flatten)] on a tagged enum is nested in a
//! HashMap inside wrapper structs, facet fails to deserialize if variant fields
//! appear before the tag field in JSON.

use std::collections::HashMap;

use facet::Facet;
use facet_testhelpers::test;

#[derive(Clone, Debug, Facet, PartialEq)]
#[facet(tag = "kind")]
#[repr(C)]
pub enum Inner {
    TypeA { value: f64 },
    TypeB { alpha: f64, beta: f64 },
}

#[derive(Clone, Debug, Facet, PartialEq)]
pub struct Item {
    #[facet(flatten)]
    pub inner: Inner,
    pub extra: Option<String>,
}

#[derive(Clone, Debug, Facet, PartialEq)]
pub struct Container {
    pub items: Option<HashMap<String, Item>>,
}

#[derive(Clone, Debug, Facet, PartialEq)]
pub struct Outer {
    pub container: Container,
}

#[test]
fn tag_before_fields() {
    let json = r#"{
        "container": {
            "items": {
                "x": {
                    "kind": "TypeB",
                    "alpha": 1.0,
                    "beta": 2.0,
                    "extra": "test"
                }
            }
        }
    }"#;

    let outer: Outer = facet_json::from_str(json).expect("tag before fields should work");
    assert_eq!(
        outer.container.items.as_ref().unwrap()["x"].inner,
        Inner::TypeB {
            alpha: 1.0,
            beta: 2.0
        }
    );
}

#[test]
fn fields_before_tag() {
    let json = r#"{
        "container": {
            "items": {
                "x": {
                    "alpha": 1.0,
                    "extra": "test",
                    "kind": "TypeB",
                    "beta": 2.0
                }
            }
        }
    }"#;

    let outer: Outer = facet_json::from_str(json).expect("fields before tag should work");
    assert_eq!(
        outer.container.items.as_ref().unwrap()["x"].inner,
        Inner::TypeB {
            alpha: 1.0,
            beta: 2.0
        }
    );
}
