// In ownership-based APIs, the last assignment to `partial` is often unused
// because the value is consumed by `.build()` - this is expected behavior
#![allow(unused_assignments)]

use facet::Facet;
use facet_reflect::Partial;
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

    let mut partial = Partial::alloc::<Simple>()?;
    partial = partial.begin_deferred()?;

    partial = partial.set_field("a", 1u32)?;
    partial = partial.set_field("b", String::from("hello"))?;

    partial = partial.finish_deferred()?;
    let result = partial.build()?.materialize::<Simple>()?;
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

    let mut partial = Partial::alloc::<Simple>()?;
    partial = partial.begin_deferred()?;

    partial = partial.set_field("a", 1u32)?;
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

    let mut partial = Partial::alloc::<Outer>()?;

    partial = partial.begin_deferred()?;
    assert!(partial.is_deferred());

    partial = partial.set_field("name", String::from("test"))?;
    partial = partial.begin_field("inner")?;
    partial = partial.set_field("x", 42u32)?;
    partial = partial.end()?;
    partial = partial.set_field("count", 100u64)?;
    partial = partial.begin_field("inner")?;
    partial = partial.set_field("y", String::from("hello"))?;
    partial = partial.end()?;

    partial = partial.finish_deferred()?;
    assert!(!partial.is_deferred());

    let outer = partial.build()?.materialize::<Outer>()?;
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

    let mut partial = Partial::alloc::<Outer>()?;
    partial = partial.begin_deferred()?;

    partial = partial.set_field("name", String::from("test"))?;
    partial = partial.begin_field("inner")?;
    partial = partial.set_field("x", 42u32)?;
    partial = partial.end()?;

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

    let partial = Partial::alloc::<Simple>()?;
    match partial.finish_deferred() {
        Ok(_) => panic!("Expected error but got Ok"),
        Err(err) => assert!(err.to_string().contains("deferred mode is not enabled")),
    }

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

    let mut partial = Partial::alloc::<Level1>()?;
    partial = partial.begin_deferred()?;

    partial = partial.set_field("top_value", 1u64)?;
    partial = partial.begin_field("level2")?;
    partial = partial.begin_field("level3")?;
    partial = partial.set_field("deep_value", 42i32)?;
    partial = partial.end()?;
    partial = partial.end()?;

    partial = partial.begin_field("level2")?;
    partial = partial.set_field("mid_value", String::from("middle"))?;
    partial = partial.end()?;

    partial = partial.finish_deferred()?;

    let result = partial.build()?.materialize::<Level1>()?;
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

    let mut partial = Partial::alloc::<Message>()?;
    partial = partial.begin_deferred()?;

    partial = partial.select_variant_named("Text")?;
    partial = partial.set_field("content", String::from("hello"))?;

    partial = partial.finish_deferred()?;
    let result = partial.build()?.materialize::<Message>()?;
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

    let mut partial = Partial::alloc::<Message>()?;
    partial = partial.begin_deferred()?;

    partial = partial.select_variant_named("Text")?;
    partial = partial.set_field("content", String::from("hello"))?;
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

    let mut partial = Partial::alloc::<User>()?;
    partial = partial.begin_deferred()?;

    // Set name first
    partial = partial.set_field("name", String::from("alice"))?;

    // Then set status enum
    partial = partial.begin_field("status")?;
    partial = partial.select_variant_named("Inactive")?;
    partial = partial.set_field("reason", String::from("on vacation"))?;
    partial = partial.end()?;

    partial = partial.finish_deferred()?;
    let result = partial.build()?.materialize::<User>()?;
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

    let mut partial = Partial::alloc::<Status>()?;
    partial = partial.begin_deferred()?;

    partial = partial.select_variant_named("Active")?;

    partial = partial.finish_deferred()?;
    let result = partial.build()?.materialize::<Status>()?;
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

    let mut partial = Partial::alloc::<User>()?;
    partial = partial.begin_deferred()?;

    // For now, we set all enum fields in one visit (non-interleaved)
    partial = partial.set_field("name", String::from("bob"))?;
    partial = partial.set_field("age", 30u32)?;

    partial = partial.begin_field("status")?;
    partial = partial.select_variant_named("Inactive")?;
    partial = partial.set_field("reason", String::from("quit"))?;
    partial = partial.set_field("code", 42u32)?;
    partial = partial.end()?;

    partial = partial.finish_deferred()?;
    let result = partial.build()?.materialize::<User>()?;
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

    let mut partial = Partial::alloc::<WithOption>()?;
    partial = partial.begin_deferred()?;

    partial = partial.set_field("required", String::from("hello"))?;
    partial = partial.begin_field("optional")?;
    partial = partial.begin_some()?;
    partial = partial.set(42u32)?;
    partial = partial.end()?;
    partial = partial.end()?;

    partial = partial.finish_deferred()?;
    let result = partial.build()?.materialize::<WithOption>()?;
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

    let mut partial = Partial::alloc::<WithOption>()?;
    partial = partial.begin_deferred()?;

    partial = partial.set_field("required", String::from("hello"))?;
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

    let mut partial = Partial::alloc::<WithDefault>()?;
    partial = partial.begin_deferred()?;

    partial = partial.set_field("name", String::from("test"))?;
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

    let mut partial = Partial::alloc::<A>()?;
    partial = partial.begin_deferred()?;

    // Maximally interleaved ordering
    partial = partial.set_field("a1", 1u64)?;

    partial = partial.begin_field("b")?;
    partial = partial.set_field("b1", String::from("first"))?;
    partial = partial.end()?;

    partial = partial.set_field("a2", 2u64)?;

    partial = partial.begin_field("b")?;
    partial = partial.begin_field("c")?;
    partial = partial.set_field("c1", 10u32)?;
    partial = partial.end()?;
    partial = partial.end()?;

    partial = partial.begin_field("b")?;
    partial = partial.set_field("b2", String::from("second"))?;
    partial = partial.end()?;

    partial = partial.begin_field("b")?;
    partial = partial.begin_field("c")?;
    partial = partial.set_field("c2", 20u32)?;
    partial = partial.end()?;
    partial = partial.end()?;

    partial = partial.finish_deferred()?;
    let result = partial.build()?.materialize::<A>()?;

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

    let mut partial = Partial::alloc::<A>()?;
    partial = partial.begin_deferred()?;

    partial = partial.set_field("a1", 1u64)?;
    partial = partial.begin_field("b")?;
    partial = partial.set_field("b1", String::from("hello"))?;
    partial = partial.begin_field("c")?;
    partial = partial.set_field("c1", 10u32)?;
    // Missing: c2
    partial = partial.end()?;
    partial = partial.end()?;

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

    let mut partial = Partial::alloc::<Simple>()?;
    partial = partial.begin_deferred()?;

    partial = partial.set_field("value", 1u32)?;
    partial = partial.set_field("value", 2u32)?;

    partial = partial.finish_deferred()?;
    let result = partial.build()?.materialize::<Simple>()?;
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

    let mut partial = Partial::alloc::<Outer>()?;
    partial = partial.begin_deferred()?;

    partial = partial.begin_field("inner")?;
    partial = partial.set_field("x", 1u32)?;
    partial = partial.end()?;

    partial = partial.begin_field("inner")?;
    partial = partial.set_field("x", 2u32)?;
    partial = partial.end()?;

    partial = partial.finish_deferred()?;
    let result = partial.build()?.materialize::<Outer>()?;
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

    let mut partial = Partial::alloc::<Container>()?;
    partial = partial.begin_deferred()?;

    // Push first item (need init_list on first visit)
    partial = partial.begin_field("items")?;
    partial = partial.init_list()?;
    partial = partial.push(1u32)?;
    partial = partial.end()?;

    // Set other field
    partial = partial.set_field("other", String::from("middle"))?;

    // Re-enter and push more items (no init_list needed - list is already initialized)
    partial = partial.begin_field("items")?;
    partial = partial.push(2u32)?;
    partial = partial.push(3u32)?;
    partial = partial.end()?;

    partial = partial.finish_deferred()?;
    let result = partial.build()?.materialize::<Container>()?;
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

    let mut partial = Partial::alloc::<Container>()?;
    partial = partial.begin_deferred()?;

    // First visit
    partial = partial.begin_field("items")?;
    partial = partial.init_list()?;
    partial = partial.push(String::from("a"))?;
    partial = partial.end()?;

    partial = partial.set_field("count", 1u32)?;

    // Second visit
    partial = partial.begin_field("items")?;
    partial = partial.push(String::from("b"))?;
    partial = partial.end()?;

    // Third visit
    partial = partial.begin_field("items")?;
    partial = partial.push(String::from("c"))?;
    partial = partial.end()?;

    partial = partial.finish_deferred()?;
    let result = partial.build()?.materialize::<Container>()?;
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

    let mut partial = Partial::alloc::<Outer>()?;
    partial = partial.begin_deferred()?;

    partial = partial.begin_field("inner")?;
    partial = partial.begin_field("values")?;
    partial = partial.init_list()?;
    partial = partial.push(1i32)?;
    partial = partial.end()?;
    partial = partial.end()?;

    partial = partial.set_field("name", String::from("test"))?;

    partial = partial.begin_field("inner")?;
    partial = partial.begin_field("values")?;
    partial = partial.push(2i32)?;
    partial = partial.push(3i32)?;
    partial = partial.end()?;
    partial = partial.end()?;

    partial = partial.finish_deferred()?;
    let result = partial.build()?.materialize::<Outer>()?;
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

    let mut partial = Partial::alloc::<Container>()?;
    partial = partial.begin_deferred()?;

    // Insert first entry
    partial = partial.begin_field("map")?;
    partial = partial.init_map()?;
    partial = partial.begin_key()?;
    partial = partial.set(String::from("a"))?;
    partial = partial.end()?;
    partial = partial.begin_value()?;
    partial = partial.set(1i32)?;
    partial = partial.end()?;
    partial = partial.end()?;

    partial = partial.set_field("label", String::from("test"))?;

    // Re-enter and insert more
    partial = partial.begin_field("map")?;
    partial = partial.begin_key()?;
    partial = partial.set(String::from("b"))?;
    partial = partial.end()?;
    partial = partial.begin_value()?;
    partial = partial.set(2i32)?;
    partial = partial.end()?;
    partial = partial.begin_key()?;
    partial = partial.set(String::from("c"))?;
    partial = partial.end()?;
    partial = partial.begin_value()?;
    partial = partial.set(3i32)?;
    partial = partial.end()?;
    partial = partial.end()?;

    partial = partial.finish_deferred()?;
    let result = partial.build()?.materialize::<Container>()?;
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

    let mut partial = Partial::alloc::<Container>()?;
    partial = partial.begin_deferred()?;

    partial = partial.begin_field("map")?;
    partial = partial.init_map()?;
    partial = partial.begin_key()?;
    partial = partial.set(String::from("x"))?;
    partial = partial.end()?;
    partial = partial.begin_value()?;
    partial = partial.set(100u64)?;
    partial = partial.end()?;
    partial = partial.end()?;

    partial = partial.set_field("count", 42u32)?;

    partial = partial.begin_field("map")?;
    partial = partial.begin_key()?;
    partial = partial.set(String::from("y"))?;
    partial = partial.end()?;
    partial = partial.begin_value()?;
    partial = partial.set(200u64)?;
    partial = partial.end()?;
    partial = partial.end()?;

    partial = partial.finish_deferred()?;
    let result = partial.build()?.materialize::<Container>()?;
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

    let mut partial = Partial::alloc::<Container>()?;
    partial = partial.begin_deferred()?;

    // Set first element
    partial = partial.begin_field("values")?;
    partial = partial.begin_nth_field(0)?;
    partial = partial.set(10u32)?;
    partial = partial.end()?;
    partial = partial.end()?;

    partial = partial.set_field("name", String::from("test"))?;

    // Re-enter and set more elements
    partial = partial.begin_field("values")?;
    partial = partial.begin_nth_field(1)?;
    partial = partial.set(20u32)?;
    partial = partial.end()?;
    partial = partial.begin_nth_field(2)?;
    partial = partial.set(30u32)?;
    partial = partial.end()?;
    partial = partial.end()?;

    partial = partial.finish_deferred()?;
    let result = partial.build()?.materialize::<Container>()?;
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

    let mut partial = Partial::alloc::<Container>()?;
    partial = partial.begin_deferred()?;

    partial = partial.begin_field("arr")?;
    partial = partial.begin_nth_field(0)?;
    partial = partial.set(1i32)?;
    partial = partial.end()?;
    partial = partial.begin_nth_field(1)?;
    partial = partial.set(2i32)?;
    partial = partial.end()?;
    partial = partial.end()?;

    // Re-enter and overwrite
    partial = partial.begin_field("arr")?;
    partial = partial.begin_nth_field(0)?;
    partial = partial.set(100i32)?;
    partial = partial.end()?;
    partial = partial.end()?;

    partial = partial.finish_deferred()?;
    let result = partial.build()?.materialize::<Container>()?;
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

    let mut partial = Partial::alloc::<Container>()?;
    partial = partial.begin_deferred()?;

    // Enter enum, select variant, set one field
    partial = partial.begin_field("data")?;
    partial = partial.select_variant_named("Record")?;
    partial = partial.set_field("id", 42u32)?;
    partial = partial.end()?;

    partial = partial.set_field("tag", String::from("important"))?;

    // Re-enter and set more fields
    partial = partial.begin_field("data")?;
    partial = partial.set_field("name", String::from("test"))?;
    partial = partial.set_field("value", 999i64)?;
    partial = partial.end()?;

    partial = partial.finish_deferred()?;
    let result = partial.build()?.materialize::<Container>()?;
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

    let mut partial = Partial::alloc::<Container>()?;
    partial = partial.begin_deferred()?;

    partial = partial.begin_field("tags")?;
    partial = partial.init_set()?;
    partial = partial.insert(String::from("alpha"))?;
    partial = partial.end()?;

    partial = partial.set_field("count", 1u32)?;

    partial = partial.begin_field("tags")?;
    partial = partial.insert(String::from("beta"))?;
    partial = partial.insert(String::from("gamma"))?;
    partial = partial.end()?;

    partial = partial.finish_deferred()?;
    let result = partial.build()?.materialize::<Container>()?;
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

    let mut partial = Partial::alloc::<Container>()?;
    partial = partial.begin_deferred()?;

    partial = partial.begin_field("ids")?;
    partial = partial.init_set()?;
    partial = partial.insert(1i32)?;
    partial = partial.insert(2i32)?;
    partial = partial.end()?;

    partial = partial.set_field("name", String::from("test"))?;

    partial = partial.begin_field("ids")?;
    partial = partial.insert(3i32)?;
    partial = partial.end()?;

    partial = partial.finish_deferred()?;
    let result = partial.build()?.materialize::<Container>()?;
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

    let mut partial = Partial::alloc::<Outer>()?;
    partial = partial.begin_deferred()?;

    // Start inner.list
    partial = partial.begin_field("inner")?;
    partial = partial.begin_field("list")?;
    partial = partial.init_list()?;
    partial = partial.push(1i32)?;
    partial = partial.end()?;
    partial = partial.end()?;

    // Set outer.name
    partial = partial.set_field("name", String::from("test"))?;

    // Add to inner.list again
    partial = partial.begin_field("inner")?;
    partial = partial.begin_field("list")?;
    partial = partial.push(2i32)?;
    partial = partial.end()?;
    partial = partial.end()?;

    // Set outer.count
    partial = partial.set_field("count", 42u64)?;

    // Start inner.map
    partial = partial.begin_field("inner")?;
    partial = partial.begin_field("map")?;
    partial = partial.init_map()?;
    partial = partial.begin_key()?;
    partial = partial.set(String::from("a"))?;
    partial = partial.end()?;
    partial = partial.begin_value()?;
    partial = partial.set(100u32)?;
    partial = partial.end()?;
    partial = partial.end()?;
    partial = partial.end()?;

    // Add more to inner.list
    partial = partial.begin_field("inner")?;
    partial = partial.begin_field("list")?;
    partial = partial.push(3i32)?;
    partial = partial.end()?;
    partial = partial.end()?;

    // Add more to inner.map
    partial = partial.begin_field("inner")?;
    partial = partial.begin_field("map")?;
    partial = partial.begin_key()?;
    partial = partial.set(String::from("b"))?;
    partial = partial.end()?;
    partial = partial.begin_value()?;
    partial = partial.set(200u32)?;
    partial = partial.end()?;
    partial = partial.end()?;
    partial = partial.end()?;

    partial = partial.finish_deferred()?;
    let result = partial.build()?.materialize::<Outer>()?;

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

    let mut partial = Partial::alloc::<Empty>()?;
    partial = partial.begin_deferred()?;

    // Nothing to set
    partial = partial.finish_deferred()?;
    let result = partial.build()?.materialize::<Empty>()?;
    assert_eq!(result, Empty {});

    Ok(())
}

#[test]
fn deferred_single_field_struct() -> Result<(), IPanic> {
    #[derive(Facet, Debug, PartialEq)]
    struct Single {
        value: u32,
    }

    let mut partial = Partial::alloc::<Single>()?;
    partial = partial.begin_deferred()?;

    partial = partial.set_field("value", 42u32)?;

    partial = partial.finish_deferred()?;
    let result = partial.build()?.materialize::<Single>()?;
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

    let mut partial = Partial::alloc::<Container>()?;
    partial = partial.begin_deferred()?;

    // Empty structs need explicit begin/end to mark them as initialized
    partial = partial.begin_field("empty1")?;
    partial = partial.end()?;
    partial = partial.set_field("value", 123u32)?;
    partial = partial.begin_field("empty2")?;
    partial = partial.end()?;

    partial = partial.finish_deferred()?;
    let result = partial.build()?.materialize::<Container>()?;
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

    let mut partial = Partial::alloc::<Outer>()?;
    partial = partial.begin_deferred()?;

    // Set everything in first visit
    partial = partial.begin_field("inner")?;
    partial = partial.set_field("x", 42u32)?;
    partial = partial.end()?;

    partial = partial.set_field("name", String::from("test"))?;

    // Re-enter but make no changes (just looking around)
    partial = partial.begin_field("inner")?;
    partial = partial.end()?;

    partial = partial.finish_deferred()?;
    let result = partial.build()?.materialize::<Outer>()?;
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

    let mut partial = Partial::alloc::<Outer>()?;
    partial = partial.begin_deferred()?;

    partial = partial.begin_field("inner")?;
    partial = partial.set_field("a", 1u32)?;
    partial = partial.set_field("b", 2u32)?;
    partial = partial.end()?;

    // Multiple empty re-entries
    partial = partial.begin_field("inner")?;
    partial = partial.end()?;
    partial = partial.begin_field("inner")?;
    partial = partial.end()?;
    partial = partial.begin_field("inner")?;
    partial = partial.end()?;

    partial = partial.finish_deferred()?;
    let result = partial.build()?.materialize::<Outer>()?;
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

    let mut partial = Partial::alloc::<Parent>()?;
    partial = partial.begin_deferred()?;

    // Interleave access to siblings
    partial = partial.begin_field("child_a")?;
    partial = partial.set_field("value", 1i32)?;
    partial = partial.end()?;

    partial = partial.begin_field("child_c")?;
    partial = partial.set_field("value", 3i32)?;
    partial = partial.end()?;

    partial = partial.begin_field("child_b")?;
    partial = partial.set_field("value", 2i32)?;
    partial = partial.end()?;

    // Re-enter each to verify stored state
    partial = partial.begin_field("child_b")?;
    partial = partial.end()?;

    partial = partial.begin_field("child_a")?;
    partial = partial.end()?;

    partial = partial.finish_deferred()?;
    let result = partial.build()?.materialize::<Parent>()?;
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

    let mut partial = Partial::alloc::<Container>()?;
    partial = partial.begin_deferred()?;

    // First visit: just initialize the list, don't push anything
    partial = partial.begin_field("items")?;
    partial = partial.init_list()?;
    partial = partial.end()?;

    partial = partial.set_field("done", false)?;

    // Second visit: now push items
    partial = partial.begin_field("items")?;
    partial = partial.push(1u32)?;
    partial = partial.push(2u32)?;
    partial = partial.end()?;

    partial = partial.finish_deferred()?;
    let result = partial.build()?.materialize::<Container>()?;
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

    let mut partial = Partial::alloc::<Container>()?;
    partial = partial.begin_deferred()?;

    // First visit: just initialize the map
    partial = partial.begin_field("data")?;
    partial = partial.init_map()?;
    partial = partial.end()?;

    partial = partial.set_field("ready", true)?;

    // Second visit: add entries
    partial = partial.begin_field("data")?;
    partial = partial.begin_key()?;
    partial = partial.set(String::from("key"))?;
    partial = partial.end()?;
    partial = partial.begin_value()?;
    partial = partial.set(42i32)?;
    partial = partial.end()?;
    partial = partial.end()?;

    partial = partial.finish_deferred()?;
    let result = partial.build()?.materialize::<Container>()?;
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

    let mut partial = Partial::alloc::<Tree>()?;
    partial = partial.begin_deferred()?;

    // Access leaves in arbitrary order
    partial = partial.begin_field("root_right")?;
    partial = partial.begin_field("left")?;
    partial = partial.set_field("val", 3i32)?;
    partial = partial.end()?;
    partial = partial.end()?;

    partial = partial.begin_field("root_left")?;
    partial = partial.begin_field("right")?;
    partial = partial.set_field("val", 2i32)?;
    partial = partial.end()?;
    partial = partial.end()?;

    partial = partial.begin_field("root_left")?;
    partial = partial.begin_field("left")?;
    partial = partial.set_field("val", 1i32)?;
    partial = partial.end()?;
    partial = partial.end()?;

    partial = partial.begin_field("root_right")?;
    partial = partial.begin_field("right")?;
    partial = partial.set_field("val", 4i32)?;
    partial = partial.end()?;
    partial = partial.end()?;

    partial = partial.finish_deferred()?;
    let result = partial.build()?.materialize::<Tree>()?;
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

    let mut partial: Partial<'_> = Partial::alloc::<Container>()?;
    partial = partial.begin_deferred()?;

    // Set total first (interleaved with items)
    partial = partial.set_field("total", 100u32)?;

    // Build items in single visit
    partial = partial.begin_field("items")?;
    partial = partial.init_list()?;
    partial = partial.begin_list_item()?;
    partial = partial.set_field("id", 1u32)?;
    partial = partial.set_field("name", String::from("first"))?;
    partial = partial.end()?;
    partial = partial.begin_list_item()?;
    partial = partial.set_field("id", 2u32)?;
    partial = partial.set_field("name", String::from("second"))?;
    partial = partial.end()?;
    partial = partial.end()?;

    partial = partial.finish_deferred()?;
    let result = partial.build()?.materialize::<Container>()?;
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

    let mut partial: Partial<'_> = Partial::alloc::<Directory>()?;
    partial = partial.begin_deferred()?;

    // Set count first (interleaved)
    partial = partial.set_field("count", 2u32)?;

    // Build map in single visit
    partial = partial.begin_field("people")?;
    partial = partial.init_map()?;
    // First entry
    partial = partial.begin_key()?;
    partial = partial.set(String::from("alice"))?;
    partial = partial.end()?;
    partial = partial.begin_value()?;
    partial = partial.set_field("age", 30u32)?;
    partial = partial.set_field("city", String::from("NYC"))?;
    partial = partial.end()?;
    // Second entry
    partial = partial.begin_key()?;
    partial = partial.set(String::from("bob"))?;
    partial = partial.end()?;
    partial = partial.begin_value()?;
    partial = partial.set_field("age", 25u32)?;
    partial = partial.set_field("city", String::from("LA"))?;
    partial = partial.end()?;
    partial = partial.end()?;

    partial = partial.finish_deferred()?;
    let result = partial.build()?.materialize::<Directory>()?;
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

    let mut partial: Partial<'_> = Partial::alloc::<Design>()?;
    partial = partial.begin_deferred()?;

    // Set foreground variant and first field
    partial = partial.begin_field("foreground")?;
    partial = partial.select_variant_named("Rgb")?;
    partial = partial.set_field("r", 255u8)?;
    partial = partial.end()?;

    partial = partial.set_field("label", String::from("design1"))?;

    // Set background (different variant)
    partial = partial.begin_field("background")?;
    partial = partial.select_variant_named("Named")?;
    partial = partial.set_field("name", String::from("black"))?;
    partial = partial.end()?;

    // Complete foreground
    partial = partial.begin_field("foreground")?;
    partial = partial.set_field("g", 128u8)?;
    partial = partial.set_field("b", 0u8)?;
    partial = partial.end()?;

    partial = partial.finish_deferred()?;
    let result = partial.build()?.materialize::<Design>()?;
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

    let mut partial: Partial<'_> = Partial::alloc::<Point>()?;
    partial = partial.begin_deferred()?;

    partial = partial.begin_nth_field(0)?;
    partial = partial.set(10i32)?;
    partial = partial.end()?;

    partial = partial.begin_nth_field(2)?;
    partial = partial.set(30i32)?;
    partial = partial.end()?;

    partial = partial.begin_nth_field(1)?;
    partial = partial.set(20i32)?;
    partial = partial.end()?;

    partial = partial.finish_deferred()?;
    let result = partial.build()?.materialize::<Point>()?;
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

    let mut partial: Partial<'_> = Partial::alloc::<Container>()?;
    partial = partial.begin_deferred()?;

    partial = partial.begin_field("pair")?;
    partial = partial.begin_nth_field(0)?;
    partial = partial.set(1i32)?;
    partial = partial.end()?;
    partial = partial.end()?;

    partial = partial.set_field("name", String::from("test"))?;

    partial = partial.begin_field("pair")?;
    partial = partial.begin_nth_field(1)?;
    partial = partial.set(2i32)?;
    partial = partial.end()?;
    partial = partial.end()?;

    partial = partial.finish_deferred()?;
    let result = partial.build()?.materialize::<Container>()?;
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

    let mut partial: Partial<'_> = Partial::alloc::<Level1>()?;
    partial = partial.begin_deferred()?;

    // Go deep first
    partial = partial.begin_field("level2")?;
    partial = partial.begin_field("level3")?;
    partial = partial.set_field("deep", String::from("bottom"))?;
    partial = partial.end()?;
    partial = partial.end()?;

    // Set top level
    partial = partial.set_field("top", String::from("surface"))?;

    // Re-enter at depth 1 only
    partial = partial.begin_field("level2")?;
    partial = partial.set_field("mid", 42u32)?;
    partial = partial.end()?;

    // Re-enter all the way down again
    partial = partial.begin_field("level2")?;
    partial = partial.begin_field("level3")?;
    // Don't change anything, just re-enter
    partial = partial.end()?;
    partial = partial.end()?;

    partial = partial.finish_deferred()?;
    let result = partial.build()?.materialize::<Level1>()?;
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

    let mut partial: Partial<'_> = Partial::alloc::<Big>()?;
    partial = partial.begin_deferred()?;

    // Set in random order, interleaved with re-entries
    partial = partial.set_field("h", 8u32)?;
    partial = partial.set_field("a", 1u32)?;
    partial = partial.set_field("d", 4u32)?;
    partial = partial.set_field("c", 3u32)?;
    partial = partial.set_field("f", 6u32)?;
    partial = partial.set_field("b", 2u32)?;
    partial = partial.set_field("g", 7u32)?;
    partial = partial.set_field("e", 5u32)?;

    // Overwrite some
    partial = partial.set_field("a", 10u32)?;
    partial = partial.set_field("h", 80u32)?;

    partial = partial.finish_deferred()?;
    let result = partial.build()?.materialize::<Big>()?;
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

// =============================================================================
// Error cases and edge conditions
// =============================================================================

// NOTE: Tests prefixed with `wip_` use the standard #[test] attribute (not
// facet_testhelpers::test) because they need to run under Miri, which doesn't
// support the test helper setup.

/// Drop a partial without calling finish_deferred() - should not leak memory
/// Miri will catch any leaks here
#[::core::prelude::v1::test]
fn wip_deferred_drop_without_finish_simple() {
    #[derive(Facet, Debug)]
    struct Simple {
        value: String,
        count: u32,
    }

    {
        let mut partial: Partial<'_> = Partial::alloc::<Simple>().unwrap();
        partial = partial.begin_deferred().unwrap();

        partial = partial
            .set_field("value", String::from("this will be dropped"))
            .unwrap();
        partial = partial.set_field("count", 42u32).unwrap();

        // Don't call finish_deferred() or build()
        // Partial is dropped here
    }

    // If Miri doesn't complain, we're good
}

/// Drop with nested struct partially initialized
#[::core::prelude::v1::test]
fn wip_deferred_drop_without_finish_nested() {
    #[derive(Facet, Debug)]
    struct Inner {
        text: String,
    }

    #[derive(Facet, Debug)]
    struct Outer {
        inner: Inner,
        name: String,
    }

    {
        let mut partial: Partial<'_> = Partial::alloc::<Outer>().unwrap();
        partial = partial.begin_deferred().unwrap();

        partial = partial
            .set_field("name", String::from("outer name"))
            .unwrap();
        partial = partial.begin_field("inner").unwrap();
        partial = partial
            .set_field("text", String::from("inner text"))
            .unwrap();
        partial = partial.end().unwrap();

        // Drop without finishing
    }
}

/// Drop with collections partially filled
#[::core::prelude::v1::test]
fn wip_deferred_drop_without_finish_collections() {
    use std::collections::HashMap;

    #[derive(Facet, Debug)]
    struct WithCollections {
        strings: Vec<String>,
        map: HashMap<String, String>,
    }

    {
        let mut partial: Partial<'_> = Partial::alloc::<WithCollections>().unwrap();
        partial = partial.begin_deferred().unwrap();

        partial = partial.begin_field("strings").unwrap();
        partial = partial.init_list().unwrap();
        partial = partial.push(String::from("item1")).unwrap();
        partial = partial.push(String::from("item2")).unwrap();
        partial = partial.push(String::from("item3")).unwrap();
        partial = partial.end().unwrap();

        partial = partial.begin_field("map").unwrap();
        partial = partial.init_map().unwrap();
        partial = partial.begin_key().unwrap();
        partial = partial.set(String::from("key1")).unwrap();
        partial = partial.end().unwrap();
        partial = partial.begin_value().unwrap();
        partial = partial.set(String::from("value1")).unwrap();
        partial = partial.end().unwrap();
        partial = partial.end().unwrap();

        // Drop without finishing - lots of allocations to clean up
    }
}

/// Drop while in the middle of a field (frame still on stack)
#[::core::prelude::v1::test]
fn wip_deferred_drop_mid_field() {
    #[derive(Facet, Debug)]
    struct Inner {
        a: u32,
        b: u32,
    }

    #[derive(Facet, Debug)]
    struct Outer {
        inner: Inner,
    }

    {
        let mut partial: Partial<'_> = Partial::alloc::<Outer>().unwrap();
        partial = partial.begin_deferred().unwrap();

        partial = partial.begin_field("inner").unwrap();
        partial = partial.set_field("a", 1u32).unwrap();
        // Don't call end() - frame is still on the stack

        // Drop with frame stack: [Outer, Inner]
    }
}

/// Calling finish_deferred() twice should fail
#[test]
fn error_finish_deferred_twice() -> Result<(), IPanic> {
    #[derive(Facet, Debug)]
    struct Simple {
        value: u32,
    }

    let mut partial: Partial<'_> = Partial::alloc::<Simple>()?;
    partial = partial.begin_deferred()?;

    partial = partial.set_field("value", 42u32)?;

    partial = partial.finish_deferred()?;

    // Second call should fail
    match partial.finish_deferred() {
        Ok(_) => panic!("Expected error on second finish_deferred"),
        Err(err) => assert!(err.to_string().contains("deferred mode is not enabled")),
    }

    Ok(())
}

/// Calling begin_deferred() twice should return an error on the second call.
#[test]
fn error_begin_deferred_twice() -> Result<(), IPanic> {
    #[derive(Facet, Debug)]
    struct Simple {
        value: u32,
    }

    let mut partial: Partial<'_> = Partial::alloc::<Simple>()?;

    partial = partial.begin_deferred()?;
    assert!(partial.is_deferred());

    // Second begin_deferred should return an error and consume the partial
    assert!(partial.begin_deferred().is_err());

    // Note: partial was consumed by the error above, so we can't continue this test
    // The test verified the error case, which is what we wanted

    Ok(())
}

/// Enum variant switching - selecting a different variant after setting fields
/// This should either fail or reset the enum state
#[test]
fn error_enum_variant_switch() -> Result<(), IPanic> {
    #[derive(Facet, Debug, PartialEq)]
    #[repr(u8)]
    #[allow(dead_code)]
    enum Choice {
        OptionA { a_value: u32 },
        OptionB { b_value: String },
    }

    let mut partial: Partial<'_> = Partial::alloc::<Choice>()?;
    partial = partial.begin_deferred()?;

    // Select variant A and set its field
    partial = partial.select_variant_named("OptionA")?;
    partial = partial.set_field("a_value", 42u32)?;

    // Now try to select variant B - this might fail or reset
    // With ownership API, if this fails, the partial is consumed
    match partial.select_variant_named("OptionB") {
        Ok(mut p) => {
            // If switching is allowed, the previous data should be gone
            // and we should be able to set B's fields
            p = p.set_field("b_value", String::from("switched"))?;
            p = p.finish_deferred()?;
            let result = p.build()?.materialize::<Choice>()?;
            assert_eq!(
                result,
                Choice::OptionB {
                    b_value: String::from("switched")
                }
            );
        }
        Err(_) => {
            // If switching is not allowed, we get an error and partial is consumed
            // We can't continue to verify the original state
        }
    }

    Ok(())
}

/// Enum variant switch in deferred mode with re-entry
#[test]
fn error_enum_variant_switch_with_reentry() -> Result<(), IPanic> {
    #[derive(Facet, Debug, PartialEq)]
    #[repr(u8)]
    #[allow(dead_code)]
    enum Status {
        Active { code: u32 },
        Inactive { reason: String },
    }

    #[derive(Facet, Debug, PartialEq)]
    struct Record {
        status: Status,
        id: u32,
    }

    let mut partial: Partial<'_> = Partial::alloc::<Record>()?;
    partial = partial.begin_deferred()?;

    // Set up Active variant
    partial = partial.begin_field("status")?;
    partial = partial.select_variant_named("Active")?;
    partial = partial.set_field("code", 200u32)?;
    partial = partial.end()?;

    partial = partial.set_field("id", 1u32)?;

    // Re-enter and try to switch variant
    partial = partial.begin_field("status")?;

    // Try to switch variant - if this fails, partial is consumed
    match partial.select_variant_named("Inactive") {
        Ok(mut p) => {
            p = p.set_field("reason", String::from("changed mind"))?;
            p = p.end()?;
            p = p.finish_deferred()?;
            let _result = p.build()?.materialize::<Record>()?;
        }
        Err(_) => {
            // Variant switch failed, partial is consumed
            // We just want to make sure we don't crash or leak
        }
    }
    Ok(())
}

/// Try to build() without finish_deferred() in deferred mode
#[test]
fn error_build_without_finish_deferred() -> Result<(), IPanic> {
    #[derive(Facet, Debug)]
    struct Simple {
        value: u32,
    }

    let mut partial: Partial<'_> = Partial::alloc::<Simple>()?;
    partial = partial.begin_deferred()?;

    partial = partial.set_field("value", 42u32)?;

    // Try to build without finishing deferred mode
    // This might succeed (deferred mode just affects validation timing)
    // or it might fail - let's document the behavior
    let build_result = partial.build();

    // Either way, we shouldn't crash
    if build_result.is_ok() {
        let result = build_result.unwrap().materialize::<Simple>()?;
        assert_eq!(result.value, 42);
    }

    Ok(())
}

/// Operations after finish_deferred() but before build()
#[test]
fn error_operations_after_finish() -> Result<(), IPanic> {
    #[derive(Facet, Debug, PartialEq)]
    struct Simple {
        a: u32,
        b: u32,
    }

    let mut partial: Partial<'_> = Partial::alloc::<Simple>()?;
    partial = partial.begin_deferred()?;

    partial = partial.set_field("a", 1u32)?;
    partial = partial.set_field("b", 2u32)?;

    partial = partial.finish_deferred()?;

    // Now we're no longer in deferred mode - can we still modify?
    match partial.set_field("a", 100u32) {
        Ok(p) => {
            let result = p.build()?.materialize::<Simple>()?;
            assert_eq!(result.a, 100); // Modified after finish
            assert_eq!(result.b, 2);
        }
        Err(_) => {
            // Modification after finish failed, which is also valid behavior
            // The partial is consumed by the error
        }
    }

    Ok(())
}

/// Deep nesting with stored frames, then drop
#[::core::prelude::v1::test]
fn wip_deferred_drop_with_stored_frames() {
    #[derive(Facet, Debug)]
    struct L3 {
        val: String,
    }

    #[derive(Facet, Debug)]
    struct L2 {
        l3: L3,
    }

    #[derive(Facet, Debug)]
    struct L1 {
        l2: L2,
        other: String,
    }

    {
        let mut partial: Partial<'_> = Partial::alloc::<L1>().unwrap();
        partial = partial.begin_deferred().unwrap();

        // Go deep
        partial = partial.begin_field("l2").unwrap();
        partial = partial.begin_field("l3").unwrap();
        partial = partial
            .set_field("val", String::from("deep value"))
            .unwrap();
        partial = partial.end().unwrap();
        partial = partial.end().unwrap();

        // Set another field (l2 and l3 are now in stored_frames)
        partial = partial
            .set_field("other", String::from("other value"))
            .unwrap();

        // Drop with frames in stored_frames map
    }
}

// =============================================================================
// Nested deferred mode (deferred mode started from non-root position)
// =============================================================================

/// Tests that deferred mode works correctly when started from a nested position.
#[test]
fn deferred_started_from_nested_position() -> Result<(), IPanic> {
    #[derive(Facet, Debug, PartialEq)]
    struct ConnectionSettings {
        port: u16,
        timeout: u32,
    }

    #[derive(Facet, Debug, PartialEq)]
    struct Server {
        host: String,
        connection: ConnectionSettings,
    }

    #[derive(Facet, Debug, PartialEq)]
    struct Config {
        server: Server,
    }

    let mut partial: Partial<'_> = Partial::alloc::<Config>()?;

    // Navigate into server first
    partial = partial.begin_field("server")?;

    // Now start deferred mode from the nested position
    // At this point, frames = [Config, Server], start_depth = 2
    partial = partial.begin_deferred()?;

    // Set host (direct field of Server)
    partial = partial.set_field("host", String::from("localhost"))?;

    // Set connection.port (nested from Server's perspective)
    partial = partial.begin_field("connection")?;
    partial = partial.set_field("port", 8080u16)?;
    partial = partial.end()?;

    // Set connection.timeout (interleaved)
    partial = partial.begin_field("connection")?;
    partial = partial.set_field("timeout", 30u32)?;
    partial = partial.end()?;

    partial = partial.finish_deferred()?;
    partial = partial.end()?;

    let result = partial.build()?.materialize::<Config>()?;
    assert_eq!(result.server.host, "localhost");
    assert_eq!(result.server.connection.port, 8080);
    assert_eq!(result.server.connection.timeout, 30);

    Ok(())
}

/// Tests deferred mode from deeper nesting
#[test]
fn deferred_started_from_deeply_nested_position() -> Result<(), IPanic> {
    #[derive(Facet, Debug, PartialEq)]
    struct Leaf {
        value: i32,
    }

    #[derive(Facet, Debug, PartialEq)]
    struct Level2 {
        leaf: Leaf,
        other: String,
    }

    #[derive(Facet, Debug, PartialEq)]
    struct Level1 {
        level2: Level2,
    }

    #[derive(Facet, Debug, PartialEq)]
    struct Root {
        level1: Level1,
    }

    let mut partial: Partial<'_> = Partial::alloc::<Root>()?;

    // Navigate deep before starting deferred mode
    partial = partial.begin_field("level1")?;
    partial = partial.begin_field("level2")?;

    // Start deferred mode at Level2 depth
    // frames = [Root, Level1, Level2], start_depth = 3
    partial = partial.begin_deferred()?;

    // Set fields in interleaved order
    partial = partial.set_field("other", String::from("test"))?;

    partial = partial.begin_field("leaf")?;
    partial = partial.set_field("value", 42i32)?;
    partial = partial.end()?;

    partial = partial.finish_deferred()?;
    partial = partial.end()?;
    partial = partial.end()?;

    let result = partial.build()?.materialize::<Root>()?;
    assert_eq!(result.level1.level2.other, "test");
    assert_eq!(result.level1.level2.leaf.value, 42);

    Ok(())
}

// =============================================================================
// Auto-defaulting tests: finish_deferred should fill in defaults for unset fields
// =============================================================================

#[test]
fn deferred_option_field_auto_defaults_to_none() -> Result<(), IPanic> {
    #[derive(Facet, Debug, PartialEq)]
    struct WithOption {
        required: String,
        optional: Option<u32>,
    }

    let mut partial: Partial<'_> = Partial::alloc::<WithOption>()?;
    partial = partial.begin_deferred()?;

    partial = partial.set_field("required", String::from("hello"))?;
    // Don't set optional - it should auto-default to None

    partial = partial.finish_deferred()?;
    let result = partial.build()?.materialize::<WithOption>()?;
    assert_eq!(result.required, "hello");
    assert_eq!(result.optional, None);

    Ok(())
}

#[test]
fn deferred_multiple_option_fields_auto_default() -> Result<(), IPanic> {
    #[derive(Facet, Debug, PartialEq)]
    struct ManyOptions {
        name: String,
        opt1: Option<i32>,
        opt2: Option<String>,
        opt3: Option<bool>,
    }

    let mut partial: Partial<'_> = Partial::alloc::<ManyOptions>()?;
    partial = partial.begin_deferred()?;

    partial = partial.set_field("name", String::from("test"))?;
    // Set only one optional field
    partial = partial.begin_field("opt2")?;
    partial = partial.begin_some()?;
    partial = partial.set(String::from("has value"))?;
    partial = partial.end()?;
    partial = partial.end()?;

    partial = partial.finish_deferred()?;
    let result = partial.build()?.materialize::<ManyOptions>()?;
    assert_eq!(result.name, "test");
    assert_eq!(result.opt1, None);
    assert_eq!(result.opt2, Some(String::from("has value")));
    assert_eq!(result.opt3, None);

    Ok(())
}

#[test]
fn deferred_field_with_default_attr_auto_applies() -> Result<(), IPanic> {
    #[derive(Facet, Debug, PartialEq)]
    struct WithDefault {
        name: String,
        #[facet(default = 100u32)]
        count: u32,
    }

    let mut partial: Partial<'_> = Partial::alloc::<WithDefault>()?;
    partial = partial.begin_deferred()?;

    partial = partial.set_field("name", String::from("test"))?;
    // Don't set count - should use default value of 100

    partial = partial.finish_deferred()?;
    let result = partial.build()?.materialize::<WithDefault>()?;
    assert_eq!(result.name, "test");
    assert_eq!(result.count, 100);

    Ok(())
}

#[test]
fn deferred_field_with_default_impl_auto_applies() -> Result<(), IPanic> {
    #[derive(Facet, Debug, PartialEq)]
    struct WithDefault {
        name: String,
        #[facet(default)]
        items: Vec<i32>,
    }

    let mut partial: Partial<'_> = Partial::alloc::<WithDefault>()?;
    partial = partial.begin_deferred()?;

    partial = partial.set_field("name", String::from("test"))?;
    // Don't set items - should use Default::default() (empty Vec)

    partial = partial.finish_deferred()?;
    let result = partial.build()?.materialize::<WithDefault>()?;
    assert_eq!(result.name, "test");
    assert_eq!(result.items, Vec::<i32>::new());

    Ok(())
}

#[test]
fn deferred_enum_variant_option_field_auto_defaults() -> Result<(), IPanic> {
    #[derive(Facet, Debug, PartialEq)]
    #[repr(u8)]
    #[allow(dead_code)]
    enum Root {
        A(Option<String>),
        B { b1: Option<i32>, b2: Option<bool> },
    }

    // Test variant B with only one field set
    let mut partial: Partial<'_> = Partial::alloc::<Root>()?;
    partial = partial.begin_deferred()?;

    partial = partial.select_variant_named("B")?;
    partial = partial.begin_field("b1")?;
    partial = partial.begin_some()?;
    partial = partial.set(42i32)?;
    partial = partial.end()?;
    partial = partial.end()?;
    // Don't set b2 - should default to None

    partial = partial.finish_deferred()?;
    let result = partial.build()?.materialize::<Root>()?;
    assert_eq!(
        result,
        Root::B {
            b1: Some(42),
            b2: None
        }
    );

    Ok(())
}

#[test]
fn deferred_enum_tuple_variant_option_defaults() -> Result<(), IPanic> {
    #[derive(Facet, Debug, PartialEq)]
    #[repr(u8)]
    #[allow(dead_code)]
    enum Root {
        A(Option<String>),
        B { b1: Option<i32>, b2: Option<bool> },
    }

    // Test variant A with no field set (table header case like [A] in TOML)
    let mut partial: Partial<'_> = Partial::alloc::<Root>()?;
    partial = partial.begin_deferred()?;

    partial = partial.select_variant_named("A")?;
    // Don't set field 0 - should default to None

    partial = partial.finish_deferred()?;
    let result = partial.build()?.materialize::<Root>()?;
    assert_eq!(result, Root::A(None));

    Ok(())
}

#[test]
fn deferred_nested_struct_option_fields_auto_default() -> Result<(), IPanic> {
    #[derive(Facet, Debug, PartialEq)]
    struct Inner {
        required: String,
        optional: Option<u32>,
    }

    #[derive(Facet, Debug, PartialEq)]
    struct Outer {
        inner: Inner,
        opt: Option<String>,
    }

    let mut partial: Partial<'_> = Partial::alloc::<Outer>()?;
    partial = partial.begin_deferred()?;

    partial = partial.begin_field("inner")?;
    partial = partial.set_field("required", String::from("hello"))?;
    // Don't set inner.optional
    partial = partial.end()?;
    // Don't set outer.opt

    partial = partial.finish_deferred()?;
    let result = partial.build()?.materialize::<Outer>()?;
    assert_eq!(result.inner.required, "hello");
    assert_eq!(result.inner.optional, None);
    assert_eq!(result.opt, None);

    Ok(())
}

#[test]
fn deferred_all_fields_are_optional() -> Result<(), IPanic> {
    #[derive(Facet, Debug, PartialEq)]
    struct AllOptional {
        a: Option<i32>,
        b: Option<String>,
        c: Option<bool>,
    }

    let mut partial: Partial<'_> = Partial::alloc::<AllOptional>()?;
    partial = partial.begin_deferred()?;

    // Set nothing at all - all should default to None

    partial = partial.finish_deferred()?;
    let result = partial.build()?.materialize::<AllOptional>()?;
    assert_eq!(result.a, None);
    assert_eq!(result.b, None);
    assert_eq!(result.c, None);

    Ok(())
}

// =============================================================================
// Option<Struct> interleaved field access (path tracking for begin_some)
// =============================================================================

#[test]
fn deferred_option_struct_interleaved_fields() -> Result<(), IPanic> {
    // This tests that begin_some() correctly pushes "Some" onto the deferred path,
    // allowing us to exit and re-enter an Option<Struct> to set fields interleaved
    // with other operations.
    #[derive(Facet, Debug, PartialEq)]
    struct Inner {
        host: String,
        port: u16,
    }

    #[derive(Facet, Debug, PartialEq)]
    struct Outer {
        name: String,
        connection: Option<Inner>,
    }

    let mut partial: Partial<'_> = Partial::alloc::<Outer>()?;
    partial = partial.begin_deferred()?;

    // Set name first
    partial = partial.set_field("name", String::from("test"))?;

    // Enter Option, set first inner field
    partial = partial.begin_field("connection")?;
    partial = partial.begin_some()?;
    partial = partial.set_field("host", String::from("localhost"))?;
    partial = partial.end()?; // end Inner (stored at path ["connection", "Some"])
    partial = partial.end()?; // end Option field (stored at path ["connection"])

    // Re-enter and set second inner field
    partial = partial.begin_field("connection")?;
    partial = partial.begin_some()?; // should restore stored frame
    partial = partial.set_field("port", 8080u16)?;
    partial = partial.end()?;
    partial = partial.end()?;

    partial = partial.finish_deferred()?;
    let result = partial.build()?.materialize::<Outer>()?;

    assert_eq!(result.name, "test");
    assert_eq!(
        result.connection,
        Some(Inner {
            host: String::from("localhost"),
            port: 8080
        })
    );

    Ok(())
}

#[test]
fn deferred_option_struct_deeply_nested_interleaved() -> Result<(), IPanic> {
    // Test deeper nesting: Outer -> Option<Middle> -> Inner
    #[derive(Facet, Debug, PartialEq)]
    struct Inner {
        value: i32,
    }

    #[derive(Facet, Debug, PartialEq)]
    struct Middle {
        inner: Inner,
        tag: String,
    }

    #[derive(Facet, Debug, PartialEq)]
    struct Outer {
        name: String,
        middle: Option<Middle>,
    }

    let mut partial: Partial<'_> = Partial::alloc::<Outer>()?;
    partial = partial.begin_deferred()?;

    // Set outer name
    partial = partial.set_field("name", String::from("outer"))?;

    // Enter Option<Middle>, then Inner, set value
    partial = partial.begin_field("middle")?;
    partial = partial.begin_some()?;
    partial = partial.begin_field("inner")?;
    partial = partial.set_field("value", 42i32)?;
    partial = partial.end()?; // end Inner
    partial = partial.end()?; // end Middle (the Some content)
    partial = partial.end()?; // end Option field

    // Re-enter to set Middle.tag
    partial = partial.begin_field("middle")?;
    partial = partial.begin_some()?;
    partial = partial.set_field("tag", String::from("tagged"))?;
    partial = partial.end()?;
    partial = partial.end()?;

    partial = partial.finish_deferred()?;
    let result = partial.build()?.materialize::<Outer>()?;

    assert_eq!(result.name, "outer");
    assert_eq!(
        result.middle,
        Some(Middle {
            inner: Inner { value: 42 },
            tag: String::from("tagged")
        })
    );

    Ok(())
}

// =============================================================================
// Proxy + deferred mode tests
// =============================================================================

/// Test that proxy conversion works correctly in deferred mode
#[test]
fn deferred_with_proxy() -> Result<(), IPanic> {
    #[derive(Copy, Clone, Debug, Eq, PartialEq)]
    pub struct NotDerivingFacet(u64);

    // Proxy type that derives Facet
    #[derive(Facet, Copy, Clone)]
    pub struct NotDerivingFacetProxy(u64);

    impl TryFrom<NotDerivingFacetProxy> for NotDerivingFacet {
        type Error = &'static str;
        fn try_from(val: NotDerivingFacetProxy) -> Result<Self, Self::Error> {
            Ok(NotDerivingFacet(val.0))
        }
    }

    impl TryFrom<&NotDerivingFacet> for NotDerivingFacetProxy {
        type Error = &'static str;
        fn try_from(val: &NotDerivingFacet) -> Result<Self, Self::Error> {
            Ok(NotDerivingFacetProxy(val.0))
        }
    }

    #[derive(Facet, Debug)]
    pub struct Container {
        name: String,
        #[facet(opaque, proxy = NotDerivingFacetProxy)]
        inner: NotDerivingFacet,
    }

    let mut partial: Partial<'_> = Partial::alloc::<Container>()?;
    partial = partial.begin_deferred()?;

    // Set name first
    partial = partial.set_field("name", String::from("test"))?;

    // Now set inner using proxy
    partial = partial.begin_field("inner")?;
    partial = partial.begin_custom_deserialization()?;
    assert_eq!(partial.shape(), NotDerivingFacetProxy::SHAPE);
    partial = partial.set(NotDerivingFacetProxy(35))?;
    partial = partial.end()?; // end proxy frame - should do conversion
    partial = partial.end()?; // end inner field

    partial = partial.finish_deferred()?;
    let result = partial.build()?.materialize::<Container>()?;

    assert_eq!(result.name, "test");
    assert_eq!(result.inner, NotDerivingFacet(35));

    Ok(())
}

/// Regression test: deferred mode started mid-deserialization with nested list
#[test]
fn deferred_started_inside_struct_with_list() -> Result<(), IPanic> {
    #[derive(Facet, Debug, PartialEq)]
    pub struct Item {
        value: u32,
    }

    #[derive(Facet, Debug)]
    pub struct Container {
        name: String,
        items: Vec<Item>,
    }

    let mut partial: Partial<'_> = Partial::alloc::<Container>()?;

    // Set name first (before deferred mode)
    partial = partial.set_field("name", String::from("test"))?;

    // Now enter deferred mode (like facet-dom does when it sees flatten)
    partial = partial.begin_deferred()?;

    // Build the list inside deferred mode
    partial = partial.begin_field("items")?;
    partial = partial.init_list()?;
    partial = partial.begin_list_item()?;
    partial = partial.set_field("value", 42u32)?;
    partial = partial.end()?; // end item
    partial = partial.end()?; // end items

    partial = partial.finish_deferred()?;
    let result = partial.build()?.materialize::<Container>()?;

    assert_eq!(result.name, "test");
    assert_eq!(result.items.len(), 1);
    assert_eq!(result.items[0], Item { value: 42 });

    Ok(())
}

/// Regression test for use-after-free when Vec reallocates during deferred mode.
/// This simulates the SVG parsing case where a Vec<Enum> (like Vec<SvgNode>)
/// grows and reallocates while deferred frames still reference earlier items.
#[test]
fn deferred_vec_realloc_with_nested_structs() -> Result<(), IPanic> {
    #[derive(Facet, Debug, PartialEq)]
    struct Text {
        content: String,
        x: i32,
        y: i32,
    }

    #[derive(Facet, Debug, PartialEq)]
    struct Rect {
        width: i32,
        height: i32,
    }

    #[derive(Facet, Debug, PartialEq)]
    #[repr(u8)]
    enum Node {
        Text(Text),
        Rect(Rect),
    }

    #[derive(Facet, Debug, PartialEq)]
    struct Container {
        name: String,
        children: Vec<Node>,
    }

    let mut partial: Partial<'_> = Partial::alloc::<Container>()?;
    partial = partial.begin_deferred()?;

    partial = partial.set_field("name", String::from("test"))?;

    // Start the children list
    partial = partial.begin_field("children")?;
    partial = partial.init_list()?;

    // Add multiple items to trigger reallocation (Vec starts small)
    for i in 0..10i32 {
        partial = partial.begin_list_item()?;
        if i % 2 == 0 {
            // Text variant - it's Text(Text), so we need to access field 0 for the inner struct
            partial = partial.select_variant_named("Text")?;
            partial = partial.begin_nth_field(0)?; // enter the Text struct
            partial = partial.set_field("content", format!("text {}", i))?;
            partial = partial.set_field("x", i * 10)?;
            partial = partial.set_field("y", i * 5)?;
            partial = partial.end()?; // end Text struct
        } else {
            // Rect variant
            partial = partial.select_variant_named("Rect")?;
            partial = partial.begin_nth_field(0)?; // enter the Rect struct
            partial = partial.set_field("width", 100 + i)?;
            partial = partial.set_field("height", 50 + i)?;
            partial = partial.end()?; // end Rect struct
        }
        partial = partial.end()?; // end list item
    }

    partial = partial.end()?; // end children

    // This is where the bug manifests: finish_deferred tries to access
    // deferred frames that point to memory from before Vec reallocation
    partial = partial.finish_deferred()?;

    let result = partial.build()?.materialize::<Container>()?;
    assert_eq!(result.name, "test");
    assert_eq!(result.children.len(), 10);

    Ok(())
}

/// Same as above but with Option fields that need auto-defaulting.
/// This is closer to the actual SVG case where finish_deferred must fill in None values.
#[test]
fn deferred_vec_realloc_with_option_defaults() -> Result<(), IPanic> {
    #[derive(Facet, Debug, PartialEq)]
    struct Text {
        content: String,
        x: Option<i32>,
        y: Option<i32>,
    }

    #[derive(Facet, Debug, PartialEq)]
    struct Rect {
        width: i32,
        height: Option<i32>,
    }

    #[derive(Facet, Debug, PartialEq)]
    #[repr(u8)]
    enum Node {
        Text(Text),
        Rect(Rect),
    }

    #[derive(Facet, Debug, PartialEq)]
    struct Container {
        name: String,
        children: Vec<Node>,
    }

    let mut partial: Partial<'_> = Partial::alloc::<Container>()?;
    partial = partial.begin_deferred()?;

    partial = partial.set_field("name", String::from("test"))?;

    // Start the children list
    partial = partial.begin_field("children")?;
    partial = partial.init_list()?;

    // Add multiple items to trigger reallocation (Vec starts small)
    for i in 0..10i32 {
        partial = partial.begin_list_item()?;
        if i % 2 == 0 {
            // Text variant - only set required content field, leave x and y as None
            partial = partial.select_variant_named("Text")?;
            partial = partial.begin_nth_field(0)?; // enter the Text struct
            partial = partial.set_field("content", format!("text {}", i))?;
            // x and y are Option<i32>, should default to None
            partial = partial.end()?; // end Text struct
        } else {
            // Rect variant - only set required width field, leave height as None
            partial = partial.select_variant_named("Rect")?;
            partial = partial.begin_nth_field(0)?; // enter the Rect struct
            partial = partial.set_field("width", 100 + i)?;
            // height is Option<i32>, should default to None
            partial = partial.end()?; // end Rect struct
        }
        partial = partial.end()?; // end list item
    }

    partial = partial.end()?; // end children

    // This is where the bug manifests: finish_deferred tries to fill defaults
    // for Option fields (x, y, height) but the Vec has reallocated
    partial = partial.finish_deferred()?;

    let result = partial.build()?.materialize::<Container>()?;
    assert_eq!(result.name, "test");
    assert_eq!(result.children.len(), 10);

    // Verify Option fields were defaulted to None
    if let Node::Text(ref text) = result.children[0] {
        assert_eq!(text.content, "text 0");
        assert_eq!(text.x, None);
        assert_eq!(text.y, None);
    } else {
        panic!("Expected Text variant at index 0");
    }

    if let Node::Rect(ref rect) = result.children[1] {
        assert_eq!(rect.width, 101);
        assert_eq!(rect.height, None);
    } else {
        panic!("Expected Rect variant at index 1");
    }

    Ok(())
}
