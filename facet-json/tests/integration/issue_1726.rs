//! Test for https://github.com/facet-rs/facet/issues/1726
//!
//! Transparent struct with inner boxed enum fails to serialize

#![allow(unused)]

use facet::Facet;
use facet_testhelpers::test;

#[derive(Facet)]
#[facet(transparent)]
struct Value(Box<Inner>); // Works without Box

impl Value {
    fn new(inner: Inner) -> Self {
        Self(Box::new(inner))
    }

    fn number(value: i64) -> Self {
        Self::new(Inner::Number(value))
    }

    fn list(value: Vec<Value>) -> Self {
        Self::new(Inner::List(value))
    }
}

#[derive(Facet)]
#[repr(u8)]
enum Inner {
    Number(i64),
    List(Vec<Value>),
}

fn repro() -> Value {
    Value::list(vec![
        Value::number(10),
        Value::number(20),
        Value::number(30),
    ])
}

#[test]
fn test_repro() {
    let json = facet_json::to_string_pretty(&repro()).unwrap();
    // The list variant with 3 numbers should serialize as expected
    assert_eq!(
        json,
        r#"{
  "List": [
    {
      "Number": 10
    },
    {
      "Number": 20
    },
    {
      "Number": 30
    }
  ]
}"#
    );
}
