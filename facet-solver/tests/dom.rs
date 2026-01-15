//! DOM format tests: Attribute vs Element disambiguation.
//!
//! These tests verify that the solver correctly distinguishes between
//! fields that are attributes vs elements in DOM formats (XML, HTML).

use facet::Facet;
use facet_solver::{FieldCategory, FieldKey, Format, KeyResult, Schema, Solver};
use facet_testhelpers::test;

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
    for (key, info) in resolution.fields() {
        eprintln!(
            "Field: {} -> category={:?}, path={:?}",
            key, info.category, info.path
        );
    }

    // Both fields should be present (by name)
    assert!(resolution.field_by_name("class").is_some());

    // Should have TWO fields with name "class" but different categories
    let attr_key = FieldKey::attribute("class");
    let elem_key = FieldKey::element("class");
    assert!(
        resolution.field_by_key(&attr_key).is_some(),
        "attribute class should exist"
    );
    assert!(
        resolution.field_by_key(&elem_key).is_some(),
        "element class should exist"
    );

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
    let id_field = resolution.field_by_name("id");
    assert!(id_field.is_some(), "id field should be present");
    assert_eq!(
        id_field.unwrap().category,
        FieldCategory::Attribute,
        "id should be an attribute"
    );

    let class_field = resolution.field_by_name("class");
    assert!(class_field.is_some(), "class field should be present");
    assert_eq!(
        class_field.unwrap().category,
        FieldCategory::Attribute,
        "class should be an attribute"
    );

    // content should be an element
    let content_field = resolution.field_by_name("content");
    assert!(content_field.is_some(), "content field should be present");
    assert_eq!(
        content_field.unwrap().category,
        FieldCategory::Element,
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
#[allow(unused)]
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

    // Debug: print schema info
    eprintln!("Format: {:?}", schema.format());
    eprintln!("Resolutions: {}", schema.resolutions().len());
    for (i, res) in schema.resolutions().iter().enumerate() {
        eprintln!("Resolution {}: {}", i, res.describe());
        for (key, info) in res.fields() {
            eprintln!("  Field: {} -> category={:?}", key, info.category);
        }
    }

    // Should have two resolutions (one per variant)
    assert_eq!(
        schema.resolutions().len(),
        2,
        "should have 2 resolutions for 2 variants"
    );

    // Note: The current implementation treats enum variants as child elements
    // (Element:Text, Element:Checkbox) rather than flattening their inner fields.
    // This is because newtype enum variants are serialized as their variant name
    // as an element, not as flattened attributes.
    //
    // For now, verify that basic disambiguation by variant element name works.
    let mut solver = Solver::new(&schema);
    solver.see_attribute("name"); // Common field

    // Seeing "Text" as an element should disambiguate to Text variant
    let result = solver.see_element("Text");
    match result {
        KeyResult::Solved(handle) => {
            let desc = handle.resolution().describe();
            assert!(
                desc.contains("Text"),
                "Text element should solve to Text variant, got: {}",
                desc
            );
        }
        KeyResult::Unambiguous { .. } => {
            let handle = solver.finish().expect("should finish successfully");
            let desc = handle.resolution().describe();
            assert!(
                desc.contains("Text"),
                "Text element should solve to Text variant, got: {}",
                desc
            );
        }
        other => panic!("unexpected result for Text element: {:?}", other),
    }

    // Test Checkbox variant
    let mut solver2 = Solver::new(&schema);
    solver2.see_attribute("name"); // Common field
    let result = solver2.see_element("Checkbox");

    match result {
        KeyResult::Solved(handle) => {
            let desc = handle.resolution().describe();
            assert!(
                desc.contains("Checkbox"),
                "Checkbox element should solve to Checkbox variant, got: {}",
                desc
            );
        }
        KeyResult::Unambiguous { .. } => {
            let handle = solver2.finish().expect("should finish successfully");
            let desc = handle.resolution().describe();
            assert!(
                desc.contains("Checkbox"),
                "Checkbox element should solve to Checkbox variant, got: {}",
                desc
            );
        }
        other => panic!("unexpected result for Checkbox element: {:?}", other),
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
    let content_field = resolution.field_by_name("content");
    assert!(content_field.is_some());
    assert_eq!(
        content_field.unwrap().category,
        FieldCategory::Text,
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
// Test 6: FieldKey type conversions
// ============================================================================

#[test]
fn test_field_key_type_conversions() {
    // &str -> FieldKey (should be Flat)
    let key: FieldKey = "test".into();
    assert!(matches!(key, FieldKey::Flat(_)));
    assert_eq!(key.name(), "test");

    // String -> FieldKey (should be Flat)
    let key: FieldKey = String::from("test").into();
    assert!(matches!(key, FieldKey::Flat(_)));

    // FieldKey constructors
    let attr_key = FieldKey::attribute("class");
    assert!(matches!(
        attr_key,
        FieldKey::Dom(FieldCategory::Attribute, _)
    ));
    assert_eq!(attr_key.name(), "class");
    assert_eq!(attr_key.category(), Some(FieldCategory::Attribute));

    let elem_key = FieldKey::element("div");
    assert!(matches!(elem_key, FieldKey::Dom(FieldCategory::Element, _)));
    assert_eq!(elem_key.name(), "div");

    let text_key = FieldKey::text();
    assert!(matches!(text_key, FieldKey::Dom(FieldCategory::Text, _)));
    assert_eq!(text_key.name(), "");
}
