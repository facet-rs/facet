use facet::Facet;
use facet_format::FormatDeserializer;
use facet_json::JsonParser;

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
    let json = br#"{"radius": 5.0, "type": "Circle"}"#;
    let mut parser = JsonParser::<false>::new(json);
    let mut de = FormatDeserializer::new_owned(&mut parser);
    let result: InternallyTagged = de.deserialize_root().expect("should deserialize");
    assert_eq!(result, InternallyTagged::Circle { radius: 5.0 });
}

#[test]
fn test_adjacently_tagged_out_of_order() {
    // Content comes BEFORE the tag - this should work now!
    let json = br#"{"c": "hello", "t": "Message"}"#;
    let mut parser = JsonParser::<false>::new(json);
    let mut de = FormatDeserializer::new_owned(&mut parser);
    let result: AdjacentlyTagged = de.deserialize_root().expect("should deserialize");
    assert_eq!(result, AdjacentlyTagged::Message("hello".into()));
}
