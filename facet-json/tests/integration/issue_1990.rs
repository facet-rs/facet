//! Regression tests for <https://github.com/facet-rs/facet/issues/1990>
//!
//! Untagged enum variant selection should prefer the best type match,
//! not declaration order, when variants share the same field names.

use facet::Facet;

#[derive(Debug, Facet, PartialEq)]
#[facet(untagged)]
#[repr(C)]
enum StringFirst {
    Stringer { x: String, y: String },
    Integer { x: i32, y: i32 },
}

#[derive(Debug, Facet, PartialEq)]
#[facet(untagged)]
#[repr(C)]
enum IntegerFirst {
    Integer { x: i32, y: i32 },
    Stringer { x: String, y: String },
}

#[test]
fn number_input_selects_integer_variant() {
    let parsed: StringFirst = facet_json::from_str(r#"{"x":1,"y":2}"#).unwrap();
    assert_eq!(parsed, StringFirst::Integer { x: 1, y: 2 });
}

#[test]
fn numeric_strings_select_string_variant() {
    let parsed: IntegerFirst = facet_json::from_str(r#"{"x":"1","y":"2"}"#).unwrap();
    assert_eq!(
        parsed,
        IntegerFirst::Stringer {
            x: "1".into(),
            y: "2".into(),
        }
    );
}

#[test]
fn non_numeric_strings_still_select_string_variant() {
    let parsed: IntegerFirst = facet_json::from_str(r#"{"x":"abc","y":"def"}"#).unwrap();
    assert_eq!(
        parsed,
        IntegerFirst::Stringer {
            x: "abc".into(),
            y: "def".into(),
        }
    );
}
