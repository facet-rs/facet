#![cfg(feature = "std")]

use facet::Facet;
use facet_json2::to_string;

#[test]
fn test_strings() {
    facet_testhelpers::setup();

    #[derive(Debug, PartialEq, Clone, Facet)]
    struct StaticFoo<'a> {
        foo: &'a str,
    }

    let test_struct = StaticFoo { foo: "foo" };

    let json = to_string(&test_struct);
    assert_eq!(json, r#"{"foo":"foo"}"#);

    #[derive(Debug, PartialEq, Clone, Facet)]
    struct OptStaticFoo<'a> {
        foo: Option<&'a str>,
    }

    let test_struct = OptStaticFoo { foo: None };

    let json = to_string(&test_struct);
    assert_eq!(json, r#"{"foo":null}"#);

    let test_struct = OptStaticFoo { foo: Some("foo") };

    let json = to_string(&test_struct);
    assert_eq!(json, r#"{"foo":"foo"}"#);

    #[derive(Debug, PartialEq, Clone, Facet)]
    struct CowFoo<'a> {
        foo: std::borrow::Cow<'a, str>,
    }

    let test_struct = CowFoo {
        foo: std::borrow::Cow::from("foo"),
    };

    let json = to_string(&test_struct);
    assert_eq!(json, r#"{"foo":"foo"}"#);
}
