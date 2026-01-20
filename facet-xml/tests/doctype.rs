//! Tests for XML DOCTYPE declaration in facet-xml.

use facet::Facet;

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
