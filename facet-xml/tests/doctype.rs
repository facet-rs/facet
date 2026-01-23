//! Tests for XML DOCTYPE declaration in facet-xml.

use facet::Facet;
use facet_xml as xml;

#[test]
fn can_read_with_doctype() {
    #[derive(Facet, Debug)]
    struct Orange {
        taste: String,
    }

    let xml = "<?xml version=\"1.0\" encoding=\"UTF-8\"?><!DOCTYPE orange SYSTEM \"orange.dtd\">\n<orange><taste>sweeter</taste></orange>";
    let result: Orange = facet_xml::from_str(xml).unwrap();
    assert_eq!(result.taste, "sweeter");
}

#[test]
fn doctype_roundtrip() {
    #[derive(Facet, Debug, PartialEq)]
    struct Document {
        #[facet(xml::doctype)]
        doctype: Option<String>,
        content: String,
    }

    let doc = Document {
        doctype: Some("document SYSTEM \"document.dtd\"".to_string()),
        content: "Hello, World!".to_string(),
    };

    let xml = facet_xml::to_string(&doc).unwrap();
    assert!(xml.contains("<!DOCTYPE document SYSTEM \"document.dtd\">"));
    assert!(xml.contains("<content>Hello, World!</content>"));

    let parsed: Document = facet_xml::from_str(&xml).unwrap();
    assert_eq!(
        parsed.doctype,
        Some("document SYSTEM \"document.dtd\"".to_string())
    );
    assert_eq!(parsed.content, "Hello, World!");
}

#[test]
fn doctype_optional_none() {
    #[derive(Facet, Debug, PartialEq)]
    struct Document {
        #[facet(xml::doctype)]
        doctype: Option<String>,
        content: String,
    }

    let doc = Document {
        doctype: None,
        content: "No DOCTYPE here".to_string(),
    };

    let xml = facet_xml::to_string(&doc).unwrap();
    assert!(!xml.contains("<!DOCTYPE"));
    assert!(xml.contains("<content>No DOCTYPE here</content>"));

    let parsed: Document = facet_xml::from_str(&xml).unwrap();
    assert_eq!(parsed.doctype, None);
    assert_eq!(parsed.content, "No DOCTYPE here");
}

#[test]
fn doctype_with_public_id() {
    #[derive(Facet, Debug, PartialEq)]
    struct HtmlDoc {
        #[facet(xml::doctype)]
        doctype: Option<String>,
        title: String,
    }

    let doc = HtmlDoc {
        doctype: Some("html PUBLIC \"-//W3C//DTD XHTML 1.0 Strict//EN\" \"http://www.w3.org/TR/xhtml1/DTD/xhtml1-strict.dtd\"".to_string()),
        title: "My Page".to_string(),
    };

    let xml = facet_xml::to_string_pretty(&doc).unwrap();
    assert!(xml.contains("<!DOCTYPE html PUBLIC"));

    let parsed: HtmlDoc = facet_xml::from_str(&xml).unwrap();
    assert_eq!(parsed.doctype, doc.doctype);
    assert_eq!(parsed.title, "My Page");
}
