use std::num::NonZero;

use facet::Facet;
use facet_json::{to_json, to_json_string};
use facet_reflect::Peek;

#[test]
fn test_to_json() {
    facet_testhelpers::setup();

    #[derive(Debug, PartialEq, Clone, Facet)]
    struct LinearFunction {
        variable: String,
        slope: f32,
        intercept: i32,
    }

    let test_struct = LinearFunction {
        variable: "x".to_string(),
        slope: -3.5,
        intercept: -5,
    };

    let expected_json = r#"{"variable":"x","slope":-3.5,"intercept":-5}"#;
    let expected_json_indented = r#"{
  "variable": "x",
  "slope": -3.5,
  "intercept": -5
}"#;

    for (indent, expected) in [(true, expected_json_indented), (false, expected_json)] {
        let mut buffer = Vec::new();
        let peek = Peek::new(&test_struct);
        to_json(peek, &mut buffer, indent).unwrap();
        let json = String::from_utf8(buffer).unwrap();
        assert_eq!(json, expected);
    }
}

#[test]
fn test_nonzero() {
    facet_testhelpers::setup();

    #[derive(Debug, PartialEq, Clone, Facet)]
    struct Foo {
        foo: NonZero<u8>,
    }

    let test_struct = Foo {
        foo: const { NonZero::new(1).unwrap() },
    };

    let peek = Peek::new(&test_struct);
    let json = to_json_string(peek, false);

    assert_eq!(json, r#"{"foo":1}"#);
}

#[test]
fn test_hashmap_to_json() {
    facet_testhelpers::setup();

    let mut json_data = std::collections::HashMap::<&str, &str>::new();
    json_data.insert("foo", "bar");

    let expected_json = r#"{"foo":"bar"}"#;
    let expected_json_indented = r#"{
  "foo": "bar"
}"#;

    for (indent, expected) in [(true, expected_json_indented), (false, expected_json)] {
        let mut buffer = Vec::new();
        let peek = Peek::new(&json_data);
        to_json(peek, &mut buffer, indent).unwrap();
        let json = String::from_utf8(buffer).unwrap();
        assert_eq!(json, expected);
    }
}

#[test]
fn test_static_strings() {
    facet_testhelpers::setup();

    #[derive(Debug, PartialEq, Clone, Facet)]
    struct StaticFoo {
        foo: &'static str,
    }

    let test_struct = StaticFoo { foo: "foo" };

    let peek = Peek::new(&test_struct);
    let json = to_json_string(peek, false);
    assert_eq!(json, r#"{"foo":"foo"}"#);

    #[derive(Debug, PartialEq, Clone, Facet)]
    struct OptStaticFoo {
        foo: Option<&'static str>,
    }

    let test_struct = OptStaticFoo { foo: None };

    let peek = Peek::new(&test_struct);
    let json = to_json_string(peek, false);

    assert_eq!(json, r#"{"foo":null}"#);

    let test_struct = OptStaticFoo { foo: Some("foo") };

    let peek = Peek::new(&test_struct);
    let json = to_json_string(peek, false);

    assert_eq!(json, r#"{"foo":"foo"}"#);

    #[derive(Debug, PartialEq, Clone, Facet)]
    struct CowFoo {
        foo: std::borrow::Cow<'static, str>,
    }

    let test_struct = CowFoo {
        foo: std::borrow::Cow::from("foo"),
    };

    let peek = Peek::new(&test_struct);
    let json = to_json_string(peek, false);

    assert_eq!(json, r#"{"foo":"foo"}"#);
}
