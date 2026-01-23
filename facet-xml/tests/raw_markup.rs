use facet::Facet;
use facet_xml::{RawMarkup, from_str};

#[derive(Facet, Debug, PartialEq)]
struct Document {
    title: String,
    body: RawMarkup,
}

#[test]
fn raw_markup_captures_element() {
    let xml =
        r#"<document><title>Hello</title><body><p>Some <b>bold</b> text</p></body></document>"#;
    let doc: Document = from_str(xml).unwrap();

    assert_eq!(doc.title, "Hello");
    assert_eq!(
        doc.body.as_str(),
        "<body><p>Some <b>bold</b> text</p></body>"
    );
}

#[test]
fn raw_markup_captures_with_attributes() {
    let xml =
        r#"<document><title>Test</title><body class="content"><span>text</span></body></document>"#;
    let doc: Document = from_str(xml).unwrap();

    assert_eq!(doc.title, "Test");
    assert_eq!(
        doc.body.as_str(),
        r#"<body class="content"><span>text</span></body>"#
    );
}

#[test]
fn raw_markup_captures_empty_element() {
    let xml = r#"<document><title>Empty</title><body/></document>"#;
    let doc: Document = from_str(xml).unwrap();

    assert_eq!(doc.title, "Empty");
    assert_eq!(doc.body.as_str(), "<body/>");
}
