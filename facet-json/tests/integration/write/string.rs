#![cfg(feature = "std")]

use facet::Facet;

#[test]
fn test_static_strings() {
    facet_testhelpers::setup();

    #[derive(Debug, PartialEq, Clone, Facet)]
    struct StaticFoo {
        foo: &'static str,
    }

    let test_struct = StaticFoo { foo: "foo" };

    let json = facet_json::to_string(&test_struct);
    assert_eq!(json, r#"{"foo":"foo"}"#);

    #[derive(Debug, PartialEq, Clone, Facet)]
    struct OptStaticFoo {
        foo: Option<&'static str>,
    }

    let test_struct = OptStaticFoo { foo: None };

    let json = facet_json::to_string(&test_struct);
    assert_eq!(json, r#"{"foo":null}"#);

    let test_struct = OptStaticFoo { foo: Some("foo") };

    let json = facet_json::to_string(&test_struct);
    assert_eq!(json, r#"{"foo":"foo"}"#);

    #[derive(Debug, PartialEq, Clone, Facet)]
    struct CowFoo {
        foo: std::borrow::Cow<'static, str>,
    }

    let test_struct = CowFoo {
        foo: std::borrow::Cow::from("foo"),
    };

    let json = facet_json::to_string(&test_struct);
    assert_eq!(json, r#"{"foo":"foo"}"#);
}
