use facet::Facet;
use facet_xml_legacy as xml;

#[derive(Facet, Debug, PartialEq)]
#[repr(C)]
enum Shape {
    #[facet(rename = "circle")]
    Circle(Circle),
    #[facet(rename = "rect")]
    Rect(Rect),
}

#[derive(Facet, Debug, PartialEq)]
struct Circle {
    #[facet(default, xml::attribute)]
    cx: Option<String>,
    #[facet(default, xml::attribute)]
    cy: Option<String>,
}

#[derive(Facet, Debug, PartialEq)]
struct Rect {
    #[facet(default, xml::attribute)]
    x: Option<String>,
    #[facet(default, xml::attribute)]
    y: Option<String>,
}

#[derive(Facet, Debug, PartialEq)]
struct Container {
    #[facet(xml::elements)]
    shapes: Vec<Shape>,
}

#[test]
fn test_simple_enum() {
    let xml = r#"<circle cx="10" cy="20"/>"#;
    let shape: Shape = xml::from_str(xml).unwrap();
    assert!(matches!(shape, Shape::Circle(_)));
}

#[test]
fn test_enum_in_list() {
    let xml = r#"<Container>
        <circle cx="10" cy="20"/>
        <rect x="5" y="15"/>
    </Container>"#;
    let container: Container = xml::from_str(xml).unwrap();
    assert_eq!(container.shapes.len(), 2);
}
