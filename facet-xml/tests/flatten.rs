//! Tests for flatten behavior in facet-xml.

use std::collections::HashMap;

use facet::Facet;
use facet_xml as xml;

// ============================================================================
// flatten - flattened structs
// ============================================================================

#[test]
fn flatten_struct_fields_appear_as_siblings() {
    #[derive(Facet, Debug, PartialEq)]
    struct Address {
        city: String,
        country: String,
    }

    #[derive(Facet, Debug, PartialEq)]
    struct Person {
        name: String,
        #[facet(flatten)]
        address: Address,
    }

    // Address fields appear directly under <person>, not nested in <address>
    let result: Person = facet_xml::from_str(
        "<person><name>Alice</name><city>Paris</city><country>France</country></person>",
    )
    .unwrap();
    assert_eq!(result.name, "Alice");
    assert_eq!(result.address.city, "Paris");
    assert_eq!(result.address.country, "France");
}

#[test]
fn flatten_struct_with_attributes() {
    #[derive(Facet, Debug, PartialEq)]
    struct CommonAttrs {
        #[facet(xml::attribute)]
        id: String,
        #[facet(xml::attribute)]
        class: Option<String>,
    }

    #[derive(Facet, Debug, PartialEq)]
    struct Element {
        #[facet(flatten)]
        attrs: CommonAttrs,
        #[facet(xml::text)]
        content: String,
    }

    let result: Element =
        facet_xml::from_str(r#"<element id="123" class="foo">hello</element>"#).unwrap();
    assert_eq!(result.attrs.id, "123");
    assert_eq!(result.attrs.class, Some("foo".to_string()));
    assert_eq!(result.content, "hello");
}

// ============================================================================
// flatten with HashMap - capture unknown attributes
// ============================================================================

#[test]
fn flatten_hashmap_captures_unknown_attributes() {
    #[derive(Facet, Debug, PartialEq)]
    struct Element {
        #[facet(xml::attribute)]
        id: String,
        #[facet(flatten, default)]
        extra: HashMap<String, String>,
    }

    let result: Element =
        facet_xml::from_str(r#"<element id="123" data-foo="bar" aria-label="test"/>"#).unwrap();
    assert_eq!(result.id, "123");
    assert_eq!(result.extra.get("data-foo"), Some(&"bar".to_string()));
    assert_eq!(result.extra.get("aria-label"), Some(&"test".to_string()));
}

#[test]
fn flatten_hashmap_captures_unknown_elements() {
    #[derive(Facet, Debug, PartialEq)]
    struct Config {
        name: String,
        #[facet(flatten, default)]
        settings: HashMap<String, String>,
    }

    let result: Config = facet_xml::from_str(
        "<config><name>app</name><timeout>30</timeout><host>localhost</host></config>",
    )
    .unwrap();
    assert_eq!(result.name, "app");
    assert_eq!(result.settings.get("timeout"), Some(&"30".to_string()));
    assert_eq!(result.settings.get("host"), Some(&"localhost".to_string()));
}

#[test]
fn flatten_hashmap_inside_flattened_struct() {
    #[derive(Facet, Debug, PartialEq)]
    struct CommonAttrs {
        #[facet(xml::attribute)]
        id: Option<String>,
        #[facet(flatten, default)]
        extra: HashMap<String, String>,
    }

    #[derive(Facet, Debug, PartialEq)]
    struct Element {
        #[facet(flatten)]
        attrs: CommonAttrs,
        #[facet(xml::text)]
        content: String,
    }

    let result: Element =
        facet_xml::from_str(r#"<element id="123" data-custom="value">hello</element>"#).unwrap();
    assert_eq!(result.attrs.id, Some("123".to_string()));
    assert_eq!(
        result.attrs.extra.get("data-custom"),
        Some(&"value".to_string())
    );
    assert_eq!(result.content, "hello");
}

// ============================================================================
// flatten with Option - optional flattened struct
// ============================================================================

#[test]
fn flatten_option_struct_present() {
    #[derive(Facet, Debug, PartialEq)]
    struct Metadata {
        author: String,
        version: String,
    }

    #[derive(Facet, Debug, PartialEq)]
    struct Document {
        title: String,
        #[facet(flatten)]
        metadata: Option<Metadata>,
    }

    let result: Document = facet_xml::from_str(
        "<document><title>Doc</title><author>Alice</author><version>1.0</version></document>",
    )
    .unwrap();
    assert_eq!(result.title, "Doc");
    assert!(result.metadata.is_some());
    let meta = result.metadata.unwrap();
    assert_eq!(meta.author, "Alice");
    assert_eq!(meta.version, "1.0");
}

#[test]
fn flatten_option_struct_absent() {
    #[derive(Facet, Debug, PartialEq, Default)]
    struct Metadata {
        author: String,
        version: String,
    }

    #[derive(Facet, Debug, PartialEq)]
    struct Document {
        title: String,
        #[facet(flatten, default)]
        metadata: Option<Metadata>,
    }

    let result: Document = facet_xml::from_str("<document><title>Doc</title></document>").unwrap();
    assert_eq!(result.title, "Doc");
    assert!(result.metadata.is_none());
}
