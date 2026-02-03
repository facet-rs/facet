//! Debug issue 2020: Arc deserialization bug
use std::{collections::HashMap, sync::Arc};

use facet::Facet;

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

fn main() {
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

    println!("Attempting to deserialize...");
    match facet_json::from_str::<Root>(json) {
        Ok(root) => {
            if let Some(items) = &root.container.items {
                if let Some(item) = items.get("a") {
                    match &item.tagged {
                        Tagged::TypeA { value, data } => {
                            println!("value: {}", value);
                            println!("data.y[0]: {}", data.y.get(0).unwrap_or(&0.0));

                            if data.y.is_empty() || *data.y.get(0).unwrap_or(&0.0) == 0.0 {
                                println!("BUG: Arc<Inner> not deserialized correctly!");
                            } else {
                                println!("SUCCESS: Arc<Inner> deserialized correctly!");
                            }
                        }
                    }
                }
            }
        }
        Err(e) => {
            println!("Failed: {:?}", e);
        }
    }

    // This produces access violation (running on windows)
    println!("Attempting second deserialization...");
    match facet_json::from_str::<Root>(json) {
        Ok(_) => println!("Second deserialization succeeded"),
        Err(e) => println!("Second deserialization failed: {:?}", e),
    }
}
