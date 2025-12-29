//! Tests for error diagnostics with miette's GraphicalReportHandler.
//!
//! These tests verify that our errors produce nice, user-friendly output
//! with source code context, unicode box-drawing, and helpful labels.

use facet::Facet;
use facet_json_legacy::from_str;
use facet_testhelpers::test;
use miette::{GraphicalReportHandler, GraphicalTheme};

/// Render an error using miette's GraphicalReportHandler with unicode theme.
fn render_error(err: &dyn miette::Diagnostic) -> String {
    let mut buf = String::new();
    let handler = GraphicalReportHandler::new_themed(GraphicalTheme::unicode());
    handler.render_report(&mut buf, err).unwrap();
    buf
}

// ============================================================================
// Syntax Errors (during tokenization)
// ============================================================================

#[test]
fn syntax_error_unexpected_character() {
    let json = "x";
    let err = from_str::<i32>(json).unwrap_err();

    #[cfg(not(miri))]
    insta::assert_snapshot!(render_error(&err));
}

#[test]
fn syntax_error_unexpected_brace() {
    let json = "}";
    let err = from_str::<i32>(json).unwrap_err();

    #[cfg(not(miri))]
    insta::assert_snapshot!(render_error(&err));
}

#[test]
fn syntax_error_with_context() {
    let json = r#"{"name": "test", "value": @invalid}"#;
    #[derive(Facet, Debug)]
    struct Data {
        name: String,
        value: i32,
    }
    let err = from_str::<Data>(json).unwrap_err();

    #[cfg(not(miri))]
    insta::assert_snapshot!(render_error(&err));
}

#[test]
fn syntax_error_multiline() {
    let json = r#"{
  "name": "test",
  "value": ???
}"#;
    #[derive(Facet, Debug)]
    struct Data {
        name: String,
        value: i32,
    }
    let err = from_str::<Data>(json).unwrap_err();

    #[cfg(not(miri))]
    insta::assert_snapshot!(render_error(&err));
}

// ============================================================================
// Semantic Errors (after parsing, during deserialization)
// ============================================================================

#[test]
fn semantic_error_type_mismatch() {
    let json = r#"{"foo": 42, "bar": 123}"#;
    #[derive(Facet, Debug)]
    struct FooBar {
        foo: u64,
        bar: String,
    }
    let err = from_str::<FooBar>(json).unwrap_err();

    #[cfg(not(miri))]
    insta::assert_snapshot!(render_error(&err));
}

#[test]
fn semantic_error_unknown_field() {
    let json = r#"{"name": "test", "unknown_field": 42}"#;
    #[derive(Facet, Debug)]
    #[facet(deny_unknown_fields)]
    struct Data {
        name: String,
    }
    let err = from_str::<Data>(json).unwrap_err();

    #[cfg(not(miri))]
    insta::assert_snapshot!(render_error(&err));
}

#[test]
fn semantic_error_wrong_type_for_array() {
    let json = r#"{"items": "not an array"}"#;
    #[derive(Facet, Debug)]
    struct Container {
        items: Vec<i32>,
    }
    let err = from_str::<Container>(json).unwrap_err();

    #[cfg(not(miri))]
    insta::assert_snapshot!(render_error(&err));
}

#[test]
fn semantic_error_number_out_of_range() {
    let json = "999999999999999999999999999999";
    let err = from_str::<u32>(json).unwrap_err();

    #[cfg(not(miri))]
    insta::assert_snapshot!(render_error(&err));
}

#[test]
fn semantic_error_tuple_wrong_size() {
    let json = "[1, 2, 3]";
    let err = from_str::<(i32, i32)>(json).unwrap_err();

    #[cfg(not(miri))]
    insta::assert_snapshot!(render_error(&err));
}

#[test]
fn semantic_error_trailing_data() {
    let json = "42 extra stuff";
    let err = from_str::<i32>(json).unwrap_err();

    #[cfg(not(miri))]
    insta::assert_snapshot!(render_error(&err));
}

// ============================================================================
// Edge Cases
// ============================================================================

#[test]
fn error_at_start_of_input() {
    let json = "";
    let err = from_str::<i32>(json).unwrap_err();

    #[cfg(not(miri))]
    insta::assert_snapshot!(render_error(&err));
}

#[test]
fn error_at_end_of_long_input() {
    let json = r#"{"a": 1, "b": 2, "c": 3, "d": 4, "e": 5, "f": !}"#;
    #[derive(Facet, Debug)]
    struct Many {
        a: i32,
        b: i32,
        c: i32,
        d: i32,
        e: i32,
        f: i32,
    }
    let err = from_str::<Many>(json).unwrap_err();

    #[cfg(not(miri))]
    insta::assert_snapshot!(render_error(&err));
}

#[test]
fn error_with_unicode_content() {
    let json = r#"{"emoji": "🎉", "value": nope}"#;
    #[derive(Facet, Debug)]
    struct Emoji {
        emoji: String,
        value: i32,
    }
    let err = from_str::<Emoji>(json).unwrap_err();

    #[cfg(not(miri))]
    insta::assert_snapshot!(render_error(&err));
}
