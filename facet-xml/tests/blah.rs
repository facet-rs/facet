//! Test to verify serde-xml-rs behavior for lists and maps

use serde::{Deserialize, Serialize};
use std::collections::HashMap;

#[derive(Debug, Serialize, Deserialize, PartialEq)]
#[serde(rename = "record")]
struct RecordWithList {
    #[serde(rename = "item")]
    items: Vec<String>,
}

#[derive(Debug, Serialize, Deserialize, PartialEq)]
#[serde(rename = "record")]
struct RecordWithMap {
    data: HashMap<String, u32>,
}

#[derive(Debug, Serialize, Deserialize, PartialEq)]
#[serde(rename = "record")]
struct RecordWithFlattenedMap {
    #[serde(flatten)]
    data: HashMap<String, u32>,
}

#[test]
fn test_list_wrapped() {
    // Try wrapped format
    let xml = r#"<record><items><item>alpha</item><item>beta</item></items></record>"#;
    let result: Result<RecordWithList, _> = serde_xml_rs::from_str(xml);
    eprintln!("list wrapped: {:?}", result);
}

#[test]
fn test_list_flat() {
    // Try flat format
    let xml = r#"<record><item>alpha</item><item>beta</item></record>"#;
    let result: Result<RecordWithList, _> = serde_xml_rs::from_str(xml);
    eprintln!("list flat: {:?}", result);
}

#[test]
fn test_map_wrapped() {
    // Try wrapped format
    let xml = r#"<record><data><alpha>1</alpha><beta>2</beta></data></record>"#;
    let result: Result<RecordWithMap, _> = serde_xml_rs::from_str(xml);
    eprintln!("map wrapped: {:?}", result);
}

#[test]
fn test_map_flat() {
    // Try flat format (if maps even work)
    let xml = r#"<record><alpha>1</alpha><beta>2</beta></record>"#;
    let result: Result<RecordWithMap, _> = serde_xml_rs::from_str(xml);
    eprintln!("map flat: {:?}", result);
}

#[test]
fn test_serialize_list() {
    let record = RecordWithList {
        items: vec!["alpha".into(), "beta".into()],
    };
    let xml = serde_xml_rs::to_string(&record);
    eprintln!("serialized list: {:?}", xml);
}

#[test]
fn test_serialize_map() {
    let mut data = HashMap::new();
    data.insert("alpha".into(), 1);
    data.insert("beta".into(), 2);
    let record = RecordWithMap { data };
    let xml = serde_xml_rs::to_string(&record);
    eprintln!("serialized map: {:?}", xml);
}

#[test]
fn test_flattened_map_deserialize() {
    // Try flat format with #[serde(flatten)]
    let xml = r#"<record><alpha>1</alpha><beta>2</beta></record>"#;
    let result: Result<RecordWithFlattenedMap, _> = serde_xml_rs::from_str(xml);
    eprintln!("flattened map deserialize: {:?}", result);
}

#[test]
fn test_flattened_map_serialize() {
    let mut data = HashMap::new();
    data.insert("alpha".into(), 1);
    data.insert("beta".into(), 2);
    let record = RecordWithFlattenedMap { data };
    let xml = serde_xml_rs::to_string(&record);
    eprintln!("flattened map serialize: {:?}", xml);
}
