use std::num::NonZero;

use facet_derive::Facet;
use facet_json_write::{to_json, to_json_string};
use facet_poke::Peek;

use facet_core as facet;

#[ctor::ctor]
fn init() {
    // Initialize color backtrace for pretty stack traces
    color_backtrace::install();

    // Initialize logger to print all log levels
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("trace"))
        .format_timestamp(None)
        .init();

    log::info!("Logging and color backtrace initialized");
}

#[test]
fn test_to_json() {
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
    #[derive(Debug, PartialEq, Clone, Facet)]
    struct StaticFoo {
        foo: &'static str,
    }

    let test_struct = StaticFoo {
        foo: "foo",
    };

    let peek = Peek::new(&test_struct);
    let json = to_json_string(peek, false);
    assert_eq!(json, r#"{"foo":"foo"}"#);

    #[derive(Debug, PartialEq, Clone, Facet)]
    struct OptStaticFoo {
        foo: Option<&'static str>,
    }

    let test_struct = OptStaticFoo {
        foo: None,
    };

    let peek = Peek::new(&test_struct);
    let json = to_json_string(peek, false);

    assert_eq!(json, r#"{"foo":null}"#);

    let test_struct = OptStaticFoo {
        foo: Some("foo"),
    };

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
