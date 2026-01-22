//! Tests for deserializing value expressions like those in dibs.
//!
//! We want to capture:
//! - Bare scalars: `$name` → tag=None, payload=Scalar("$name")
//! - Nullary functions: `@now` → tag=Some("now"), payload=Unit
//! - Functions with args: `@coalesce($a $b)` → tag=Some("coalesce"), payload=Seq

use facet::Facet;
use facet_testhelpers::test;

use crate::from_str;

/// The payload of a value expression - can be scalar or sequence.
/// Unit payloads are represented as None in Option<Payload>.
#[derive(Facet, Debug, PartialEq)]
#[facet(untagged)]
#[repr(u8)]
pub enum Payload {
    /// Scalar payload (for bare values like $name)
    Scalar(String),
    /// Sequence payload (for functions with args like @coalesce($a $b))
    Seq(Vec<ValueExpr>),
}

/// A value expression - either @default, a function call, or a bare scalar.
#[derive(Facet, Debug, PartialEq)]
#[facet(rename_all = "lowercase")]
#[repr(u8)]
pub enum ValueExpr {
    /// The @default keyword
    Default,
    /// Everything else: functions and bare scalars
    #[facet(other)]
    Other {
        #[facet(tag)]
        tag: Option<String>,
        #[facet(content)]
        content: Option<Payload>,
    },
}

/// Wrapper for testing - styx docs are implicitly objects
#[derive(Facet, Debug, PartialEq)]
struct Doc {
    v: ValueExpr,
}

#[test]
fn test_bare_scalar_no_hash() {
    // v $name -> bare scalar, no tag
    let input = r#"v name"#;
    let result: Doc = from_str(input).unwrap();
    assert_eq!(
        result.v,
        ValueExpr::Other {
            tag: None,
            content: Some(Payload::Scalar("name".into())),
        }
    );
}

#[test]
fn test_bare_scalar_hash() {
    // v $name -> bare scalar, no tag
    let input = r#"v $name"#;
    let result: Doc = from_str(input).unwrap();
    assert_eq!(
        result.v,
        ValueExpr::Other {
            tag: None,
            content: Some(Payload::Scalar("$name".into())),
        }
    );
}

#[test]
fn test_bare_scalar_quoted() {
    // v "hello world" -> bare scalar, no tag
    let input = r#"v "hello world""#;
    let result: Doc = from_str(input).unwrap();
    assert_eq!(
        result.v,
        ValueExpr::Other {
            tag: None,
            content: Some(Payload::Scalar("hello world".into())),
        }
    );
}

#[test]
fn test_bare_scalar_number() {
    // v 123 -> bare scalar, no tag
    let input = r#"v 123"#;
    let result: Doc = from_str(input).unwrap();
    // Numbers are parsed as strings in this context
    assert_eq!(
        result.v,
        ValueExpr::Other {
            tag: None,
            content: Some(Payload::Scalar("123".into())),
        }
    );
}

#[test]
fn test_default_tag() {
    // v @default -> known tag
    let input = r#"v @default"#;
    let result: Doc = from_str(input).unwrap();
    assert_eq!(result.v, ValueExpr::Default);
}

#[test]
fn test_nullary_function() {
    // v @now -> tag=now, payload=unit (None)
    let input = r#"v @now"#;
    let result: Doc = from_str(input).unwrap();
    assert_eq!(
        result.v,
        ValueExpr::Other {
            tag: Some("now".into()),
            content: None,
        }
    );
}

#[test]
fn test_function_with_args() {
    // v @coalesce($a $b) -> tag=coalesce, payload=seq
    let input = r#"v @coalesce($a $b)"#;
    let result: Doc = from_str(input).unwrap();
    assert_eq!(
        result.v,
        ValueExpr::Other {
            tag: Some("coalesce".into()),
            content: Some(Payload::Seq(vec![
                ValueExpr::Other {
                    tag: None,
                    content: Some(Payload::Scalar("$a".into())),
                },
                ValueExpr::Other {
                    tag: None,
                    content: Some(Payload::Scalar("$b".into())),
                },
            ])),
        }
    );
}

#[test]
fn test_nested_function() {
    // v @lower(@concat($a $b)) -> nested function calls
    let input = r#"v @lower(@concat($a $b))"#;
    let result: Doc = from_str(input).unwrap();
    assert_eq!(
        result.v,
        ValueExpr::Other {
            tag: Some("lower".into()),
            content: Some(Payload::Seq(vec![ValueExpr::Other {
                tag: Some("concat".into()),
                content: Some(Payload::Seq(vec![
                    ValueExpr::Other {
                        tag: None,
                        content: Some(Payload::Scalar("$a".into())),
                    },
                    ValueExpr::Other {
                        tag: None,
                        content: Some(Payload::Scalar("$b".into())),
                    },
                ])),
            }])),
        }
    );
}
