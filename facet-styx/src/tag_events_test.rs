//! Tests for tag event sequences as defined in docs/007-tag-events.md

use facet_format::{ContainerKind, FormatParser, ParseEvent, ParseEventKind, ScalarValue};
use facet_testhelpers::test;

use crate::StyxParser;

fn collect_events(input: &str) -> String {
    let mut parser = StyxParser::new(input);
    let mut lines = Vec::new();
    let mut indent: usize = 0;

    // Skip root StructStart
    let _ = parser.next_event();

    loop {
        match parser.next_event() {
            Ok(Some(event)) => {
                // Decrease indent before End events
                if matches!(
                    event.kind,
                    ParseEventKind::StructEnd | ParseEventKind::SequenceEnd
                ) {
                    indent = indent.saturating_sub(1);
                }

                let prefix = "  ".repeat(indent);
                lines.push(format!("{}{}", prefix, format_event(&event)));

                // Increase indent after Start events
                if matches!(
                    event.kind,
                    ParseEventKind::StructStart(_) | ParseEventKind::SequenceStart(_)
                ) {
                    indent += 1;
                }
            }
            Ok(None) => break,
            Err(e) => {
                lines.push(format!("Error: {:?}", e));
                break;
            }
        }
    }

    // Remove the final StructEnd (root object end)
    if lines.last().map(|s| s.trim()) == Some("StructEnd") {
        lines.pop();
    }

    lines.join("\n")
}

fn format_event(event: &ParseEvent) -> String {
    match &event.kind {
        ParseEventKind::Scalar(ScalarValue::Unit) => "Scalar(Unit)".to_string(),
        ParseEventKind::Scalar(ScalarValue::Null) => "Scalar(Unit)".to_string(),
        ParseEventKind::Scalar(ScalarValue::I64(n)) => format!("Scalar({})", n),
        ParseEventKind::Scalar(ScalarValue::Str(s)) => format!("Scalar({:?})", s.as_ref()),
        ParseEventKind::Scalar(s) => format!("Scalar({:?})", s),
        ParseEventKind::VariantTag(Some(name)) => format!("VariantTag({:?})", name),
        ParseEventKind::VariantTag(None) => "VariantTag(None)".to_string(),
        ParseEventKind::StructStart(ContainerKind::Object) => "StructStart".to_string(),
        ParseEventKind::StructStart(k) => format!("StructStart({:?})", k),
        ParseEventKind::StructEnd => "StructEnd".to_string(),
        ParseEventKind::SequenceStart(_) => "SequenceStart".to_string(),
        ParseEventKind::SequenceEnd => "SequenceEnd".to_string(),
        ParseEventKind::FieldKey(k) => format!("FieldKey({:?})", k.name.as_ref()),
        other => format!("{:?}", other),
    }
}

#[test]
fn test_01_bare_at() {
    // @ is a unit tag (no name) with implicit unit payload
    insta::assert_snapshot!(collect_events("x @"));
}

#[test]
fn test_02_unit_tag() {
    // @Foo -> VariantTag("Foo"), Scalar(Unit)
    insta::assert_snapshot!(collect_events("x @Foo"));
}

#[test]
fn test_03_tag_explicit_null() {
    // @Foo@ -> VariantTag("Foo"), Scalar(Unit)
    insta::assert_snapshot!(collect_events("x @Foo@"));
}

#[test]
fn test_04_tag_sequence() {
    // @Foo(a b) -> VariantTag("Foo"), SequenceStart, Scalar("a"), Scalar("b"), SequenceEnd
    insta::assert_snapshot!(collect_events("x @Foo(a b)"));
}

#[test]
fn test_05_tag_struct() {
    // @Foo{x 1} -> VariantTag("Foo"), StructStart, FieldKey("x"), Scalar(1), StructEnd
    insta::assert_snapshot!(collect_events("y @Foo{x 1}"));
}

#[test]
fn test_06_nested_unit_tags() {
    // @Foo(@Bar) -> VariantTag("Foo"), SequenceStart, VariantTag("Bar"), Scalar(Unit), SequenceEnd
    insta::assert_snapshot!(collect_events("x @Foo(@Bar)"));
}

#[test]
fn test_07_tag_struct_with_tag_value() {
    // @Foo{x @Bar} -> VariantTag("Foo"), StructStart, FieldKey("x"), VariantTag("Bar"), Scalar(Unit), StructEnd
    insta::assert_snapshot!(collect_events("y @Foo{x @Bar}"));
}

#[test]
fn test_08_field_unit_tag() {
    // x @Foo -> FieldKey("x"), VariantTag("Foo"), Scalar(Unit)
    insta::assert_snapshot!(collect_events("x @Foo"));
}

#[test]
fn test_09_field_struct_tag() {
    // x @Foo{y 1} -> FieldKey("x"), VariantTag("Foo"), StructStart, FieldKey("y"), Scalar(1), StructEnd
    insta::assert_snapshot!(collect_events("x @Foo{y 1}"));
}

#[test]
fn test_10_sequence_unit_tags() {
    // (@Foo @Bar) -> SequenceStart, VariantTag("Foo"), Scalar(Unit), VariantTag("Bar"), Scalar(Unit), SequenceEnd
    insta::assert_snapshot!(collect_events("x (@Foo @Bar)"));
}

#[test]
fn test_11_deeply_nested() {
    // @Foo(@Bar{x 1}) -> VariantTag("Foo"), SequenceStart, VariantTag("Bar"), StructStart, FieldKey("x"), Scalar(1), StructEnd, SequenceEnd
    insta::assert_snapshot!(collect_events("y @Foo(@Bar{x 1})"));
}

#[test]
fn test_12_at_as_key() {
    // @ {x 1} -> FieldKey("@"), StructStart, FieldKey("x"), Scalar(1), StructEnd
    insta::assert_snapshot!(collect_events("@ {x 1}"));
}
