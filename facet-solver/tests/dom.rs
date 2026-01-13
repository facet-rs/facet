//! DOM format tests: Attribute vs Element disambiguation.
//!
//! These tests verify that the solver correctly distinguishes between
//! fields that are attributes vs elements in DOM formats (XML, HTML).

use facet::Facet;
use facet_solver::{FieldCategory, Format, Key, KeyResult, Schema, Solver};

// Import xml namespace for attributes
use facet_xml as xml;

// ============================================================================
// Test 1: Same name as attribute vs element
// ============================================================================

#[derive(Facet, Debug)]
struct ElementWithAttrAndChild {
    /// This is an attribute
    #[facet(xml::attribute)]
    class: String,

    /// This is a child element with the same name
    #[facet(rename = "class")]
    class_element: String,
}

#[test]
fn test_attribute_vs_element_same_name() {
    let schema = Schema::build_dom(ElementWithAttrAndChild::SHAPE).unwrap();

    // Debug: print schema info
    eprintln!("Format: {:?}", schema.format());
    eprintln!("Resolutions: {}", schema.resolutions().len());

    // Should have one resolution
    assert_eq!(schema.resolutions().len(), 1);
    let resolution = &schema.resolutions()[0];

    // Debug: print fields
    for (name, info) in resolution.fields() {
        eprintln!(
            "Field: {} -> category={:?}, path={:?}",
            name, info.category, info.path
        );
    }

    // Both fields should be present
    assert!(resolution.field("class").is_some());

    // Create solver and test attribute lookup
    let mut solver = Solver::new(&schema);

    // Seeing "class" as an attribute should work
    let result = solver.see_attribute("class");
    assert!(
        matches!(result, KeyResult::Unambiguous { .. } | KeyResult::Solved(_)),
        "attribute lookup failed: {:?}",
        result
    );

    // Create fresh solver for element test
    let mut solver2 = Solver::new(&schema);

    // Seeing "class" as an element should also work
    let result = solver2.see_element("class");
    assert!(
        matches!(result, KeyResult::Unambiguous { .. } | KeyResult::Solved(_)),
        "element lookup failed: {:?}",
        result
    );
}

// ============================================================================
// Test 2: Flattened struct with attributes
// ============================================================================

#[derive(Facet, Debug)]
struct GlobalAttrs {
    #[facet(xml::attribute)]
    id: Option<String>,

    #[facet(xml::attribute)]
    class: Option<String>,
}

#[derive(Facet, Debug)]
struct DivElement {
    #[facet(flatten)]
    attrs: GlobalAttrs,

    /// Child content
    content: Option<String>,
}

#[test]
fn test_flattened_struct_with_attributes() {
    let schema = Schema::build_dom(DivElement::SHAPE).unwrap();

    assert_eq!(schema.resolutions().len(), 1);
    let resolution = &schema.resolutions()[0];

    // Check that flattened attributes are present
    let id_field = resolution.field("id");
    assert!(id_field.is_some(), "id field should be present");
    assert_eq!(
        id_field.unwrap().category,
        Some(FieldCategory::Attribute),
        "id should be an attribute"
    );

    let class_field = resolution.field("class");
    assert!(class_field.is_some(), "class field should be present");
    assert_eq!(
        class_field.unwrap().category,
        Some(FieldCategory::Attribute),
        "class should be an attribute"
    );

    // content should be an element
    let content_field = resolution.field("content");
    assert!(content_field.is_some(), "content field should be present");
    assert_eq!(
        content_field.unwrap().category,
        Some(FieldCategory::Element),
        "content should be an element"
    );

    // Test solver
    let mut solver = Solver::new(&schema);

    // Attribute lookup
    let result = solver.see_attribute("id");
    assert!(
        !matches!(result, KeyResult::Unknown),
        "id attribute should be known"
    );

    let result = solver.see_attribute("class");
    assert!(
        !matches!(result, KeyResult::Unknown),
        "class attribute should be known"
    );

    // Element lookup
    let result = solver.see_element("content");
    assert!(
        !matches!(result, KeyResult::Unknown),
        "content element should be known"
    );

    // Attribute lookup for element should fail
    let mut solver3 = Solver::new(&schema);
    let result = solver3.see_attribute("content");
    assert!(
        matches!(result, KeyResult::Unknown),
        "content as attribute should be unknown"
    );
}

// ============================================================================
// Test 3: Enum disambiguation with DOM categories
// ============================================================================

#[derive(Facet, Debug)]
struct InputText {
    #[facet(xml::attribute)]
    value: String,
}

#[derive(Facet, Debug)]
struct InputCheckbox {
    #[facet(xml::attribute)]
    checked: Option<String>,
}

#[derive(Facet, Debug)]
#[repr(u8)]
enum InputKind {
    Text(InputText),
    Checkbox(InputCheckbox),
}

#[derive(Facet, Debug)]
struct InputElement {
    #[facet(xml::attribute)]
    name: String,

    #[facet(flatten)]
    kind: InputKind,
}

#[test]
fn test_enum_disambiguation_with_attributes() {
    let schema = Schema::build_dom(InputElement::SHAPE).unwrap();

    // Should have two resolutions (one per variant)
    assert_eq!(
        schema.resolutions().len(),
        2,
        "should have 2 resolutions for 2 variants"
    );

    // Test that "value" attribute disambiguates to Text variant
    let mut solver = Solver::new(&schema);
    solver.see_attribute("name"); // Common field
    let result = solver.see_attribute("value");

    match result {
        KeyResult::Solved(handle) => {
            let desc = handle.resolution().describe();
            assert!(
                desc.contains("Text"),
                "value attribute should solve to Text variant, got: {}",
                desc
            );
        }
        KeyResult::Unambiguous { .. } => {
            // Still need to finish to get resolution
            let handle = solver.finish().expect("should finish successfully");
            let desc = handle.resolution().describe();
            assert!(
                desc.contains("Text"),
                "value attribute should solve to Text variant, got: {}",
                desc
            );
        }
        other => panic!("unexpected result for value attribute: {:?}", other),
    }

    // Test that "checked" attribute disambiguates to Checkbox variant
    let mut solver2 = Solver::new(&schema);
    solver2.see_attribute("name"); // Common field
    let result = solver2.see_attribute("checked");

    match result {
        KeyResult::Solved(handle) => {
            let desc = handle.resolution().describe();
            assert!(
                desc.contains("Checkbox"),
                "checked attribute should solve to Checkbox variant, got: {}",
                desc
            );
        }
        KeyResult::Unambiguous { .. } => {
            let handle = solver2.finish().expect("should finish successfully");
            let desc = handle.resolution().describe();
            assert!(
                desc.contains("Checkbox"),
                "checked attribute should solve to Checkbox variant, got: {}",
                desc
            );
        }
        other => panic!("unexpected result for checked attribute: {:?}", other),
    }
}

// ============================================================================
// Test 4: Text content field
// ============================================================================

#[derive(Facet, Debug)]
struct TextElement {
    #[facet(xml::attribute)]
    lang: Option<String>,

    #[facet(xml::text)]
    content: String,
}

#[test]
fn test_text_content_field() {
    let schema = Schema::build_dom(TextElement::SHAPE).unwrap();

    assert_eq!(schema.resolutions().len(), 1);
    let resolution = &schema.resolutions()[0];

    // Check content is marked as Text category
    let content_field = resolution.field("content");
    assert!(content_field.is_some());
    assert_eq!(
        content_field.unwrap().category,
        Some(FieldCategory::Text),
        "content should be Text category"
    );

    // Test solver with text key
    let mut solver = Solver::new(&schema);
    let result = solver.see_text();
    assert!(
        !matches!(result, KeyResult::Unknown),
        "text content should be known"
    );
}

// ============================================================================
// Test 5: Flat format ignores categories
// ============================================================================

#[test]
fn test_flat_format_ignores_categories() {
    // Same struct but with flat format
    let schema = Schema::build_with_format(DivElement::SHAPE, Format::Flat).unwrap();

    let mut solver = Solver::new(&schema);

    // In flat format, regular see_key should work for everything
    let result = solver.see_key("id");
    assert!(
        !matches!(result, KeyResult::Unknown),
        "id should be known in flat format"
    );

    let result = solver.see_key("class");
    assert!(
        !matches!(result, KeyResult::Unknown),
        "class should be known in flat format"
    );

    let result = solver.see_key("content");
    assert!(
        !matches!(result, KeyResult::Unknown),
        "content should be known in flat format"
    );
}

// ============================================================================
// Test 6: Key type conversions
// ============================================================================

#[test]
fn test_key_type_conversions() {
    // &str -> Key (should be Flat)
    let key: Key = "test".into();
    assert!(matches!(key, Key::Flat(_)));
    assert_eq!(key.name(), "test");

    // String -> Key (should be Flat)
    let key: Key = String::from("test").into();
    assert!(matches!(key, Key::Flat(_)));

    // Key constructors
    let attr_key = Key::attribute("class");
    assert!(matches!(attr_key, Key::Dom(FieldCategory::Attribute, _)));
    assert_eq!(attr_key.name(), "class");
    assert_eq!(attr_key.category(), Some(FieldCategory::Attribute));

    let elem_key = Key::element("div");
    assert!(matches!(elem_key, Key::Dom(FieldCategory::Element, _)));
    assert_eq!(elem_key.name(), "div");

    let text_key = Key::text();
    assert!(matches!(text_key, Key::Dom(FieldCategory::Text, _)));
    assert_eq!(text_key.name(), "");
}
