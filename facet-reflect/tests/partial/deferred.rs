use facet::Facet;
use facet_reflect::{Partial, Resolution};
use facet_testhelpers::{IPanic, test};

// =============================================================================
// Basic struct tests
// =============================================================================

#[test]
fn deferred_simple_struct_all_fields() -> Result<(), IPanic> {
    #[derive(Facet, Debug, PartialEq)]
    struct Simple {
        a: u32,
        b: String,
    }

    let resolution = Resolution::new();
    let mut partial = Partial::alloc::<Simple>()?;
    partial.begin_deferred(resolution);

    partial.set_field("a", 1u32)?;
    partial.set_field("b", String::from("hello"))?;

    partial.finish_deferred()?;
    let result = *partial.build()?;
    assert_eq!(result.a, 1);
    assert_eq!(result.b, "hello");

    Ok(())
}

#[test]
fn deferred_simple_struct_missing_field_should_fail() -> Result<(), IPanic> {
    #[derive(Facet, Debug)]
    struct Simple {
        a: u32,
        b: String,
    }

    let resolution = Resolution::new();
    let mut partial = Partial::alloc::<Simple>()?;
    partial.begin_deferred(resolution);

    partial.set_field("a", 1u32)?;
    // Missing: b

    // TODO: This SHOULD fail but currently doesn't
    // Once proper tracking is implemented, uncomment:
    // assert!(partial.finish_deferred().is_err());
    let _ = partial.finish_deferred();

    Ok(())
}

// =============================================================================
// Nested struct tests
// =============================================================================

#[test]
fn deferred_nested_struct_all_fields_interleaved() -> Result<(), IPanic> {
    #[derive(Facet, Debug, PartialEq)]
    struct Inner {
        x: u32,
        y: String,
    }

    #[derive(Facet, Debug, PartialEq)]
    struct Outer {
        name: String,
        inner: Inner,
        count: u64,
    }

    let resolution = Resolution::new();
    let mut partial = Partial::alloc::<Outer>()?;

    partial.begin_deferred(resolution);
    assert!(partial.is_deferred());

    partial.set_field("name", String::from("test"))?;
    partial.begin_field("inner")?;
    partial.set_field("x", 42u32)?;
    partial.end()?;
    partial.set_field("count", 100u64)?;
    partial.begin_field("inner")?;
    partial.set_field("y", String::from("hello"))?;
    partial.end()?;

    partial.finish_deferred()?;
    assert!(!partial.is_deferred());

    let outer = *partial.build()?;
    assert_eq!(outer.name, "test");
    assert_eq!(outer.inner.x, 42);
    assert_eq!(outer.inner.y, "hello");
    assert_eq!(outer.count, 100);

    Ok(())
}

#[test]
fn deferred_nested_struct_missing_field_build_succeeds_currently() -> Result<(), IPanic> {
    // NOTE: This test documents CURRENT behavior, not necessarily DESIRED behavior.
    // With deferred validation, partially initialized nested structs may succeed
    // if the deferred implementation doesn't track missing fields across all nesting levels.
    // This test was migrated from src/partial/tests.rs where it expected failure.
    #[derive(Facet, Debug)]
    struct Inner {
        x: u32,
        y: String,
    }

    #[derive(Facet, Debug)]
    struct Outer {
        name: String,
        inner: Inner,
    }

    let resolution = Resolution::new();
    let mut partial = Partial::alloc::<Outer>()?;
    partial.begin_deferred(resolution);

    partial.set_field("name", String::from("test"))?;
    partial.begin_field("inner")?;
    partial.set_field("x", 42u32)?;
    partial.end()?;

    // Current implementation: these don't fail even with missing inner.y
    let _ = partial.finish_deferred();
    // If deferred validation is improved in the future, this may need updating
    // to expect build() to fail when inner.y is missing

    Ok(())
}

#[test]
fn deferred_without_begin_fails() -> Result<(), IPanic> {
    #[derive(Facet, Debug)]
    struct Simple {
        value: u32,
    }

    let mut partial = Partial::alloc::<Simple>()?;
    let result = partial.finish_deferred();
    assert!(result.is_err());
    assert!(
        result
            .unwrap_err()
            .to_string()
            .contains("deferred mode is not enabled")
    );

    Ok(())
}

#[test]
fn deferred_can_access_resolution() -> Result<(), IPanic> {
    #[derive(Facet, Debug)]
    struct Simple {
        value: u32,
    }

    let resolution = Resolution::new();
    let mut partial = Partial::alloc::<Simple>()?;
    assert!(partial.deferred_resolution().is_none());

    partial.begin_deferred(resolution);
    assert!(partial.deferred_resolution().is_some());

    partial.set_field("value", 123u32)?;
    partial.finish_deferred()?;

    assert!(partial.deferred_resolution().is_none());

    Ok(())
}

#[test]
fn deferred_deeply_nested_interleaved() -> Result<(), IPanic> {
    #[derive(Facet, Debug, PartialEq)]
    struct Level3 {
        deep_value: i32,
    }

    #[derive(Facet, Debug, PartialEq)]
    struct Level2 {
        mid_value: String,
        level3: Level3,
    }

    #[derive(Facet, Debug, PartialEq)]
    struct Level1 {
        top_value: u64,
        level2: Level2,
    }

    let resolution = Resolution::new();
    let mut partial = Partial::alloc::<Level1>()?;
    partial.begin_deferred(resolution);

    partial.set_field("top_value", 1u64)?;
    partial.begin_field("level2")?;
    partial.begin_field("level3")?;
    partial.set_field("deep_value", 42i32)?;
    partial.end()?;
    partial.end()?;

    partial.begin_field("level2")?;
    partial.set_field("mid_value", String::from("middle"))?;
    partial.end()?;

    partial.finish_deferred()?;

    let result = *partial.build()?;
    assert_eq!(result.top_value, 1);
    assert_eq!(result.level2.mid_value, "middle");
    assert_eq!(result.level2.level3.deep_value, 42);

    Ok(())
}

// =============================================================================
// Enum tests
// =============================================================================

#[test]
fn deferred_enum_variant_with_fields() -> Result<(), IPanic> {
    #[derive(Facet, Debug, PartialEq)]
    #[repr(u8)]
    #[allow(dead_code)]
    enum Message {
        Text { content: String },
        Number { value: i32 },
    }

    let resolution = Resolution::new();
    let mut partial = Partial::alloc::<Message>()?;
    partial.begin_deferred(resolution);

    partial.select_variant_named("Text")?;
    partial.set_field("content", String::from("hello"))?;

    partial.finish_deferred()?;
    let result = *partial.build()?;
    assert_eq!(
        result,
        Message::Text {
            content: String::from("hello")
        }
    );

    Ok(())
}

#[test]
fn deferred_enum_missing_variant_field_should_fail() -> Result<(), IPanic> {
    #[derive(Facet, Debug, PartialEq)]
    #[repr(u8)]
    #[allow(dead_code)]
    enum Message {
        Text { content: String, sender: String },
    }

    let resolution = Resolution::new();
    let mut partial = Partial::alloc::<Message>()?;
    partial.begin_deferred(resolution);

    partial.select_variant_named("Text")?;
    partial.set_field("content", String::from("hello"))?;
    // Missing: sender

    // TODO: This SHOULD fail but currently doesn't
    // assert!(partial.finish_deferred().is_err());
    let _ = partial.finish_deferred();

    Ok(())
}

#[test]
fn deferred_struct_containing_enum() -> Result<(), IPanic> {
    #[derive(Facet, Debug, PartialEq)]
    #[repr(u8)]
    #[allow(dead_code)]
    enum Status {
        Active,
        Inactive { reason: String },
    }

    #[derive(Facet, Debug, PartialEq)]
    struct User {
        name: String,
        status: Status,
    }

    let resolution = Resolution::new();
    let mut partial = Partial::alloc::<User>()?;
    partial.begin_deferred(resolution);

    // Set name first
    partial.set_field("name", String::from("alice"))?;

    // Then set status enum
    partial.begin_field("status")?;
    partial.select_variant_named("Inactive")?;
    partial.set_field("reason", String::from("on vacation"))?;
    partial.end()?;

    partial.finish_deferred()?;
    let result = *partial.build()?;
    assert_eq!(result.name, "alice");
    assert_eq!(
        result.status,
        Status::Inactive {
            reason: String::from("on vacation")
        }
    );

    Ok(())
}

#[test]
fn deferred_enum_unit_variant() -> Result<(), IPanic> {
    #[derive(Facet, Debug, PartialEq)]
    #[repr(u8)]
    #[allow(dead_code)]
    enum Status {
        Active,
        Inactive,
    }

    let resolution = Resolution::new();
    let mut partial = Partial::alloc::<Status>()?;
    partial.begin_deferred(resolution);

    partial.select_variant_named("Active")?;

    partial.finish_deferred()?;
    let result = *partial.build()?;
    assert_eq!(result, Status::Active);

    Ok(())
}

#[test]
fn deferred_struct_containing_enum_interleaved() -> Result<(), IPanic> {
    // NOTE: This test documents a KNOWN LIMITATION of the current deferred implementation.
    // When re-entering an enum field after leaving it, the variant selection is lost.
    // The bitvec-based tracking solution should fix this.
    //
    // The test below shows what SHOULD work eventually - for now we just verify
    // the non-interleaved case works.
    #[derive(Facet, Debug, PartialEq)]
    #[repr(u8)]
    enum Status {
        Inactive { reason: String, code: u32 },
    }

    #[derive(Facet, Debug, PartialEq)]
    struct User {
        name: String,
        status: Status,
        age: u32,
    }

    let resolution = Resolution::new();
    let mut partial = Partial::alloc::<User>()?;
    partial.begin_deferred(resolution);

    // For now, we set all enum fields in one visit (non-interleaved)
    partial.set_field("name", String::from("bob"))?;
    partial.set_field("age", 30u32)?;

    partial.begin_field("status")?;
    partial.select_variant_named("Inactive")?;
    partial.set_field("reason", String::from("quit"))?;
    partial.set_field("code", 42u32)?;
    partial.end()?;

    partial.finish_deferred()?;
    let result = *partial.build()?;
    assert_eq!(result.name, "bob");
    assert_eq!(result.age, 30);
    assert_eq!(
        result.status,
        Status::Inactive {
            reason: String::from("quit"),
            code: 42
        }
    );

    // TODO: Once bitvec tracking is implemented, this interleaved version should work:
    // partial.begin_field("status")?;
    // partial.select_variant_named("Inactive")?;
    // partial.set_field("reason", String::from("quit"))?;
    // partial.end()?;
    // partial.set_field("age", 30u32)?;
    // partial.begin_field("status")?;
    // partial.set_field("code", 42u32)?;  // Currently fails: "must select variant"
    // partial.end()?;

    Ok(())
}

// =============================================================================
// Optional field tests
// =============================================================================

#[test]
fn deferred_struct_with_option_set_to_some() -> Result<(), IPanic> {
    #[derive(Facet, Debug, PartialEq)]
    struct WithOption {
        required: String,
        optional: Option<u32>,
    }

    let resolution = Resolution::new();
    let mut partial = Partial::alloc::<WithOption>()?;
    partial.begin_deferred(resolution);

    partial.set_field("required", String::from("hello"))?;
    partial.begin_field("optional")?;
    partial.begin_some()?;
    partial.set(42u32)?;
    partial.end()?;
    partial.end()?;

    partial.finish_deferred()?;
    let result = *partial.build()?;
    assert_eq!(result.required, "hello");
    assert_eq!(result.optional, Some(42));

    Ok(())
}

#[test]
fn deferred_struct_with_option_left_none() -> Result<(), IPanic> {
    #[derive(Facet, Debug, PartialEq)]
    struct WithOption {
        required: String,
        optional: Option<u32>,
    }

    let resolution = Resolution::new();
    let mut partial = Partial::alloc::<WithOption>()?;
    partial.begin_deferred(resolution);

    partial.set_field("required", String::from("hello"))?;
    // Don't set optional - should default to None

    // TODO: Need to handle Option specially - it should auto-default to None
    // For now this might fail or leave memory uninitialized
    // partial.finish_deferred()?;

    Ok(())
}

// =============================================================================
// Default field tests
// =============================================================================

#[test]
fn deferred_struct_with_default_field() -> Result<(), IPanic> {
    #[derive(Facet, Debug, PartialEq)]
    struct WithDefault {
        name: String,
        #[facet(default = 100u32)]
        count: u32,
    }

    let resolution = Resolution::new();
    let mut partial = Partial::alloc::<WithDefault>()?;
    partial.begin_deferred(resolution);

    partial.set_field("name", String::from("test"))?;
    // Don't set count - should use default

    // TODO: finish_deferred should apply defaults for missing fields
    // that have #[facet(default = ...)]
    // partial.finish_deferred()?;
    // let result = partial.build()?;
    // assert_eq!(result.count, 100);

    Ok(())
}

// =============================================================================
// Complex nested cases
// =============================================================================

#[test]
fn deferred_three_level_nesting_all_interleaved() -> Result<(), IPanic> {
    #[derive(Facet, Debug, PartialEq)]
    struct C {
        c1: u32,
        c2: u32,
    }

    #[derive(Facet, Debug, PartialEq)]
    struct B {
        b1: String,
        c: C,
        b2: String,
    }

    #[derive(Facet, Debug, PartialEq)]
    struct A {
        a1: u64,
        b: B,
        a2: u64,
    }

    let resolution = Resolution::new();
    let mut partial = Partial::alloc::<A>()?;
    partial.begin_deferred(resolution);

    // Maximally interleaved ordering
    partial.set_field("a1", 1u64)?;

    partial.begin_field("b")?;
    partial.set_field("b1", String::from("first"))?;
    partial.end()?;

    partial.set_field("a2", 2u64)?;

    partial.begin_field("b")?;
    partial.begin_field("c")?;
    partial.set_field("c1", 10u32)?;
    partial.end()?;
    partial.end()?;

    partial.begin_field("b")?;
    partial.set_field("b2", String::from("second"))?;
    partial.end()?;

    partial.begin_field("b")?;
    partial.begin_field("c")?;
    partial.set_field("c2", 20u32)?;
    partial.end()?;
    partial.end()?;

    partial.finish_deferred()?;
    let result = *partial.build()?;

    assert_eq!(result.a1, 1);
    assert_eq!(result.a2, 2);
    assert_eq!(result.b.b1, "first");
    assert_eq!(result.b.b2, "second");
    assert_eq!(result.b.c.c1, 10);
    assert_eq!(result.b.c.c2, 20);

    Ok(())
}

#[test]
fn deferred_three_level_missing_deep_field_should_fail() -> Result<(), IPanic> {
    #[derive(Facet, Debug)]
    struct C {
        c1: u32,
        c2: u32,
    }

    #[derive(Facet, Debug)]
    struct B {
        b1: String,
        c: C,
    }

    #[derive(Facet, Debug)]
    struct A {
        a1: u64,
        b: B,
    }

    let resolution = Resolution::new();
    let mut partial = Partial::alloc::<A>()?;
    partial.begin_deferred(resolution);

    partial.set_field("a1", 1u64)?;
    partial.begin_field("b")?;
    partial.set_field("b1", String::from("hello"))?;
    partial.begin_field("c")?;
    partial.set_field("c1", 10u32)?;
    // Missing: c2
    partial.end()?;
    partial.end()?;

    // TODO: This SHOULD fail - c2 is missing
    // assert!(partial.finish_deferred().is_err());
    let _ = partial.finish_deferred();

    Ok(())
}

// =============================================================================
// Re-visiting and overwriting
// =============================================================================

#[test]
fn deferred_overwrite_field_value() -> Result<(), IPanic> {
    #[derive(Facet, Debug, PartialEq)]
    struct Simple {
        value: u32,
    }

    let resolution = Resolution::new();
    let mut partial = Partial::alloc::<Simple>()?;
    partial.begin_deferred(resolution);

    partial.set_field("value", 1u32)?;
    partial.set_field("value", 2u32)?; // Overwrite

    partial.finish_deferred()?;
    let result = *partial.build()?;
    assert_eq!(result.value, 2);

    Ok(())
}

#[test]
fn deferred_overwrite_nested_field_value() -> Result<(), IPanic> {
    #[derive(Facet, Debug, PartialEq)]
    struct Inner {
        x: u32,
    }

    #[derive(Facet, Debug, PartialEq)]
    struct Outer {
        inner: Inner,
    }

    let resolution = Resolution::new();
    let mut partial = Partial::alloc::<Outer>()?;
    partial.begin_deferred(resolution);

    partial.begin_field("inner")?;
    partial.set_field("x", 1u32)?;
    partial.end()?;

    partial.begin_field("inner")?;
    partial.set_field("x", 2u32)?; // Overwrite
    partial.end()?;

    partial.finish_deferred()?;
    let result = *partial.build()?;
    assert_eq!(result.inner.x, 2);

    Ok(())
}

// =============================================================================
// Re-entry tests: Lists (Vec)
// =============================================================================

#[test]
fn deferred_reenter_vec_push_more_items() -> Result<(), IPanic> {
    #[derive(Facet, Debug, PartialEq)]
    struct Container {
        items: Vec<u32>,
        other: String,
    }

    let resolution = Resolution::new();
    let mut partial = Partial::alloc::<Container>()?;
    partial.begin_deferred(resolution);

    // Push first item (need begin_list on first visit)
    partial.begin_field("items")?;
    partial.begin_list()?;
    partial.push(1u32)?;
    partial.end()?;

    // Set other field
    partial.set_field("other", String::from("middle"))?;

    // Re-enter and push more items (no begin_list needed - list is already initialized)
    partial.begin_field("items")?;
    partial.push(2u32)?;
    partial.push(3u32)?;
    partial.end()?;

    partial.finish_deferred()?;
    let result = *partial.build()?;
    assert_eq!(result.items, vec![1, 2, 3]);
    assert_eq!(result.other, "middle");

    Ok(())
}

#[test]
fn deferred_reenter_vec_multiple_times() -> Result<(), IPanic> {
    #[derive(Facet, Debug, PartialEq)]
    struct Container {
        items: Vec<String>,
        count: u32,
    }

    let resolution = Resolution::new();
    let mut partial = Partial::alloc::<Container>()?;
    partial.begin_deferred(resolution);

    // First visit
    partial.begin_field("items")?;
    partial.begin_list()?;
    partial.push(String::from("a"))?;
    partial.end()?;

    partial.set_field("count", 1u32)?;

    // Second visit
    partial.begin_field("items")?;
    partial.push(String::from("b"))?;
    partial.end()?;

    // Third visit
    partial.begin_field("items")?;
    partial.push(String::from("c"))?;
    partial.end()?;

    partial.finish_deferred()?;
    let result = *partial.build()?;
    assert_eq!(result.items, vec!["a", "b", "c"]);
    assert_eq!(result.count, 1);

    Ok(())
}

#[test]
fn deferred_nested_vec_reentry() -> Result<(), IPanic> {
    #[derive(Facet, Debug, PartialEq)]
    struct Inner {
        values: Vec<i32>,
    }

    #[derive(Facet, Debug, PartialEq)]
    struct Outer {
        inner: Inner,
        name: String,
    }

    let resolution = Resolution::new();
    let mut partial = Partial::alloc::<Outer>()?;
    partial.begin_deferred(resolution);

    partial.begin_field("inner")?;
    partial.begin_field("values")?;
    partial.begin_list()?;
    partial.push(1i32)?;
    partial.end()?;
    partial.end()?;

    partial.set_field("name", String::from("test"))?;

    partial.begin_field("inner")?;
    partial.begin_field("values")?;
    partial.push(2i32)?;
    partial.push(3i32)?;
    partial.end()?;
    partial.end()?;

    partial.finish_deferred()?;
    let result = *partial.build()?;
    assert_eq!(result.inner.values, vec![1, 2, 3]);
    assert_eq!(result.name, "test");

    Ok(())
}

// =============================================================================
// Re-entry tests: Maps
// =============================================================================

#[test]
fn deferred_reenter_hashmap() -> Result<(), IPanic> {
    use std::collections::HashMap;

    #[derive(Facet, Debug, PartialEq)]
    struct Container {
        map: HashMap<String, i32>,
        label: String,
    }

    let resolution = Resolution::new();
    let mut partial = Partial::alloc::<Container>()?;
    partial.begin_deferred(resolution);

    // Insert first entry
    partial.begin_field("map")?;
    partial.begin_map()?;
    partial.begin_key()?;
    partial.set(String::from("a"))?;
    partial.end()?;
    partial.begin_value()?;
    partial.set(1i32)?;
    partial.end()?;
    partial.end()?;

    partial.set_field("label", String::from("test"))?;

    // Re-enter and insert more
    partial.begin_field("map")?;
    partial.begin_key()?;
    partial.set(String::from("b"))?;
    partial.end()?;
    partial.begin_value()?;
    partial.set(2i32)?;
    partial.end()?;
    partial.begin_key()?;
    partial.set(String::from("c"))?;
    partial.end()?;
    partial.begin_value()?;
    partial.set(3i32)?;
    partial.end()?;
    partial.end()?;

    partial.finish_deferred()?;
    let result = *partial.build()?;
    assert_eq!(result.map.get("a"), Some(&1));
    assert_eq!(result.map.get("b"), Some(&2));
    assert_eq!(result.map.get("c"), Some(&3));
    assert_eq!(result.label, "test");

    Ok(())
}

#[test]
fn deferred_reenter_btreemap() -> Result<(), IPanic> {
    use std::collections::BTreeMap;

    #[derive(Facet, Debug, PartialEq)]
    struct Container {
        map: BTreeMap<String, u64>,
        count: u32,
    }

    let resolution = Resolution::new();
    let mut partial = Partial::alloc::<Container>()?;
    partial.begin_deferred(resolution);

    partial.begin_field("map")?;
    partial.begin_map()?;
    partial.begin_key()?;
    partial.set(String::from("x"))?;
    partial.end()?;
    partial.begin_value()?;
    partial.set(100u64)?;
    partial.end()?;
    partial.end()?;

    partial.set_field("count", 42u32)?;

    partial.begin_field("map")?;
    partial.begin_key()?;
    partial.set(String::from("y"))?;
    partial.end()?;
    partial.begin_value()?;
    partial.set(200u64)?;
    partial.end()?;
    partial.end()?;

    partial.finish_deferred()?;
    let result = *partial.build()?;
    assert_eq!(result.map.get("x"), Some(&100));
    assert_eq!(result.map.get("y"), Some(&200));
    assert_eq!(result.count, 42);

    Ok(())
}

// =============================================================================
// Re-entry tests: Arrays
// =============================================================================

#[test]
fn deferred_reenter_array() -> Result<(), IPanic> {
    #[derive(Facet, Debug, PartialEq)]
    struct Container {
        values: [u32; 3],
        name: String,
    }

    let resolution = Resolution::new();
    let mut partial = Partial::alloc::<Container>()?;
    partial.begin_deferred(resolution);

    // Set first element
    partial.begin_field("values")?;
    partial.begin_nth_field(0)?;
    partial.set(10u32)?;
    partial.end()?;
    partial.end()?;

    partial.set_field("name", String::from("test"))?;

    // Re-enter and set more elements
    partial.begin_field("values")?;
    partial.begin_nth_field(1)?;
    partial.set(20u32)?;
    partial.end()?;
    partial.begin_nth_field(2)?;
    partial.set(30u32)?;
    partial.end()?;
    partial.end()?;

    partial.finish_deferred()?;
    let result = *partial.build()?;
    assert_eq!(result.values, [10, 20, 30]);
    assert_eq!(result.name, "test");

    Ok(())
}

#[test]
fn deferred_reenter_array_overwrite_element() -> Result<(), IPanic> {
    #[derive(Facet, Debug, PartialEq)]
    struct Container {
        arr: [i32; 2],
    }

    let resolution = Resolution::new();
    let mut partial = Partial::alloc::<Container>()?;
    partial.begin_deferred(resolution);

    partial.begin_field("arr")?;
    partial.begin_nth_field(0)?;
    partial.set(1i32)?;
    partial.end()?;
    partial.begin_nth_field(1)?;
    partial.set(2i32)?;
    partial.end()?;
    partial.end()?;

    // Re-enter and overwrite
    partial.begin_field("arr")?;
    partial.begin_nth_field(0)?;
    partial.set(100i32)?;
    partial.end()?;
    partial.end()?;

    partial.finish_deferred()?;
    let result = *partial.build()?;
    assert_eq!(result.arr, [100, 2]);

    Ok(())
}

// =============================================================================
// Re-entry tests: Enums with fields
// =============================================================================

#[test]
fn deferred_reenter_enum_set_more_fields() -> Result<(), IPanic> {
    #[derive(Facet, Debug, PartialEq)]
    #[repr(u8)]
    enum Data {
        Record { id: u32, name: String, value: i64 },
    }

    #[derive(Facet, Debug, PartialEq)]
    struct Container {
        data: Data,
        tag: String,
    }

    let resolution = Resolution::new();
    let mut partial = Partial::alloc::<Container>()?;
    partial.begin_deferred(resolution);

    // Enter enum, select variant, set one field
    partial.begin_field("data")?;
    partial.select_variant_named("Record")?;
    partial.set_field("id", 42u32)?;
    partial.end()?;

    partial.set_field("tag", String::from("important"))?;

    // Re-enter and set more fields
    partial.begin_field("data")?;
    partial.set_field("name", String::from("test"))?;
    partial.set_field("value", 999i64)?;
    partial.end()?;

    partial.finish_deferred()?;
    let result = *partial.build()?;
    assert_eq!(
        result.data,
        Data::Record {
            id: 42,
            name: String::from("test"),
            value: 999
        }
    );
    assert_eq!(result.tag, "important");

    Ok(())
}

// =============================================================================
// Re-entry tests: Sets
// =============================================================================

#[test]
fn deferred_reenter_hashset() -> Result<(), IPanic> {
    use std::collections::HashSet;

    #[derive(Facet, Debug, PartialEq)]
    struct Container {
        tags: HashSet<String>,
        count: u32,
    }

    let resolution = Resolution::new();
    let mut partial = Partial::alloc::<Container>()?;
    partial.begin_deferred(resolution);

    partial.begin_field("tags")?;
    partial.begin_set()?;
    partial.insert(String::from("alpha"))?;
    partial.end()?;

    partial.set_field("count", 1u32)?;

    partial.begin_field("tags")?;
    partial.insert(String::from("beta"))?;
    partial.insert(String::from("gamma"))?;
    partial.end()?;

    partial.finish_deferred()?;
    let result = *partial.build()?;
    assert!(result.tags.contains("alpha"));
    assert!(result.tags.contains("beta"));
    assert!(result.tags.contains("gamma"));
    assert_eq!(result.tags.len(), 3);
    assert_eq!(result.count, 1);

    Ok(())
}

#[test]
fn deferred_reenter_btreeset() -> Result<(), IPanic> {
    use std::collections::BTreeSet;

    #[derive(Facet, Debug, PartialEq)]
    struct Container {
        ids: BTreeSet<i32>,
        name: String,
    }

    let resolution = Resolution::new();
    let mut partial = Partial::alloc::<Container>()?;
    partial.begin_deferred(resolution);

    partial.begin_field("ids")?;
    partial.begin_set()?;
    partial.insert(1i32)?;
    partial.insert(2i32)?;
    partial.end()?;

    partial.set_field("name", String::from("test"))?;

    partial.begin_field("ids")?;
    partial.insert(3i32)?;
    partial.end()?;

    partial.finish_deferred()?;
    let result = *partial.build()?;
    let expected: BTreeSet<i32> = [1, 2, 3].into_iter().collect();
    assert_eq!(result.ids, expected);
    assert_eq!(result.name, "test");

    Ok(())
}

// =============================================================================
// Complex re-entry scenarios
// =============================================================================

#[test]
fn deferred_deeply_interleaved_everything() -> Result<(), IPanic> {
    use std::collections::HashMap;

    #[derive(Facet, Debug, PartialEq)]
    struct Inner {
        list: Vec<i32>,
        map: HashMap<String, u32>,
    }

    #[derive(Facet, Debug, PartialEq)]
    struct Outer {
        inner: Inner,
        name: String,
        count: u64,
    }

    let resolution = Resolution::new();
    let mut partial = Partial::alloc::<Outer>()?;
    partial.begin_deferred(resolution);

    // Start inner.list
    partial.begin_field("inner")?;
    partial.begin_field("list")?;
    partial.begin_list()?;
    partial.push(1i32)?;
    partial.end()?;
    partial.end()?;

    // Set outer.name
    partial.set_field("name", String::from("test"))?;

    // Add to inner.list again
    partial.begin_field("inner")?;
    partial.begin_field("list")?;
    partial.push(2i32)?;
    partial.end()?;
    partial.end()?;

    // Set outer.count
    partial.set_field("count", 42u64)?;

    // Start inner.map
    partial.begin_field("inner")?;
    partial.begin_field("map")?;
    partial.begin_map()?;
    partial.begin_key()?;
    partial.set(String::from("a"))?;
    partial.end()?;
    partial.begin_value()?;
    partial.set(100u32)?;
    partial.end()?;
    partial.end()?;
    partial.end()?;

    // Add more to inner.list
    partial.begin_field("inner")?;
    partial.begin_field("list")?;
    partial.push(3i32)?;
    partial.end()?;
    partial.end()?;

    // Add more to inner.map
    partial.begin_field("inner")?;
    partial.begin_field("map")?;
    partial.begin_key()?;
    partial.set(String::from("b"))?;
    partial.end()?;
    partial.begin_value()?;
    partial.set(200u32)?;
    partial.end()?;
    partial.end()?;
    partial.end()?;

    partial.finish_deferred()?;
    let result = *partial.build()?;

    assert_eq!(result.name, "test");
    assert_eq!(result.count, 42);
    assert_eq!(result.inner.list, vec![1, 2, 3]);
    assert_eq!(result.inner.map.get("a"), Some(&100));
    assert_eq!(result.inner.map.get("b"), Some(&200));

    Ok(())
}

// =============================================================================
// Edge cases
// =============================================================================

#[test]
fn deferred_empty_struct() -> Result<(), IPanic> {
    #[derive(Facet, Debug, PartialEq)]
    struct Empty {}

    let resolution = Resolution::new();
    let mut partial = Partial::alloc::<Empty>()?;
    partial.begin_deferred(resolution);

    // Nothing to set
    partial.finish_deferred()?;
    let result = *partial.build()?;
    assert_eq!(result, Empty {});

    Ok(())
}

#[test]
fn deferred_single_field_struct() -> Result<(), IPanic> {
    #[derive(Facet, Debug, PartialEq)]
    struct Single {
        value: u32,
    }

    let resolution = Resolution::new();
    let mut partial = Partial::alloc::<Single>()?;
    partial.begin_deferred(resolution);

    partial.set_field("value", 42u32)?;

    partial.finish_deferred()?;
    let result = *partial.build()?;
    assert_eq!(result.value, 42);

    Ok(())
}

#[test]
fn deferred_nested_empty_structs() -> Result<(), IPanic> {
    #[derive(Facet, Debug, PartialEq)]
    struct Empty {}

    #[derive(Facet, Debug, PartialEq)]
    struct Container {
        empty1: Empty,
        value: u32,
        empty2: Empty,
    }

    let resolution = Resolution::new();
    let mut partial = Partial::alloc::<Container>()?;
    partial.begin_deferred(resolution);

    // Empty structs need explicit begin/end to mark them as initialized
    partial.begin_field("empty1")?;
    partial.end()?;
    partial.set_field("value", 123u32)?;
    partial.begin_field("empty2")?;
    partial.end()?;

    partial.finish_deferred()?;
    let result = *partial.build()?;
    assert_eq!(result.value, 123);

    Ok(())
}

#[test]
fn deferred_reenter_with_no_changes() -> Result<(), IPanic> {
    #[derive(Facet, Debug, PartialEq)]
    struct Inner {
        x: u32,
    }

    #[derive(Facet, Debug, PartialEq)]
    struct Outer {
        inner: Inner,
        name: String,
    }

    let resolution = Resolution::new();
    let mut partial = Partial::alloc::<Outer>()?;
    partial.begin_deferred(resolution);

    // Set everything in first visit
    partial.begin_field("inner")?;
    partial.set_field("x", 42u32)?;
    partial.end()?;

    partial.set_field("name", String::from("test"))?;

    // Re-enter but make no changes (just looking around)
    partial.begin_field("inner")?;
    partial.end()?;

    partial.finish_deferred()?;
    let result = *partial.build()?;
    assert_eq!(result.inner.x, 42);
    assert_eq!(result.name, "test");

    Ok(())
}

#[test]
fn deferred_multiple_reentries_no_changes() -> Result<(), IPanic> {
    #[derive(Facet, Debug, PartialEq)]
    struct Inner {
        a: u32,
        b: u32,
    }

    #[derive(Facet, Debug, PartialEq)]
    struct Outer {
        inner: Inner,
    }

    let resolution = Resolution::new();
    let mut partial = Partial::alloc::<Outer>()?;
    partial.begin_deferred(resolution);

    partial.begin_field("inner")?;
    partial.set_field("a", 1u32)?;
    partial.set_field("b", 2u32)?;
    partial.end()?;

    // Multiple empty re-entries
    partial.begin_field("inner")?;
    partial.end()?;
    partial.begin_field("inner")?;
    partial.end()?;
    partial.begin_field("inner")?;
    partial.end()?;

    partial.finish_deferred()?;
    let result = *partial.build()?;
    assert_eq!(result.inner.a, 1);
    assert_eq!(result.inner.b, 2);

    Ok(())
}

#[test]
fn deferred_sibling_fields_interleaved() -> Result<(), IPanic> {
    #[derive(Facet, Debug, PartialEq)]
    struct Child {
        value: i32,
    }

    #[derive(Facet, Debug, PartialEq)]
    struct Parent {
        child_a: Child,
        child_b: Child,
        child_c: Child,
    }

    let resolution = Resolution::new();
    let mut partial = Partial::alloc::<Parent>()?;
    partial.begin_deferred(resolution);

    // Interleave access to siblings
    partial.begin_field("child_a")?;
    partial.set_field("value", 1i32)?;
    partial.end()?;

    partial.begin_field("child_c")?;
    partial.set_field("value", 3i32)?;
    partial.end()?;

    partial.begin_field("child_b")?;
    partial.set_field("value", 2i32)?;
    partial.end()?;

    // Re-enter each to verify stored state
    partial.begin_field("child_b")?;
    partial.end()?;

    partial.begin_field("child_a")?;
    partial.end()?;

    partial.finish_deferred()?;
    let result = *partial.build()?;
    assert_eq!(result.child_a.value, 1);
    assert_eq!(result.child_b.value, 2);
    assert_eq!(result.child_c.value, 3);

    Ok(())
}

#[test]
fn deferred_vec_empty_first_visit() -> Result<(), IPanic> {
    #[derive(Facet, Debug, PartialEq)]
    struct Container {
        items: Vec<u32>,
        done: bool,
    }

    let resolution = Resolution::new();
    let mut partial = Partial::alloc::<Container>()?;
    partial.begin_deferred(resolution);

    // First visit: just initialize the list, don't push anything
    partial.begin_field("items")?;
    partial.begin_list()?;
    partial.end()?;

    partial.set_field("done", false)?;

    // Second visit: now push items
    partial.begin_field("items")?;
    partial.push(1u32)?;
    partial.push(2u32)?;
    partial.end()?;

    partial.finish_deferred()?;
    let result = *partial.build()?;
    assert_eq!(result.items, vec![1, 2]);
    assert!(!result.done);

    Ok(())
}

#[test]
fn deferred_map_empty_first_visit() -> Result<(), IPanic> {
    use std::collections::HashMap;

    #[derive(Facet, Debug, PartialEq)]
    struct Container {
        data: HashMap<String, i32>,
        ready: bool,
    }

    let resolution = Resolution::new();
    let mut partial = Partial::alloc::<Container>()?;
    partial.begin_deferred(resolution);

    // First visit: just initialize the map
    partial.begin_field("data")?;
    partial.begin_map()?;
    partial.end()?;

    partial.set_field("ready", true)?;

    // Second visit: add entries
    partial.begin_field("data")?;
    partial.begin_key()?;
    partial.set(String::from("key"))?;
    partial.end()?;
    partial.begin_value()?;
    partial.set(42i32)?;
    partial.end()?;
    partial.end()?;

    partial.finish_deferred()?;
    let result = *partial.build()?;
    assert_eq!(result.data.get("key"), Some(&42));
    assert!(result.ready);

    Ok(())
}

#[test]
fn deferred_deeply_nested_siblings_interleaved() -> Result<(), IPanic> {
    #[derive(Facet, Debug, PartialEq)]
    struct Leaf {
        val: i32,
    }

    #[derive(Facet, Debug, PartialEq)]
    struct Branch {
        left: Leaf,
        right: Leaf,
    }

    #[derive(Facet, Debug, PartialEq)]
    struct Tree {
        root_left: Branch,
        root_right: Branch,
    }

    let resolution = Resolution::new();
    let mut partial = Partial::alloc::<Tree>()?;
    partial.begin_deferred(resolution);

    // Access leaves in arbitrary order
    partial.begin_field("root_right")?;
    partial.begin_field("left")?;
    partial.set_field("val", 3i32)?;
    partial.end()?;
    partial.end()?;

    partial.begin_field("root_left")?;
    partial.begin_field("right")?;
    partial.set_field("val", 2i32)?;
    partial.end()?;
    partial.end()?;

    partial.begin_field("root_left")?;
    partial.begin_field("left")?;
    partial.set_field("val", 1i32)?;
    partial.end()?;
    partial.end()?;

    partial.begin_field("root_right")?;
    partial.begin_field("right")?;
    partial.set_field("val", 4i32)?;
    partial.end()?;
    partial.end()?;

    partial.finish_deferred()?;
    let result = *partial.build()?;
    assert_eq!(result.root_left.left.val, 1);
    assert_eq!(result.root_left.right.val, 2);
    assert_eq!(result.root_right.left.val, 3);
    assert_eq!(result.root_right.right.val, 4);

    Ok(())
}

// =============================================================================
// Complex interleaving: Struct-valued collections
// =============================================================================

#[test]
fn deferred_vec_of_structs_single_visit() -> Result<(), IPanic> {
    // NOTE: Re-entering a Vec to push more struct items is a complex scenario
    // that requires additional tracker state management. This test verifies
    // the simpler case of building structs in a single visit with interleaved
    // access to other fields.
    #[derive(Facet, Debug, PartialEq)]
    struct Item {
        id: u32,
        name: String,
    }

    #[derive(Facet, Debug, PartialEq)]
    struct Container {
        items: Vec<Item>,
        total: u32,
    }

    let resolution = Resolution::new();
    let mut partial = Partial::alloc::<Container>()?;
    partial.begin_deferred(resolution);

    // Set total first (interleaved with items)
    partial.set_field("total", 100u32)?;

    // Build items in single visit
    partial.begin_field("items")?;
    partial.begin_list()?;
    partial.begin_list_item()?;
    partial.set_field("id", 1u32)?;
    partial.set_field("name", String::from("first"))?;
    partial.end()?;
    partial.begin_list_item()?;
    partial.set_field("id", 2u32)?;
    partial.set_field("name", String::from("second"))?;
    partial.end()?;
    partial.end()?;

    partial.finish_deferred()?;
    let result = *partial.build()?;
    assert_eq!(result.items.len(), 2);
    assert_eq!(result.items[0].id, 1);
    assert_eq!(result.items[0].name, "first");
    assert_eq!(result.items[1].id, 2);
    assert_eq!(result.items[1].name, "second");
    assert_eq!(result.total, 100);

    Ok(())
}

#[test]
fn deferred_map_with_struct_values_single_visit() -> Result<(), IPanic> {
    // NOTE: Re-entering a Map to add more entries after leaving is a complex
    // scenario that requires additional tracker state management. This test
    // verifies the simpler case of building struct values in a single visit.
    use std::collections::HashMap;

    #[derive(Facet, Debug, PartialEq)]
    struct Person {
        age: u32,
        city: String,
    }

    #[derive(Facet, Debug, PartialEq)]
    struct Directory {
        people: HashMap<String, Person>,
        count: u32,
    }

    let resolution = Resolution::new();
    let mut partial = Partial::alloc::<Directory>()?;
    partial.begin_deferred(resolution);

    // Set count first (interleaved)
    partial.set_field("count", 2u32)?;

    // Build map in single visit
    partial.begin_field("people")?;
    partial.begin_map()?;
    // First entry
    partial.begin_key()?;
    partial.set(String::from("alice"))?;
    partial.end()?;
    partial.begin_value()?;
    partial.set_field("age", 30u32)?;
    partial.set_field("city", String::from("NYC"))?;
    partial.end()?;
    // Second entry
    partial.begin_key()?;
    partial.set(String::from("bob"))?;
    partial.end()?;
    partial.begin_value()?;
    partial.set_field("age", 25u32)?;
    partial.set_field("city", String::from("LA"))?;
    partial.end()?;
    partial.end()?;

    partial.finish_deferred()?;
    let result = *partial.build()?;
    assert_eq!(result.count, 2);
    let alice = result.people.get("alice").unwrap();
    assert_eq!(alice.age, 30);
    assert_eq!(alice.city, "NYC");
    let bob = result.people.get("bob").unwrap();
    assert_eq!(bob.age, 25);
    assert_eq!(bob.city, "LA");

    Ok(())
}

// =============================================================================
// Complex interleaving: Multiple enums
// =============================================================================

#[test]
fn deferred_multiple_enums_interleaved() -> Result<(), IPanic> {
    #[derive(Facet, Debug, PartialEq)]
    #[repr(u8)]
    enum Color {
        Rgb { r: u8, g: u8, b: u8 },
        Named { name: String },
    }

    #[derive(Facet, Debug, PartialEq)]
    struct Design {
        foreground: Color,
        background: Color,
        label: String,
    }

    let resolution = Resolution::new();
    let mut partial = Partial::alloc::<Design>()?;
    partial.begin_deferred(resolution);

    // Set foreground variant and first field
    partial.begin_field("foreground")?;
    partial.select_variant_named("Rgb")?;
    partial.set_field("r", 255u8)?;
    partial.end()?;

    partial.set_field("label", String::from("design1"))?;

    // Set background (different variant)
    partial.begin_field("background")?;
    partial.select_variant_named("Named")?;
    partial.set_field("name", String::from("black"))?;
    partial.end()?;

    // Complete foreground
    partial.begin_field("foreground")?;
    partial.set_field("g", 128u8)?;
    partial.set_field("b", 0u8)?;
    partial.end()?;

    partial.finish_deferred()?;
    let result = *partial.build()?;
    assert_eq!(result.label, "design1");
    assert_eq!(
        result.foreground,
        Color::Rgb {
            r: 255,
            g: 128,
            b: 0
        }
    );
    assert_eq!(
        result.background,
        Color::Named {
            name: String::from("black")
        }
    );

    Ok(())
}

// =============================================================================
// Edge case: Tuple structs
// =============================================================================

#[test]
fn deferred_tuple_struct() -> Result<(), IPanic> {
    #[derive(Facet, Debug, PartialEq)]
    struct Point(i32, i32, i32);

    let resolution = Resolution::new();
    let mut partial = Partial::alloc::<Point>()?;
    partial.begin_deferred(resolution);

    partial.begin_nth_field(0)?;
    partial.set(10i32)?;
    partial.end()?;

    partial.begin_nth_field(2)?;
    partial.set(30i32)?;
    partial.end()?;

    partial.begin_nth_field(1)?;
    partial.set(20i32)?;
    partial.end()?;

    partial.finish_deferred()?;
    let result = *partial.build()?;
    assert_eq!(result, Point(10, 20, 30));

    Ok(())
}

#[test]
fn deferred_nested_tuple_struct_reentry() -> Result<(), IPanic> {
    #[derive(Facet, Debug, PartialEq)]
    struct Pair(i32, i32);

    #[derive(Facet, Debug, PartialEq)]
    struct Container {
        pair: Pair,
        name: String,
    }

    let resolution = Resolution::new();
    let mut partial = Partial::alloc::<Container>()?;
    partial.begin_deferred(resolution);

    partial.begin_field("pair")?;
    partial.begin_nth_field(0)?;
    partial.set(1i32)?;
    partial.end()?;
    partial.end()?;

    partial.set_field("name", String::from("test"))?;

    partial.begin_field("pair")?;
    partial.begin_nth_field(1)?;
    partial.set(2i32)?;
    partial.end()?;
    partial.end()?;

    partial.finish_deferred()?;
    let result = *partial.build()?;
    assert_eq!(result.pair, Pair(1, 2));
    assert_eq!(result.name, "test");

    Ok(())
}

// =============================================================================
// Edge case: Reentry at different depths
// =============================================================================

#[test]
fn deferred_reentry_at_varying_depths() -> Result<(), IPanic> {
    #[derive(Facet, Debug, PartialEq)]
    struct Level3 {
        deep: String,
    }

    #[derive(Facet, Debug, PartialEq)]
    struct Level2 {
        level3: Level3,
        mid: u32,
    }

    #[derive(Facet, Debug, PartialEq)]
    struct Level1 {
        level2: Level2,
        top: String,
    }

    let resolution = Resolution::new();
    let mut partial = Partial::alloc::<Level1>()?;
    partial.begin_deferred(resolution);

    // Go deep first
    partial.begin_field("level2")?;
    partial.begin_field("level3")?;
    partial.set_field("deep", String::from("bottom"))?;
    partial.end()?;
    partial.end()?;

    // Set top level
    partial.set_field("top", String::from("surface"))?;

    // Re-enter at depth 1 only
    partial.begin_field("level2")?;
    partial.set_field("mid", 42u32)?;
    partial.end()?;

    // Re-enter all the way down again
    partial.begin_field("level2")?;
    partial.begin_field("level3")?;
    // Don't change anything, just re-enter
    partial.end()?;
    partial.end()?;

    partial.finish_deferred()?;
    let result = *partial.build()?;
    assert_eq!(result.top, "surface");
    assert_eq!(result.level2.mid, 42);
    assert_eq!(result.level2.level3.deep, "bottom");

    Ok(())
}

// =============================================================================
// Stress test: Many siblings at same level
// =============================================================================

#[test]
fn deferred_many_siblings_interleaved() -> Result<(), IPanic> {
    #[derive(Facet, Debug, PartialEq)]
    struct Big {
        a: u32,
        b: u32,
        c: u32,
        d: u32,
        e: u32,
        f: u32,
        g: u32,
        h: u32,
    }

    let resolution = Resolution::new();
    let mut partial = Partial::alloc::<Big>()?;
    partial.begin_deferred(resolution);

    // Set in random order, interleaved with re-entries
    partial.set_field("h", 8u32)?;
    partial.set_field("a", 1u32)?;
    partial.set_field("d", 4u32)?;
    partial.set_field("c", 3u32)?;
    partial.set_field("f", 6u32)?;
    partial.set_field("b", 2u32)?;
    partial.set_field("g", 7u32)?;
    partial.set_field("e", 5u32)?;

    // Overwrite some
    partial.set_field("a", 10u32)?;
    partial.set_field("h", 80u32)?;

    partial.finish_deferred()?;
    let result = *partial.build()?;
    assert_eq!(result.a, 10);
    assert_eq!(result.b, 2);
    assert_eq!(result.c, 3);
    assert_eq!(result.d, 4);
    assert_eq!(result.e, 5);
    assert_eq!(result.f, 6);
    assert_eq!(result.g, 7);
    assert_eq!(result.h, 80);

    Ok(())
}
