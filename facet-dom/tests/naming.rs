//! Tests for naming conventions in facet-dom.

use std::borrow::Cow;

use facet_dom::naming::{dom_key, to_element_name};

#[test]
fn test_type_names() {
    assert_eq!(&*to_element_name("Banana"), "banana");
    assert_eq!(&*to_element_name("MyPlaylist"), "myPlaylist");
    assert_eq!(&*to_element_name("XMLParser"), "xmlParser");
    assert_eq!(&*to_element_name("HTTPSConnection"), "httpsConnection");
}

#[test]
fn test_field_names() {
    assert_eq!(&*to_element_name("field_name"), "fieldName");
    assert_eq!(&*to_element_name("my_field"), "myField");
}

#[test]
fn test_already_lower_camel_borrows() {
    assert!(matches!(to_element_name("banana"), Cow::Borrowed(_)));
    assert!(matches!(to_element_name("myPlaylist"), Cow::Borrowed(_)));
}

#[test]
fn test_needs_conversion_owns() {
    assert!(matches!(to_element_name("Banana"), Cow::Owned(_)));
    assert!(matches!(to_element_name("field_name"), Cow::Owned(_)));
}

#[test]
fn test_dom_key_with_rename() {
    // When name != effective_name, use effective_name as-is
    let result = dom_key("my_field", "customName");
    assert_eq!(&*result, "customName");
    assert!(matches!(result, Cow::Borrowed(_)));
}

#[test]
fn test_dom_key_without_rename() {
    // When name == effective_name, apply lowerCamelCase
    let result = dom_key("my_field", "my_field");
    assert_eq!(&*result, "myField");
}
