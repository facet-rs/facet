use std::io::Write;

use facet::Facet;
use facet_xml_legacy as xml;

#[derive(Facet, Debug, PartialEq)]
struct Test1 {
    #[facet(xml::attribute)]
    required: String,
}

#[derive(Facet, Debug, PartialEq)]
struct Test2 {
    #[facet(xml::attribute)]
    required: String,
    #[facet(xml::attribute)]
    optional: Option<String>,
}

#[test]
fn test_basic_required() {
    let xml = r#"<Test1 required="hello"/>"#;
    let result: Test1 = xml::from_str(xml).unwrap();
    assert_eq!(result.required, "hello");
}

#[test]
fn test_optional_present() {
    let xml = r#"<Test2 required="hello" optional="world"/>"#;
    let result: Test2 = xml::from_str(xml).unwrap();
    assert_eq!(result.required, "hello");
    assert_eq!(result.optional, Some("world".to_string()));
}

#[test]
fn test_optional_absent() {
    let xml = r#"<Test2 required="hello"/>"#;
    let result: Test2 = xml::from_str(xml).unwrap();
    assert_eq!(result.required, "hello");
    assert_eq!(result.optional, None);
}

#[derive(Facet, Debug, PartialEq)]
struct Test3 {
    #[facet(xml::element)]
    required: String,
    #[facet(xml::element)]
    maybe: Option<u32>,
}

#[test]
fn test_optional_element_absent() {
    let xml = r#"<Test3><required>hi</required></Test3>"#;
    let parsed: Test3 = xml::from_str(xml).unwrap();
    assert_eq!(parsed.required, "hi");
    assert_eq!(parsed.maybe, None);
}

// ============================================================================
// Pretty-printing tests
// ============================================================================

#[derive(Facet, Debug, PartialEq)]
struct Person {
    #[facet(xml::attribute)]
    id: u32,
    #[facet(xml::element)]
    name: String,
    #[facet(xml::element)]
    age: u32,
}

#[test]
fn test_to_string_compact() {
    let person = Person {
        id: 42,
        name: "Alice".to_string(),
        age: 30,
    };
    let xml_output = xml::to_string(&person).unwrap();
    // Compact output: no newlines or indentation
    assert_eq!(
        xml_output,
        r#"<Person id="42"><name>Alice</name><age>30</age></Person>"#
    );
}

#[test]
fn test_to_string_pretty() {
    let person = Person {
        id: 42,
        name: "Alice".to_string(),
        age: 30,
    };
    let xml_output = xml::to_string_pretty(&person).unwrap();
    // Pretty output: newlines and default indentation (2 spaces)
    let expected = r#"<Person id="42">
  <name>Alice</name>
  <age>30</age>
</Person>"#;
    assert_eq!(xml_output, expected);
}

#[test]
fn test_to_string_with_options_custom_indent() {
    let person = Person {
        id: 42,
        name: "Alice".to_string(),
        age: 30,
    };
    let xml_output =
        xml::to_string_with_options(&person, &xml::SerializeOptions::default().indent("\t"))
            .unwrap();
    // Pretty output with tabs
    let expected = "<Person id=\"42\">\n\t<name>Alice</name>\n\t<age>30</age>\n</Person>";
    assert_eq!(xml_output, expected);
}

#[derive(Facet, Debug, PartialEq)]
struct Nested {
    #[facet(xml::element)]
    person: Person,
}

#[test]
fn test_pretty_nested_elements() {
    let nested = Nested {
        person: Person {
            id: 1,
            name: "Bob".to_string(),
            age: 25,
        },
    };
    let xml_output = xml::to_string_pretty(&nested).unwrap();
    let expected = r#"<Nested>
  <person id="1">
    <name>Bob</name>
    <age>25</age>
  </person>
</Nested>"#;
    assert_eq!(xml_output, expected);
}

#[test]
fn test_pretty_roundtrip() {
    let person = Person {
        id: 42,
        name: "Alice".to_string(),
        age: 30,
    };
    // Pretty-print, then parse back
    let xml_output = xml::to_string_pretty(&person).unwrap();
    let parsed: Person = xml::from_str(&xml_output).unwrap();
    assert_eq!(parsed, person);
}

// ============================================================================
// Float formatter tests
// ============================================================================

#[derive(Facet, Debug, PartialEq)]
struct Point {
    #[facet(xml::attribute)]
    x: f64,
    #[facet(xml::attribute)]
    y: f64,
}

/// Custom float formatter that mimics C's %g behavior:
/// - 6 significant digits
/// - Trim trailing zeros
/// - Trim trailing decimal point
fn fmt_g(value: f64, w: &mut dyn Write) -> std::io::Result<()> {
    // Use 6 significant digits like %g
    let s = format!("{value:.6}");
    let s = s.trim_end_matches('0').trim_end_matches('.');
    write!(w, "{s}")
}

#[test]
fn test_float_formatter_attribute() {
    let point = Point { x: 1.5, y: 2.0 };
    let options = xml::SerializeOptions::new().float_formatter(fmt_g);
    let xml_output = xml::to_string_with_options(&point, &options).unwrap();
    // Without formatter: x="1.5" y="2" (default Display)
    // With formatter: x="1.5" y="2" (trimmed zeros)
    assert_eq!(xml_output, r#"<Point x="1.5" y="2"/>"#);
}

#[test]
fn test_float_formatter_long_decimal() {
    // This value has floating-point representation issues
    let point = Point {
        x: 38.160000000000004,
        y: 139.98337649086284,
    };
    let options = xml::SerializeOptions::new().float_formatter(fmt_g);
    let xml_output = xml::to_string_with_options(&point, &options).unwrap();
    // Should be nicely formatted, not the full precision mess
    assert_eq!(xml_output, r#"<Point x="38.16" y="139.983376"/>"#);
}

#[test]
fn test_float_formatter_default() {
    // Without a custom formatter, uses default Display
    let point = Point { x: 1.5, y: 2.0 };
    let xml_output = xml::to_string(&point).unwrap();
    // Default Display keeps the full representation
    assert_eq!(xml_output, r#"<Point x="1.5" y="2"/>"#);
}

#[derive(Facet, Debug, PartialEq)]
struct Circle {
    #[facet(xml::element)]
    radius: f64,
}

#[test]
fn test_float_formatter_element() {
    let circle = Circle {
        radius: 1.234_567_890_123_456,
    };
    let options = xml::SerializeOptions::new().float_formatter(fmt_g);
    let xml_output = xml::to_string_with_options(&circle, &options).unwrap();
    // Element content should also use the formatter
    assert_eq!(xml_output, r#"<Circle><radius>1.234568</radius></Circle>"#);
}

#[derive(Facet, Debug, PartialEq)]
struct PointF32 {
    #[facet(xml::attribute)]
    x: f32,
    #[facet(xml::attribute)]
    y: f32,
}

#[test]
fn test_float_formatter_f32() {
    // f32 values should be upcast to f64 and formatted
    let point = PointF32 { x: 1.5, y: 2.0 };
    let options = xml::SerializeOptions::new().float_formatter(fmt_g);
    let xml_output = xml::to_string_with_options(&point, &options).unwrap();
    assert_eq!(xml_output, r#"<PointF32 x="1.5" y="2"/>"#);
}
