use std::borrow::Cow;

use facet::Facet;
use facet_testhelpers::test;
use facet_xml::{self as xml, from_str, to_string};

#[test]
fn test_deserialize_attribute_when_element_with_the_same_name_is_present() {
    #[derive(Facet, Debug)]
    #[facet(rename = "root")]
    struct Root<'a> {
        #[facet(xml::attribute)]
        id: Cow<'a, str>,
    }

    let xml_data = r#"<root><id>value</id></root>"#;
    assert!(from_str::<Root>(xml_data).is_err());
}

#[test]
fn test_deserialize_element_when_attribute_with_the_same_name_is_present() {
    #[derive(Facet, Debug)]
    #[facet(rename = "root")]
    struct Root<'a> {
        #[facet(xml::element)]
        id: Cow<'a, str>,
    }

    let xml_data = r#"<root id="value"/>"#;
    assert!(from_str::<Root>(xml_data).is_err());
}

#[test]
fn test_deserialize_attribute_and_element_with_the_same_name() {
    #[derive(Facet, Debug, PartialEq)]
    #[facet(rename = "root")]
    struct Root<'a> {
        #[facet(xml::attribute, rename = "id")]
        id_attribute: Cow<'a, str>,
        #[facet(xml::element, rename = "id")]
        id_element: Cow<'a, str>,
    }

    let xml_data = r#"<root id="attribute"><id>element</id></root>"#;
    assert_eq!(
        from_str::<Root>(xml_data).unwrap(),
        Root {
            id_attribute: Cow::Borrowed("attribute"),
            id_element: Cow::Borrowed("element")
        }
    );
}

#[test]
fn test_serialize_attribute_and_element_with_the_same_name() {
    #[derive(Facet, Debug)]
    #[facet(rename = "root")]
    struct Root<'a> {
        #[facet(xml::attribute, rename = "id")]
        id_attribute: Cow<'a, str>,
        #[facet(xml::element, rename = "id")]
        id_element: Cow<'a, str>,
    }

    assert_eq!(
        to_string(&Root {
            id_attribute: Cow::Borrowed("attribute"),
            id_element: Cow::Borrowed("element")
        })
        .unwrap(),
        r#"<root id="attribute"><id>element</id></root>"#,
    );
}
