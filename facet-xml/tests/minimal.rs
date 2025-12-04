use facet::Facet;
use facet_xml as xml;

#[derive(Facet, Debug, PartialEq)]
struct Test1 {
    #[facet(xml::attribute)]
    required: String,
}

#[derive(Facet, Debug, PartialEq)]
struct Test2 {
    #[facet(xml::attribute)]
    required: String,
    #[facet(default, xml::attribute)]
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
