use facet::Facet;
use facet_format::FormatDeserializer;
use facet_xml::XmlParser;

#[derive(Debug, PartialEq, Facet)]
#[facet(tag = "type")]
#[repr(u8)]
pub enum InternallyTagged {
    Circle { radius: f64 },
}

#[derive(Debug, PartialEq, Facet)]
#[facet(tag = "t", content = "c")]
#[repr(u8)]
pub enum AdjacentlyTagged {
    Message(String),
}

#[test]
fn test_internally_tagged_out_of_order() {
    // Tag comes AFTER the other field - this should work now!
    let xml = br#"<shape><radius>5.0</radius><type>Circle</type></shape>"#;
    let parser = XmlParser::new(xml);
    let mut de = FormatDeserializer::new_owned(parser);
    let result: InternallyTagged = de.deserialize_root().expect("should deserialize");
    assert_eq!(result, InternallyTagged::Circle { radius: 5.0 });
}

#[test]
fn test_adjacently_tagged_out_of_order() {
    // Content comes BEFORE the tag - this should work now!
    let xml = br#"<value><c>hello</c><t>Message</t></value>"#;
    let parser = XmlParser::new(xml);
    let mut de = FormatDeserializer::new_owned(parser);
    let result: AdjacentlyTagged = de.deserialize_root().expect("should deserialize");
    assert_eq!(result, AdjacentlyTagged::Message("hello".into()));
}
