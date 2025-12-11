//! Tests for XML parsing errors and failure cases.
//!
//! These tests verify that facet-xml produces appropriate errors for various
//! invalid inputs and mismatched types.

use facet::Facet;
use facet_xml::{self as xml, DeserializeOptions, XmlErrorKind};

// ============================================================================
// Test helpers
// ============================================================================

/// Helper macro to assert that deserialization fails with a specific error kind.
macro_rules! assert_err_kind {
    ($result:expr, $pattern:pat $(if $guard:expr)? $(,)?) => {
        match &$result {
            Err(e) => match e.kind() {
                $pattern $(if $guard)? => { /* ok */ }
                other => panic!(
                    "expected error matching {}, got: {:?}",
                    stringify!($pattern),
                    other
                ),
            },
            Ok(v) => panic!("expected error, got success: {:?}", v),
        }
    };
}

// ============================================================================
// Malformed XML
// ============================================================================

#[derive(Facet, Debug)]
struct SimpleStruct {
    #[facet(xml::attribute)]
    name: String,
}

#[test]
fn test_malformed_xml_unclosed_tag() {
    let xml = r#"<SimpleStruct name="test">"#;
    let result: Result<SimpleStruct, _> = xml::from_str(xml);
    // Unclosed tag results in unexpected EOF when looking for end tag
    assert_err_kind!(result, XmlErrorKind::UnexpectedEof);
}

#[test]
fn test_malformed_xml_mismatched_tags() {
    let xml = r#"<SimpleStruct name="test"></WrongTag>"#;
    let result: Result<SimpleStruct, _> = xml::from_str(xml);
    assert_err_kind!(result, XmlErrorKind::Parse(_));
}

#[test]
fn test_malformed_xml_invalid_attribute() {
    let xml = r#"<SimpleStruct name=test/>"#;
    let result: Result<SimpleStruct, _> = xml::from_str(xml);
    assert_err_kind!(result, XmlErrorKind::Parse(_));
}

#[test]
fn test_empty_input() {
    let xml = "";
    let result: Result<SimpleStruct, _> = xml::from_str(xml);
    // Empty input results in unexpected event (EOF instead of start element)
    assert_err_kind!(result, XmlErrorKind::UnexpectedEvent(_));
}

#[test]
fn test_whitespace_only_input() {
    let xml = "   \n\t   ";
    let result: Result<SimpleStruct, _> = xml::from_str(xml);
    // Whitespace-only input results in unexpected event (EOF instead of start element)
    assert_err_kind!(result, XmlErrorKind::UnexpectedEvent(_));
}

// ============================================================================
// Type mismatches
// ============================================================================

#[derive(Facet, Debug)]
struct WithInteger {
    #[facet(xml::attribute)]
    count: i32,
}

#[test]
fn test_invalid_integer_value() {
    let xml = r#"<WithInteger count="not_a_number"/>"#;
    let result: Result<WithInteger, _> = xml::from_str(xml);
    // This should fail during reflection/parsing
    assert!(result.is_err(), "Should fail to parse non-integer as i32");
}

#[test]
fn test_float_for_integer() {
    let xml = r#"<WithInteger count="3.14"/>"#;
    let result: Result<WithInteger, _> = xml::from_str(xml);
    assert!(result.is_err(), "Should fail to parse float as i32");
}

#[test]
fn test_overflow_integer() {
    let xml = r#"<WithInteger count="999999999999999999999"/>"#;
    let result: Result<WithInteger, _> = xml::from_str(xml);
    assert!(result.is_err(), "Should fail on integer overflow");
}

#[derive(Facet, Debug)]
struct WithBool {
    #[facet(xml::attribute)]
    enabled: bool,
}

#[test]
fn test_valid_bool_true() {
    let xml = r#"<WithBool enabled="true"/>"#;
    let result: WithBool = xml::from_str(xml).unwrap();
    assert!(result.enabled);
}

#[test]
fn test_valid_bool_false() {
    let xml = r#"<WithBool enabled="false"/>"#;
    let result: WithBool = xml::from_str(xml).unwrap();
    assert!(!result.enabled);
}

#[test]
fn test_bool_accepts_truthy_values() {
    // Note: facet-xml accepts various truthy string values for bool
    let xml = r#"<WithBool enabled="yes"/>"#;
    let result: WithBool = xml::from_str(xml).unwrap();
    // "yes" is accepted as truthy
    assert!(result.enabled);
}

// ============================================================================
// Missing required fields
// ============================================================================

// Note: facet-xml uses type defaults for missing fields, so fields without
// #[facet(default)] will still get their type's Default value (0 for integers,
// empty string for String, etc.). This is different from serde's strict behavior.

#[derive(Facet, Debug)]
struct RequiredFields {
    #[facet(xml::attribute)]
    id: u32,
    #[facet(xml::element)]
    name: String,
}

#[test]
fn test_missing_attribute_uses_default() {
    // Note: facet-xml uses default values for missing fields
    let xml = r#"<RequiredFields><name>Test</name></RequiredFields>"#;
    let result: RequiredFields = xml::from_str(xml).unwrap();
    assert_eq!(result.id, 0); // u32::default()
    assert_eq!(result.name, "Test");
}

// ============================================================================
// Missing XML annotations
// ============================================================================

#[derive(Facet, Debug)]
struct UnannotatedField {
    // Intentionally missing #[facet(xml::...)]
    value: String,
}

#[test]
fn test_deserialize_missing_xml_annotation_errors() {
    let xml = r#"<UnannotatedField><value>hi</value></UnannotatedField>"#;
    let result: Result<UnannotatedField, _> = xml::from_str(xml);
    assert_err_kind!(result, XmlErrorKind::MissingXmlAnnotations { .. });
}

#[test]
fn test_serialize_missing_xml_annotation_errors() {
    let value = UnannotatedField {
        value: "hi".to_string(),
    };
    let result = xml::to_string(&value);
    assert_err_kind!(result, XmlErrorKind::MissingXmlAnnotations { .. });
}

#[test]
fn test_missing_element_uses_default() {
    // Note: facet-xml uses default values for missing elements
    let xml = r#"<RequiredFields id="42"/>"#;
    let result: RequiredFields = xml::from_str(xml).unwrap();
    assert_eq!(result.id, 42);
    assert_eq!(result.name, ""); // String::default()
}

// ============================================================================
// Optional fields (should succeed)
// ============================================================================

#[derive(Facet, Debug)]
struct OptionalFields {
    #[facet(xml::attribute)]
    id: Option<u32>,
    #[facet(xml::element)]
    name: Option<String>,
}

#[test]
fn test_optional_fields_all_missing() {
    let xml = r#"<OptionalFields/>"#;
    let result: OptionalFields = xml::from_str(xml).unwrap();
    assert_eq!(result.id, None);
    assert_eq!(result.name, None);
}

#[test]
fn test_optional_fields_all_present() {
    let xml = r#"<OptionalFields id="42"><name>Test</name></OptionalFields>"#;
    let result: OptionalFields = xml::from_str(xml).unwrap();
    assert_eq!(result.id, Some(42));
    assert_eq!(result.name, Some("Test".to_string()));
}

// ============================================================================
// Default fields
// ============================================================================

#[derive(Facet, Debug)]
struct WithDefaults {
    #[facet(xml::attribute, default)]
    count: i32,
    #[facet(xml::element, default)]
    name: String,
}

#[test]
fn test_default_fields_used() {
    let xml = r#"<WithDefaults/>"#;
    let result: WithDefaults = xml::from_str(xml).unwrap();
    assert_eq!(result.count, 0); // i32::default()
    assert_eq!(result.name, ""); // String::default()
}

#[test]
fn test_default_fields_overridden() {
    let xml = r#"<WithDefaults count="10"><name>Custom</name></WithDefaults>"#;
    let result: WithDefaults = xml::from_str(xml).unwrap();
    assert_eq!(result.count, 10);
    assert_eq!(result.name, "Custom");
}

// ============================================================================
// Unknown fields with deny_unknown_fields
// ============================================================================

#[derive(Facet, Debug)]
#[facet(deny_unknown_fields)]
struct StrictStruct {
    #[facet(xml::attribute)]
    name: String,
}

#[test]
fn test_deny_unknown_fields_attribute_unknown_attr() {
    let xml = r#"<StrictStruct name="test" extra="unknown"/>"#;
    let result: Result<StrictStruct, _> = xml::from_str(xml);
    assert_err_kind!(result, XmlErrorKind::UnknownAttribute { attribute, .. } if attribute == "extra");
}

#[test]
fn test_deny_unknown_fields_attribute_unknown_element() {
    let xml = r#"<StrictStruct name="test"><extra>value</extra></StrictStruct>"#;
    let result: Result<StrictStruct, _> = xml::from_str(xml);
    assert_err_kind!(result, XmlErrorKind::UnknownField { field, .. } if field == "extra");
}

#[test]
fn test_deny_unknown_fields_attribute_valid() {
    let xml = r#"<StrictStruct name="test"/>"#;
    let result: StrictStruct = xml::from_str(xml).unwrap();
    assert_eq!(result.name, "test");
}

// ============================================================================
// Unknown fields with runtime option
// ============================================================================

#[derive(Facet, Debug)]
struct LenientStruct {
    #[facet(xml::attribute)]
    name: String,
}

#[test]
fn test_runtime_deny_unknown_rejects_extra_attr() {
    let xml = r#"<LenientStruct name="test" unknown="ignored"/>"#;

    // Default: unknown attributes are ignored
    let result: LenientStruct = xml::from_str(xml).unwrap();
    assert_eq!(result.name, "test");

    // With option: unknown attributes cause error
    let options = DeserializeOptions::default().deny_unknown_fields(true);
    let result: Result<LenientStruct, _> = xml::from_str_with_options(xml, &options);
    assert_err_kind!(result, XmlErrorKind::UnknownAttribute { attribute, .. } if attribute == "unknown");
}

#[test]
fn test_runtime_deny_unknown_rejects_extra_element() {
    let xml = r#"<LenientStruct name="test"><unknown>ignored</unknown></LenientStruct>"#;

    // Default: unknown elements are ignored
    let result: LenientStruct = xml::from_str(xml).unwrap();
    assert_eq!(result.name, "test");

    // With option: unknown elements cause error
    let options = DeserializeOptions::default().deny_unknown_fields(true);
    let result: Result<LenientStruct, _> = xml::from_str_with_options(xml, &options);
    assert_err_kind!(result, XmlErrorKind::UnknownField { field, .. } if field == "unknown");
}

// ============================================================================
// Invalid UTF-8
// ============================================================================

#[test]
fn test_invalid_utf8() {
    let bytes: &[u8] = &[0xFF, 0xFE, 0x00, 0x00]; // Invalid UTF-8
    let result: Result<SimpleStruct, _> = xml::from_slice(bytes);
    assert_err_kind!(result, XmlErrorKind::InvalidUtf8(_));
}

// ============================================================================
// Enum variants
// ============================================================================

// Note: In facet-xml, enums are represented as nested elements where the
// element name is the variant name. The #[repr(u8)] enum must use element
// children, not text content.

#[derive(Facet, Debug, PartialEq)]
#[repr(u8)]
enum Status {
    #[facet(rename = "active")]
    Active,
    #[facet(rename = "inactive")]
    Inactive,
}

#[derive(Facet, Debug)]
struct WithEnum {
    #[facet(xml::element)]
    status: Status,
}

#[test]
fn test_valid_enum_variant() {
    // XML enums expect the variant as a child element, not text content
    let xml = r#"<WithEnum><status><active/></status></WithEnum>"#;
    let result: WithEnum = xml::from_str(xml).unwrap();
    assert_eq!(result.status, Status::Active);
}

// ============================================================================
// Nested struct errors
// ============================================================================

#[derive(Facet, Debug)]
struct Inner {
    #[facet(xml::attribute)]
    value: i32,
}

#[derive(Facet, Debug)]
struct Outer {
    #[facet(xml::element)]
    inner: Inner,
}

#[test]
fn test_nested_struct_inner_error() {
    let xml = r#"<Outer><inner value="not_a_number"/></Outer>"#;
    let result: Result<Outer, _> = xml::from_str(xml);
    assert!(result.is_err(), "Should propagate error from nested struct");
}

#[test]
fn test_nested_struct_missing_inner() {
    let xml = r#"<Outer/>"#;
    let result: Result<Outer, _> = xml::from_str(xml);
    assert!(
        result.is_err(),
        "Should fail when required nested struct is missing"
    );
}

// ============================================================================
// Collections
// ============================================================================

#[derive(Facet, Debug)]
struct WithList {
    #[facet(xml::elements)]
    items: Vec<Item>,
}

#[derive(Facet, Debug)]
struct Item {
    #[facet(xml::attribute)]
    id: i32,
}

#[test]
fn test_list_with_invalid_item() {
    let xml = r#"<WithList>
        <Item id="1"/>
        <Item id="not_a_number"/>
        <Item id="3"/>
    </WithList>"#;
    let result: Result<WithList, _> = xml::from_str(xml);
    assert!(
        result.is_err(),
        "Should fail when list item has invalid value"
    );
}

#[test]
fn test_empty_list() {
    let xml = r#"<WithList/>"#;
    let result: WithList = xml::from_str(xml).unwrap();
    assert!(result.items.is_empty());
}

#[test]
fn test_valid_list() {
    let xml = r#"<WithList>
        <Item id="1"/>
        <Item id="2"/>
        <Item id="3"/>
    </WithList>"#;
    let result: WithList = xml::from_str(xml).unwrap();
    assert_eq!(result.items.len(), 3);
    assert_eq!(result.items[0].id, 1);
    assert_eq!(result.items[1].id, 2);
    assert_eq!(result.items[2].id, 3);
}

// ============================================================================
// Text content errors
// ============================================================================

// Note: facet-xml's text content parsing currently only works with String types.
// For other types, you would typically need to post-process.

#[derive(Facet, Debug)]
struct WithTextString {
    #[facet(xml::text)]
    content: String,
}

#[test]
fn test_text_content_valid_string() {
    let xml = r#"<WithTextString>Hello, World!</WithTextString>"#;
    let result: WithTextString = xml::from_str(xml).unwrap();
    assert_eq!(result.content, "Hello, World!");
}

#[test]
fn test_text_content_empty() {
    let xml = r#"<WithTextString></WithTextString>"#;
    let result: WithTextString = xml::from_str(xml).unwrap();
    assert_eq!(result.content, "");
}

#[test]
fn test_text_content_with_whitespace() {
    // Note: facet-xml trims leading/trailing whitespace from text content
    let xml = r#"<WithTextString>  spaced  </WithTextString>"#;
    let result: WithTextString = xml::from_str(xml).unwrap();
    assert_eq!(result.content, "spaced"); // whitespace is trimmed
}

// ============================================================================
// Renamed fields
// ============================================================================

#[derive(Facet, Debug)]
struct RenamedFields {
    #[facet(xml::attribute, rename = "user-name")]
    user_name: String,
    #[facet(xml::element, rename = "user-email")]
    user_email: String,
}

#[test]
fn test_renamed_fields_correct_names() {
    let xml = r#"<RenamedFields user-name="alice"><user-email>alice@example.com</user-email></RenamedFields>"#;
    let result: RenamedFields = xml::from_str(xml).unwrap();
    assert_eq!(result.user_name, "alice");
    assert_eq!(result.user_email, "alice@example.com");
}

#[test]
fn test_renamed_fields_wrong_names_ignored() {
    // Note: facet-xml uses defaults for unmatched fields and ignores unknown ones
    // Using original field names instead of renamed ones - they get ignored
    let xml = r#"<RenamedFields user_name="alice"><user_email>alice@example.com</user_email></RenamedFields>"#;
    let result: RenamedFields = xml::from_str(xml).unwrap();
    // Fields with wrong names are ignored, defaults are used
    assert_eq!(result.user_name, ""); // default
    assert_eq!(result.user_email, ""); // default
}

#[test]
fn test_renamed_fields_strict_mode() {
    // With deny_unknown_fields option, wrong field names cause errors
    let xml =
        r#"<RenamedFields user_name="alice"><user_email>ignored</user_email></RenamedFields>"#;
    let options = DeserializeOptions::default().deny_unknown_fields(true);
    let result: Result<RenamedFields, _> = xml::from_str_with_options(xml, &options);
    // Should fail because user_name doesn't match the renamed user-name
    assert!(
        result.is_err(),
        "Should reject unmatched attributes in strict mode"
    );
}
