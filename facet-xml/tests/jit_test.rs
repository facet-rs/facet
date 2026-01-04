//! Test JIT with XML parser

use facet::Facet;
use facet_format::FormatParser;
use facet_format::jit;
use facet_xml::{XmlParser, to_vec};

#[derive(Facet, Debug, PartialEq)]
struct SimpleRecord {
    id: u64,
    name: String,
}

#[test]
fn test_jit_compatibility() {
    assert!(jit::is_jit_compatible::<SimpleRecord>());
}

#[test]
fn test_xml_events() {
    // First, let's see what events the XML parser actually produces
    let data = SimpleRecord {
        id: 42,
        name: "test".to_string(),
    };
    let xml = to_vec(&data).unwrap();
    eprintln!("XML: {}", String::from_utf8_lossy(&xml));

    let mut parser = XmlParser::new(&xml);
    eprintln!("Events from XML parser:");
    for i in 0..20 {
        match parser.next_event() {
            Ok(event) => {
                eprintln!("  {}: {:?}", i, event);
            }
            Err(e) => {
                eprintln!("  {}: Error: {:?}", i, e);
                break;
            }
        }
    }
}

#[test]
#[ignore = "JIT does not validate scalar tags, reads garbage on type mismatch - see #1642"]
fn test_jit_xml_deserialize() {
    let data = SimpleRecord {
        id: 42,
        name: "test".to_string(),
    };
    let xml = to_vec(&data).unwrap();
    eprintln!("XML: {}", String::from_utf8_lossy(&xml));

    let parser = XmlParser::new(&xml);
    let result = jit::deserialize_with_fallback::<SimpleRecord, _>(parser);

    eprintln!("Result: {:?}", result);

    let parsed = result.unwrap();
    assert_eq!(parsed.id, 42);
    assert_eq!(parsed.name, "test");
}
