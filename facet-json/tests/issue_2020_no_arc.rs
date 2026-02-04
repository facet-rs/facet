//! Test Arc deserialization without Arc to isolate the issue

use facet::Facet;
use facet_testhelpers::test;
use std::collections::HashMap;

#[derive(Clone, Debug, Facet)]
pub struct Inner {
    pub x: Vec<f64>,
    pub y: Vec<f64>,
}

#[derive(Clone, Debug, Facet)]
#[facet(tag = "kind")]
#[repr(C)]
pub enum Tagged {
    TypeA { value: f64, data: Inner }, // No Arc
}

#[derive(Clone, Debug, Facet)]
pub struct Item {
    #[facet(flatten)]
    pub tagged: Tagged,
}

#[derive(Clone, Debug, Facet)]
pub struct Container {
    pub items: Option<HashMap<String, Item>>,
}

#[derive(Clone, Debug, Facet)]
pub struct Root {
    pub container: Container,
}

#[test]
fn test_no_arc_deserialization() {
    let json = r#"{
    "container": {
      "items": {
        "a": {
          "kind": "TypeA",
          "value": 42.0,
          "data": {
            "x": [1.0, 2.0],
            "y": [100.0, 200.0]
          }
        }
      }
    }
  }"#;

    let root = facet_json::from_str::<Root>(json).expect("deserialization failed");

    if let Some(items) = &root.container.items {
        if let Some(item) = items.get("a") {
            match &item.tagged {
                Tagged::TypeA { value, data } => {
                    assert_eq!(*value, 42.0);
                    assert!(!data.y.is_empty(), "Inner not deserialized correctly");
                    assert_eq!(data.y.get(0), Some(&100.0));
                    assert_eq!(data.x.get(0), Some(&1.0));
                }
            }
        }
    }
}
