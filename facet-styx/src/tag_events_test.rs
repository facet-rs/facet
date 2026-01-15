//! Tests for tag event sequences as defined in docs/007-tag-events.md

use facet_format::{ContainerKind, FormatParser, ParseEvent, ScalarValue};
use facet_testhelpers::test;

use crate::StyxParser;

fn collect_events(input: &str) -> Vec<String> {
    let mut parser = StyxParser::new(input);
    let mut events = Vec::new();

    // Skip root StructStart
    let _ = parser.next_event();

    loop {
        match parser.next_event() {
            Ok(Some(event)) => {
                events.push(format_event(&event));
            }
            Ok(None) => break,
            Err(e) => {
                events.push(format!("Error: {:?}", e));
                break;
            }
        }
    }

    // Remove the final StructEnd (root object end)
    if events.last().map(|s| s.as_str()) == Some("StructEnd") {
        events.pop();
    }

    events
}

fn format_event(event: &ParseEvent) -> String {
    match event {
        ParseEvent::Scalar(ScalarValue::Unit) => "Scalar(Unit)".to_string(),
        ParseEvent::Scalar(ScalarValue::Null) => "Scalar(Unit)".to_string(),
        ParseEvent::Scalar(ScalarValue::I64(n)) => format!("Scalar({})", n),
        ParseEvent::Scalar(ScalarValue::Str(s)) => format!("Scalar({:?})", s.as_ref()),
        ParseEvent::Scalar(s) => format!("Scalar({:?})", s),
        ParseEvent::VariantTag(name) => format!("VariantTag({:?})", name),
        ParseEvent::StructStart(ContainerKind::Object) => "StructStart".to_string(),
        ParseEvent::StructStart(k) => format!("StructStart({:?})", k),
        ParseEvent::StructEnd => "StructEnd".to_string(),
        ParseEvent::SequenceStart(_) => "SequenceStart".to_string(),
        ParseEvent::SequenceEnd => "SequenceEnd".to_string(),
        ParseEvent::FieldKey(k) => format!("FieldKey({:?})", k.name.as_ref()),
        other => format!("{:?}", other),
    }
}

#[test]
fn test_01_bare_at() {
    // @ -> Scalar(Unit)
    let events = collect_events("x @");
    assert_eq!(events, vec!["FieldKey(\"x\")", "Scalar(Unit)"]);
}

#[test]
fn test_02_unit_tag() {
    // @Foo -> VariantTag("Foo"), Scalar(Unit)
    let events = collect_events("x @Foo");
    assert_eq!(
        events,
        vec!["FieldKey(\"x\")", "VariantTag(\"Foo\")", "Scalar(Unit)"]
    );
}

#[test]
fn test_03_tag_explicit_null() {
    // @Foo@ -> VariantTag("Foo"), Scalar(Unit)
    let events = collect_events("x @Foo@");
    assert_eq!(
        events,
        vec!["FieldKey(\"x\")", "VariantTag(\"Foo\")", "Scalar(Unit)"]
    );
}

#[test]
fn test_04_tag_sequence() {
    // @Foo(a b) -> VariantTag("Foo"), SequenceStart, Scalar("a"), Scalar("b"), SequenceEnd
    let events = collect_events("x @Foo(a b)");
    assert_eq!(
        events,
        vec![
            "FieldKey(\"x\")",
            "VariantTag(\"Foo\")",
            "SequenceStart",
            "Scalar(\"a\")",
            "Scalar(\"b\")",
            "SequenceEnd"
        ]
    );
}

#[test]
fn test_05_tag_struct() {
    // @Foo{x 1} -> VariantTag("Foo"), StructStart, FieldKey("x"), Scalar(1), StructEnd
    let events = collect_events("y @Foo{x 1}");
    assert_eq!(
        events,
        vec![
            "FieldKey(\"y\")",
            "VariantTag(\"Foo\")",
            "StructStart",
            "FieldKey(\"x\")",
            "Scalar(1)",
            "StructEnd"
        ]
    );
}

#[test]
fn test_06_nested_unit_tags() {
    // @Foo(@Bar) -> VariantTag("Foo"), SequenceStart, VariantTag("Bar"), Scalar(Unit), SequenceEnd
    let events = collect_events("x @Foo(@Bar)");
    assert_eq!(
        events,
        vec![
            "FieldKey(\"x\")",
            "VariantTag(\"Foo\")",
            "SequenceStart",
            "VariantTag(\"Bar\")",
            "Scalar(Unit)",
            "SequenceEnd"
        ]
    );
}

#[test]
fn test_07_tag_struct_with_tag_value() {
    // @Foo{x @Bar} -> VariantTag("Foo"), StructStart, FieldKey("x"), VariantTag("Bar"), Scalar(Unit), StructEnd
    let events = collect_events("y @Foo{x @Bar}");
    assert_eq!(
        events,
        vec![
            "FieldKey(\"y\")",
            "VariantTag(\"Foo\")",
            "StructStart",
            "FieldKey(\"x\")",
            "VariantTag(\"Bar\")",
            "Scalar(Unit)",
            "StructEnd"
        ]
    );
}

#[test]
fn test_08_field_unit_tag() {
    // x @Foo -> FieldKey("x"), VariantTag("Foo"), Scalar(Unit)
    let events = collect_events("x @Foo");
    assert_eq!(
        events,
        vec!["FieldKey(\"x\")", "VariantTag(\"Foo\")", "Scalar(Unit)"]
    );
}

#[test]
fn test_09_field_struct_tag() {
    // x @Foo{y 1} -> FieldKey("x"), VariantTag("Foo"), StructStart, FieldKey("y"), Scalar(1), StructEnd
    let events = collect_events("x @Foo{y 1}");
    assert_eq!(
        events,
        vec![
            "FieldKey(\"x\")",
            "VariantTag(\"Foo\")",
            "StructStart",
            "FieldKey(\"y\")",
            "Scalar(1)",
            "StructEnd"
        ]
    );
}

#[test]
fn test_10_sequence_unit_tags() {
    // (@Foo @Bar) -> SequenceStart, VariantTag("Foo"), Scalar(Unit), VariantTag("Bar"), Scalar(Unit), SequenceEnd
    let events = collect_events("x (@Foo @Bar)");
    assert_eq!(
        events,
        vec![
            "FieldKey(\"x\")",
            "SequenceStart",
            "VariantTag(\"Foo\")",
            "Scalar(Unit)",
            "VariantTag(\"Bar\")",
            "Scalar(Unit)",
            "SequenceEnd"
        ]
    );
}

#[test]
fn test_11_deeply_nested() {
    // @Foo(@Bar{x 1}) -> VariantTag("Foo"), SequenceStart, VariantTag("Bar"), StructStart, FieldKey("x"), Scalar(1), StructEnd, SequenceEnd
    let events = collect_events("y @Foo(@Bar{x 1})");
    assert_eq!(
        events,
        vec![
            "FieldKey(\"y\")",
            "VariantTag(\"Foo\")",
            "SequenceStart",
            "VariantTag(\"Bar\")",
            "StructStart",
            "FieldKey(\"x\")",
            "Scalar(1)",
            "StructEnd",
            "SequenceEnd"
        ]
    );
}

#[test]
fn test_12_at_as_key() {
    // @ {x 1} -> FieldKey("@"), StructStart, FieldKey("x"), Scalar(1), StructEnd
    let events = collect_events("@ {x 1}");
    assert_eq!(
        events,
        vec![
            "FieldKey(\"@\")",
            "StructStart",
            "FieldKey(\"x\")",
            "Scalar(1)",
            "StructEnd"
        ]
    );
}
