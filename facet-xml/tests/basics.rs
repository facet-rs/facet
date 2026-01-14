//! Basic tests for facet-xml behavior that needs to be implemented.

use facet::Facet;
use facet_xml as xml;

// ============================================================================
// lowerCamelCase default
// ============================================================================

#[test]
fn struct_name_defaults_to_lower_camel_case() {
    #[derive(Facet, Debug, PartialEq)]
    struct Banana {
        taste: String,
    }

    // Should match <banana>, not <Banana>
    let result: Banana = facet_xml::from_str("<banana><taste>sweet</taste></banana>").unwrap();
    assert_eq!(result.taste, "sweet");
}

#[test]
fn struct_name_rejects_wrong_case() {
    #[derive(Facet, Debug, PartialEq)]
    struct Banana {
        taste: String,
    }

    // Should FAIL - <Banana> doesn't match expected <banana>
    let result = facet_xml::from_str::<Banana>("<Banana><taste>sweet</taste></Banana>");
    assert!(
        result.is_err(),
        "Should reject <Banana> when expecting <banana>"
    );
}

#[test]
fn struct_name_rejects_completely_wrong_tag() {
    #[derive(Facet, Debug, PartialEq)]
    struct Banana {
        taste: String,
    }

    // Should FAIL - <apple> doesn't match expected <banana>
    let result = facet_xml::from_str::<Banana>("<apple><taste>sweet</taste></apple>");
    assert!(
        result.is_err(),
        "Should reject <apple> when expecting <banana>"
    );
}

#[test]
fn rename_overrides_default() {
    #[derive(Facet, Debug, PartialEq)]
    #[facet(rename = "Banana")]
    struct Banana {
        taste: String,
    }

    // With explicit rename, should match <Banana>
    let result: Banana = facet_xml::from_str("<Banana><taste>sweet</taste></Banana>").unwrap();
    assert_eq!(result.taste, "sweet");
}

// ============================================================================
// Vec default singularization
// ============================================================================

#[test]
fn vec_defaults_to_singularized_element_name() {
    #[derive(Facet, Debug, PartialEq)]
    struct Playlist {
        tracks: Vec<String>, // "tracks" → expects <track> elements
    }

    let result: Playlist =
        facet_xml::from_str("<playlist><track>Song A</track><track>Song B</track></playlist>")
            .unwrap();
    assert_eq!(result.tracks, vec!["Song A", "Song B"]);
}

#[test]
fn vec_rename_overrides_singularization() {
    #[derive(Facet, Debug, PartialEq)]
    struct Playlist {
        #[facet(rename = "song")]
        tracks: Vec<String>, // expects <song> instead of <track>
    }

    let result: Playlist =
        facet_xml::from_str("<playlist><song>Song A</song><song>Song B</song></playlist>").unwrap();
    assert_eq!(result.tracks, vec!["Song A", "Song B"]);
}

// ============================================================================
// Vec with xml::text - collect text nodes
// ============================================================================

#[test]
fn vec_with_xml_text_collects_text_nodes() {
    #[derive(Facet, Debug, PartialEq)]
    struct Message {
        #[facet(xml::text)]
        parts: Vec<String>,
    }

    let result: Message = facet_xml::from_str("<message>Hello world!</message>").unwrap();
    assert_eq!(result.parts, vec!["Hello world!"]);
}

#[test]
fn vec_with_xml_text_collects_multiple_text_nodes() {
    #[derive(Facet, Debug, PartialEq)]
    struct Message {
        #[facet(xml::text)]
        parts: Vec<String>,
    }

    // Text nodes around a child element
    let result: Message = facet_xml::from_str("<message>Hello <b>world</b>!</message>").unwrap();
    assert_eq!(result.parts, vec!["Hello ", "!"]);
}

// ============================================================================
// Vec with xml::attribute - collect attribute values
// ============================================================================

#[test]
fn vec_with_xml_attribute_collects_all_values() {
    #[derive(Facet, Debug, PartialEq)]
    #[facet(rename = "element")]
    struct Element {
        #[facet(xml::attribute)]
        values: Vec<String>,
    }

    let result: Element = facet_xml::from_str(r#"<element foo="1" bar="2" baz="3"/>"#).unwrap();
    assert_eq!(result.values, vec!["1", "2", "3"]);
}
