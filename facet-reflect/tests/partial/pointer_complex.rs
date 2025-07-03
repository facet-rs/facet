use std::sync::Arc;

use facet::Facet;
use facet_reflect::Partial;

#[derive(Facet, Debug, PartialEq)]
struct Person {
    name: String,
    age: u32,
    email: Option<String>,
}

#[test]
fn arc_slice_complex_struct() -> eyre::Result<()> {
    // Test building Arc<[Person]>
    let mut partial = Partial::alloc::<Arc<[Person]>>().unwrap();
    partial.begin_smart_ptr()?;
    partial.begin_list()?;

    // Add first person
    partial.begin_list_item()?;
    partial.set_field("name", "Alice".to_string())?;
    partial.set_field("age", 30u32)?;
    partial.set_field("email", Some("alice@example.com".to_string()))?;

    partial.end()?; // end list item

    // Add second person
    partial.begin_list_item()?;
    partial.set_field("name", "Bob".to_string())?;
    partial.set_field("age", 25u32)?;
    partial.set_field("email", None::<String>)?;

    partial.end()?; // end list item

    // Add third person
    partial.begin_list_item()?;
    partial.set_field("name", "Charlie".to_string())?;
    partial.set_field("age", 35u32)?;
    partial.set_field("email", Some("charlie@example.com".to_string()))?;

    partial.end()?; // end list item

    partial.end()?; // end list/smart pointer
    let built = *partial.build()?;

    // Verify the result
    assert_eq!(built.len(), 3);
    assert_eq!(built[0].name, "Alice");
    assert_eq!(built[0].age, 30);
    assert_eq!(built[0].email, Some("alice@example.com".to_string()));
    assert_eq!(built[1].name, "Bob");
    assert_eq!(built[1].age, 25);
    assert_eq!(built[1].email, None);
    assert_eq!(built[2].name, "Charlie");
    assert_eq!(built[2].age, 35);
    assert_eq!(built[2].email, Some("charlie@example.com".to_string()));

    Ok(())
}

#[derive(Facet, Debug, PartialEq)]
struct NestedStruct {
    id: u64,
    person: Person,
    tags: Vec<String>,
}

#[test]
fn arc_slice_nested_struct() -> eyre::Result<()> {
    // Test building Arc<[NestedStruct]> with nested structures
    let mut partial = Partial::alloc::<Arc<[NestedStruct]>>().unwrap();
    partial.begin_smart_ptr()?;
    partial.begin_list()?;

    // Add first nested struct
    partial.begin_list_item()?;
    partial.set_field("id", 1001u64)?;

    // Set the person field
    partial.begin_field("person")?;
    partial.set_field("name", "David".to_string())?;
    partial.set_field("age", 40u32)?;
    partial.set_field("email", Some("david@example.com".to_string()))?;

    partial.end()?; // end person field

    // Set the tags field
    partial.begin_field("tags")?;
    partial.begin_list()?;
    partial.push("developer".to_string())?;
    partial.push("rust".to_string())?;
    partial.push("senior".to_string())?;
    partial.end()?; // end tags field

    partial.end()?; // end list item

    // Add second nested struct
    partial.begin_list_item()?;
    partial.set_field("id", 1002u64)?;

    partial.begin_field("person")?;
    partial.set_field("name", "Eve".to_string())?;
    partial.set_field("age", 28u32)?;
    partial.set_field("email", None::<String>)?;

    partial.end()?; // end person field

    partial.begin_field("tags")?;
    partial.begin_list()?;
    partial.push("designer".to_string())?;
    partial.push("ui/ux".to_string())?;
    partial.end()?; // end tags field

    partial.end()?; // end list item

    partial.end()?; // end list/smart pointer
    let built = *partial.build()?;

    // Verify the result
    assert_eq!(built.len(), 2);

    assert_eq!(built[0].id, 1001);
    assert_eq!(built[0].person.name, "David");
    assert_eq!(built[0].person.age, 40);
    assert_eq!(built[0].person.email, Some("david@example.com".to_string()));
    assert_eq!(built[0].tags, vec!["developer", "rust", "senior"]);

    assert_eq!(built[1].id, 1002);
    assert_eq!(built[1].person.name, "Eve");
    assert_eq!(built[1].person.age, 28);
    assert_eq!(built[1].person.email, None);
    assert_eq!(built[1].tags, vec!["designer", "ui/ux"]);

    Ok(())
}

#[test]
fn arc_slice_empty() -> eyre::Result<()> {
    // Test building an empty Arc<[Person]>
    let mut partial = Partial::alloc::<Arc<[Person]>>().unwrap();
    partial.begin_smart_ptr()?;
    partial.begin_list()?;
    partial.end()?; // end list/smart pointer
    let built = *partial.build()?;

    // Verify the result is an empty slice
    assert_eq!(built.len(), 0);

    Ok(())
}

#[test]
fn arc_slice_single_element() -> eyre::Result<()> {
    // Test building Arc<[Person]> with just one element
    let mut partial = Partial::alloc::<Arc<[Person]>>().unwrap();
    partial.begin_smart_ptr()?;
    partial.begin_list()?;

    partial.begin_list_item()?;
    partial.set_field("name", "Solo".to_string())?;
    partial.set_field("age", 42u32)?;
    partial.set_field("email", Some("solo@example.com".to_string()))?;

    partial.end()?; // end list item

    partial.end()?; // end list/smart pointer
    let built = *partial.build()?;

    // Verify the result
    assert_eq!(built.len(), 1);
    assert_eq!(built[0].name, "Solo");
    assert_eq!(built[0].age, 42);
    assert_eq!(built[0].email, Some("solo@example.com".to_string()));

    Ok(())
}

#[derive(Facet, Debug, Clone, PartialEq)]
struct CopyableStruct {
    x: i32,
    y: i32,
}

#[test]
fn arc_slice_copyable_struct() -> eyre::Result<()> {
    // Test with a copyable struct
    let mut partial = Partial::alloc::<Arc<[CopyableStruct]>>().unwrap();
    partial.begin_smart_ptr()?;
    partial.begin_list()?;

    // Add multiple elements
    for i in 0..5 {
        partial.begin_list_item()?;
        partial.set_field("x", i * 10)?;
        partial.set_field("y", i * 20)?;

        partial.end()?; // end list item
    }

    partial.end()?; // end list/smart pointer
    let built = *partial.build()?;

    // Verify the result
    assert_eq!(built.len(), 5);
    for i in 0..5 {
        assert_eq!(built[i].x, (i * 10) as i32);
        assert_eq!(built[i].y, (i * 20) as i32);
    }

    Ok(())
}
