//! Test Arc deserialization without flatten to isolate the issue

use facet::Facet;
use facet_testhelpers::test;
use std::sync::Arc;

#[derive(Clone, Debug, Facet)]
pub struct Inner {
    pub x: Vec<f64>,
    pub y: Vec<f64>,
}

#[derive(Clone, Debug, Facet)]
#[facet(tag = "kind")]
#[repr(C)]
pub enum Tagged {
    TypeA { value: f64, data: Arc<Inner> },
}

#[derive(Clone, Debug, Facet)]
pub struct Item {
    pub tagged: Tagged, // No flatten
}

#[derive(Clone, Debug, Facet)]
pub struct Root {
    pub item: Item,
}

#[test]
fn test_arc_no_flatten_deserialization() {
    let json = r#"{
    "item": {
      "tagged": {
        "kind": "TypeA",
        "value": 42.0,
        "data": {
          "x": [1.0, 2.0],
          "y": [100.0, 200.0]
        }
      }
    }
  }"#;

    let root = facet_json::from_str::<Root>(json).expect("deserialization failed");

    match &root.item.tagged {
        Tagged::TypeA { value, data } => {
            assert_eq!(*value, 42.0);
            assert!(!data.y.is_empty(), "Arc<Inner> not deserialized correctly");
            assert_eq!(data.y.first(), Some(&100.0));
            assert_eq!(data.x.first(), Some(&1.0));
        }
    }
}
