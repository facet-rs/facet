//! Basic tests for facet-xml behavior that needs to be implemented.

use facet::Facet;
use facet_xml as xml;
use test_log::test;

// ============================================================================
// xml::element - explicit single child element
// ============================================================================

#[test]
fn xml_element_single_child() {
    #[derive(Facet, Debug, PartialEq)]
    struct Inner {
        value: String,
    }

    #[derive(Facet, Debug, PartialEq)]
    struct Outer {
        #[facet(xml::element)]
        inner: Inner,
    }

    let result: Outer =
        facet_xml::from_str("<outer><inner><value>hello</value></inner></outer>").unwrap();
    assert_eq!(result.inner.value, "hello");
}

// ============================================================================
// xml::tag - capture element tag name
// ============================================================================

#[test]
fn xml_tag_captures_element_name() {
    #[derive(Facet, Debug, PartialEq)]
    struct AnyElement {
        #[facet(xml::tag)]
        tag: String,
        #[facet(xml::text)]
        content: String,
    }

    let result: AnyElement = facet_xml::from_str("<foo>hello</foo>").unwrap();
    assert_eq!(result.tag, "foo");
    assert_eq!(result.content, "hello");
}

#[test]
fn xml_tag_captures_different_tags() {
    #[derive(Facet, Debug, PartialEq)]
    struct AnyElement {
        #[facet(xml::tag)]
        tag: String,
        #[facet(xml::text)]
        content: String,
    }

    let result: AnyElement = facet_xml::from_str("<bar>world</bar>").unwrap();
    assert_eq!(result.tag, "bar");
    assert_eq!(result.content, "world");
}

// ============================================================================
// default - missing elements get default values
// ============================================================================

#[test]
fn default_for_missing_element() {
    #[derive(Facet, Debug, PartialEq)]
    struct Config {
        #[facet(default)]
        name: String,
        #[facet(default)]
        count: u32,
    }

    let result: Config = facet_xml::from_str("<config></config>").unwrap();
    assert_eq!(result.name, "");
    assert_eq!(result.count, 0);
}

#[test]
fn default_for_missing_attribute() {
    #[derive(Facet, Debug, PartialEq)]
    struct Element {
        #[facet(xml::attribute, default)]
        id: String,
        #[facet(xml::attribute, default)]
        count: u32,
    }

    let result: Element = facet_xml::from_str("<element/>").unwrap();
    assert_eq!(result.id, "");
    assert_eq!(result.count, 0);
}

#[test]
fn default_with_custom_value() {
    fn default_name() -> String {
        "unnamed".to_string()
    }

    #[derive(Facet, Debug, PartialEq)]
    struct Item {
        #[facet(default = default_name())]
        name: String,
    }

    let result: Item = facet_xml::from_str("<item></item>").unwrap();
    assert_eq!(result.name, "unnamed");
}

// ============================================================================
// alias - alternative names for fields
// ============================================================================

#[test]
fn alias_matches_alternative_name() {
    #[derive(Facet, Debug, PartialEq)]
    struct Person {
        #[facet(alias = "fullName")]
        name: String,
    }

    // Primary name works
    let result: Person = facet_xml::from_str("<person><name>Alice</name></person>").unwrap();
    assert_eq!(result.name, "Alice");

    // Alias also works
    let result: Person = facet_xml::from_str("<person><fullName>Bob</fullName></person>").unwrap();
    assert_eq!(result.name, "Bob");
}

#[test]
fn alias_for_attribute() {
    #[derive(Facet, Debug, PartialEq)]
    struct Element {
        #[facet(xml::attribute, alias = "identifier")]
        id: String,
    }

    // Primary name works
    let result: Element = facet_xml::from_str(r#"<element id="123"/>"#).unwrap();
    assert_eq!(result.id, "123");

    // Alias also works
    let result: Element = facet_xml::from_str(r#"<element identifier="456"/>"#).unwrap();
    assert_eq!(result.id, "456");
}

// ============================================================================
// deny_unknown_fields - reject unexpected elements/attributes
// ============================================================================

#[test]
fn deny_unknown_fields_rejects_unknown_element() {
    #[derive(Facet, Debug, PartialEq)]
    #[facet(deny_unknown_fields)]
    struct Strict {
        name: String,
    }

    let result =
        facet_xml::from_str::<Strict>("<strict><name>ok</name><extra>bad</extra></strict>");
    assert!(result.is_err(), "Should reject unknown element <extra>");
}

#[test]
fn deny_unknown_fields_rejects_unknown_attribute() {
    #[derive(Facet, Debug, PartialEq)]
    #[facet(deny_unknown_fields)]
    struct Strict {
        #[facet(xml::attribute)]
        id: String,
    }

    let result = facet_xml::from_str::<Strict>(r#"<strict id="1" extra="bad"/>"#);
    assert!(result.is_err(), "Should reject unknown attribute extra=");
}

#[test]
fn without_deny_unknown_fields_ignores_extra() {
    #[derive(Facet, Debug, PartialEq)]
    struct Lenient {
        name: String,
    }

    // Should succeed, ignoring the extra element
    let result: Lenient =
        facet_xml::from_str("<lenient><name>ok</name><extra>ignored</extra></lenient>").unwrap();
    assert_eq!(result.name, "ok");
}

// ============================================================================
// Option<T> - optional fields
// ============================================================================

#[test]
fn optional_attribute_present() {
    #[derive(Facet, Debug, PartialEq)]
    struct Element {
        #[facet(xml::attribute)]
        id: Option<String>,
    }

    let result: Element = facet_xml::from_str(r#"<element id="123"/>"#).unwrap();
    assert_eq!(result.id, Some("123".to_string()));
}

#[test]
fn optional_attribute_absent() {
    #[derive(Facet, Debug, PartialEq)]
    struct Element {
        #[facet(xml::attribute)]
        id: Option<String>,
    }

    let result: Element = facet_xml::from_str(r#"<element/>"#).unwrap();
    assert_eq!(result.id, None);
}

#[test]
fn optional_child_element_present() {
    #[derive(Facet, Debug, PartialEq)]
    struct Container {
        child: Option<String>,
    }

    let result: Container =
        facet_xml::from_str("<container><child>value</child></container>").unwrap();
    assert_eq!(result.child, Some("value".to_string()));
}

#[test]
fn optional_child_element_absent() {
    #[derive(Facet, Debug, PartialEq)]
    struct Container {
        child: Option<String>,
    }

    let result: Container = facet_xml::from_str("<container></container>").unwrap();
    assert_eq!(result.child, None);
}

#[test]
fn empty_self_closing_element() {
    #[derive(Facet, Debug, PartialEq)]
    struct Empty {
        #[facet(xml::attribute)]
        flag: Option<String>,
    }

    let result: Empty = facet_xml::from_str("<empty/>").unwrap();
    assert_eq!(result.flag, None);
}

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
// Field name lowerCamelCase conversion
// ============================================================================

#[test]
fn field_name_snake_case_becomes_lower_camel() {
    #[derive(Facet, Debug, PartialEq)]
    struct Config {
        user_name: String, // expects <userName>
        max_retries: u32,  // expects <maxRetries>
    }

    let result: Config = facet_xml::from_str(
        "<config><userName>alice</userName><maxRetries>3</maxRetries></config>",
    )
    .unwrap();
    assert_eq!(result.user_name, "alice");
    assert_eq!(result.max_retries, 3);
}

#[test]
fn field_name_already_lower_camel_unchanged() {
    #[derive(Facet, Debug, PartialEq)]
    #[allow(non_snake_case)]
    struct Config {
        userName: String, // already lowerCamelCase
    }

    let result: Config = facet_xml::from_str("<config><userName>bob</userName></config>").unwrap();
    assert_eq!(result.userName, "bob");
}

#[test]
fn field_rename_overrides_conversion() {
    #[derive(Facet, Debug, PartialEq)]
    struct Config {
        #[facet(rename = "USER_NAME")]
        user_name: String, // expects <USER_NAME> not <userName>
    }

    let result: Config =
        facet_xml::from_str("<config><USER_NAME>charlie</USER_NAME></config>").unwrap();
    assert_eq!(result.user_name, "charlie");
}

#[test]
fn attribute_name_snake_case_becomes_lower_camel() {
    #[derive(Facet, Debug, PartialEq)]
    struct Element {
        #[facet(xml::attribute)]
        data_id: String, // expects data-id? or dataId?
        #[facet(xml::attribute)]
        max_size: u32, // expects maxSize
    }

    let result: Element = facet_xml::from_str(r#"<element dataId="abc" maxSize="100"/>"#).unwrap();
    assert_eq!(result.data_id, "abc");
    assert_eq!(result.max_size, 100);
}

#[test]
fn multi_word_struct_name_becomes_lower_camel() {
    #[derive(Facet, Debug, PartialEq)]
    struct MyPlaylist {
        name: String,
    }

    // MyPlaylist → <myPlaylist>
    let result: MyPlaylist =
        facet_xml::from_str("<myPlaylist><name>Favorites</name></myPlaylist>").unwrap();
    assert_eq!(result.name, "Favorites");
}

#[test]
fn serialization_uses_lower_camel_case() {
    #[derive(Facet, Debug, PartialEq)]
    struct Config {
        user_name: String,
        max_retries: u32,
    }

    let config = Config {
        user_name: "alice".to_string(),
        max_retries: 3,
    };

    let xml = facet_xml::to_string(&config).unwrap();
    assert!(xml.contains("<config>"), "root should be <config>");
    assert!(xml.contains("<userName>"), "field should be <userName>");
    assert!(xml.contains("<maxRetries>"), "field should be <maxRetries>");
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

    // Text nodes around a child element (note: XML parser trims whitespace from text nodes)
    let result: Message = facet_xml::from_str("<message>Hello <b>world</b>!</message>").unwrap();
    assert_eq!(result.parts, vec!["Hello", "!"]);
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
