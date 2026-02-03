//! Tests for deferred processing of elements inside lists.
//!
//! These tests exercise the scenario where struct elements inside a Vec need
//! deferred processing (e.g., interleaved field access). This is the core issue
//! that motivates the rope-based list storage approach.
//!
//! Rope-based storage ensures that:
//! - Elements live in stable chunks that never move
//! - Frames can safely point into these chunks for deferred processing
//! - On list finalization, elements are moved to the real Vec
//! - This enables full deferred processing inside list elements

use facet::Facet;
use facet_reflect::Partial;
use facet_testhelpers::{IPanic, test};

// =============================================================================
// Basic: Deferred mode for struct elements in Vec
// =============================================================================

#[test]
fn vec_struct_element_deferred_simple() -> Result<(), IPanic> {
    // Simple case: struct element built with deferred mode
    // Fields set in order, but using begin_field/end pattern
    #[derive(Facet, Debug, PartialEq)]
    struct Item {
        a: u32,
        b: String,
    }

    let mut partial = Partial::alloc::<Vec<Item>>()?;
    partial = partial.init_list()?;

    // Build first element
    partial = partial.begin_list_item()?;
    partial = partial.begin_deferred()?;
    partial = partial.set_field("a", 1u32)?;
    partial = partial.set_field("b", String::from("first"))?;
    partial = partial.finish_deferred()?;
    partial = partial.end()?;

    // Build second element
    partial = partial.begin_list_item()?;
    partial = partial.begin_deferred()?;
    partial = partial.set_field("a", 2u32)?;
    partial = partial.set_field("b", String::from("second"))?;
    partial = partial.finish_deferred()?;
    partial = partial.end()?;

    let result = partial.build()?.materialize::<Vec<Item>>()?;
    assert_eq!(result.len(), 2);
    assert_eq!(
        result[0],
        Item {
            a: 1,
            b: String::from("first")
        }
    );
    assert_eq!(
        result[1],
        Item {
            a: 2,
            b: String::from("second")
        }
    );

    Ok(())
}

#[test]
fn vec_struct_element_deferred_interleaved_fields() -> Result<(), IPanic> {
    // Harder case: struct fields set in interleaved order
    // This requires re-entering the struct within a single element
    #[derive(Facet, Debug, PartialEq)]
    struct Item {
        x: u32,
        y: u32,
        z: u32,
    }

    let mut partial = Partial::alloc::<Vec<Item>>()?;
    partial = partial.init_list()?;

    partial = partial.begin_list_item()?;
    partial = partial.begin_deferred()?;

    // Set fields in non-sequential order with re-entry
    partial = partial.set_field("x", 1u32)?;
    partial = partial.set_field("z", 3u32)?;
    partial = partial.set_field("y", 2u32)?;

    partial = partial.finish_deferred()?;
    partial = partial.end()?;

    let result = partial.build()?.materialize::<Vec<Item>>()?;
    assert_eq!(result.len(), 1);
    assert_eq!(result[0], Item { x: 1, y: 2, z: 3 });

    Ok(())
}

// =============================================================================
// Nested structs inside Vec elements
// =============================================================================

#[test]
fn vec_nested_struct_element_deferred() -> Result<(), IPanic> {
    #[derive(Facet, Debug, PartialEq)]
    struct Inner {
        value: i32,
    }

    #[derive(Facet, Debug, PartialEq)]
    struct Outer {
        name: String,
        inner: Inner,
    }

    let mut partial = Partial::alloc::<Vec<Outer>>()?;
    partial = partial.init_list()?;

    partial = partial.begin_list_item()?;
    partial = partial.begin_deferred()?;

    // Interleaved: set name, then inner.value, then could revisit name
    partial = partial.set_field("name", String::from("test"))?;
    partial = partial.begin_field("inner")?;
    partial = partial.set_field("value", 42i32)?;
    partial = partial.end()?;

    partial = partial.finish_deferred()?;
    partial = partial.end()?;

    let result = partial.build()?.materialize::<Vec<Outer>>()?;
    assert_eq!(result.len(), 1);
    assert_eq!(result[0].name, "test");
    assert_eq!(result[0].inner.value, 42);

    Ok(())
}

#[test]
fn vec_nested_struct_interleaved_reentry() -> Result<(), IPanic> {
    // The key failing case: re-entering nested struct after leaving it
    #[derive(Facet, Debug, PartialEq)]
    struct Inner {
        a: u32,
        b: u32,
    }

    #[derive(Facet, Debug, PartialEq)]
    struct Outer {
        inner: Inner,
        name: String,
    }

    let mut partial = Partial::alloc::<Vec<Outer>>()?;
    partial = partial.init_list()?;

    partial = partial.begin_list_item()?;
    partial = partial.begin_deferred()?;

    // Set inner.a
    partial = partial.begin_field("inner")?;
    partial = partial.set_field("a", 1u32)?;
    partial = partial.end()?;

    // Set outer name
    partial = partial.set_field("name", String::from("test"))?;

    // Re-enter inner to set b (this is the problematic case)
    partial = partial.begin_field("inner")?;
    partial = partial.set_field("b", 2u32)?;
    partial = partial.end()?;

    partial = partial.finish_deferred()?;
    partial = partial.end()?;

    let result = partial.build()?.materialize::<Vec<Outer>>()?;
    assert_eq!(result.len(), 1);
    assert_eq!(result[0].inner.a, 1);
    assert_eq!(result[0].inner.b, 2);
    assert_eq!(result[0].name, "test");

    Ok(())
}

// =============================================================================
// Enum inside Vec elements (the #2007/#2010 case)
// =============================================================================

#[test]
fn vec_enum_element_simple() -> Result<(), IPanic> {
    #[derive(Facet, Debug, PartialEq)]
    #[repr(u8)]
    enum Item {
        A { value: u32 },
        B { x: u32, y: u32 },
    }

    let mut partial = Partial::alloc::<Vec<Item>>()?;
    partial = partial.init_list()?;

    partial = partial.begin_list_item()?;
    partial = partial.begin_deferred()?;
    partial = partial.select_variant_named("A")?;
    partial = partial.set_field("value", 42u32)?;
    partial = partial.finish_deferred()?;
    partial = partial.end()?;

    partial = partial.begin_list_item()?;
    partial = partial.begin_deferred()?;
    partial = partial.select_variant_named("B")?;
    partial = partial.set_field("x", 1u32)?;
    partial = partial.set_field("y", 2u32)?;
    partial = partial.finish_deferred()?;
    partial = partial.end()?;

    let result = partial.build()?.materialize::<Vec<Item>>()?;
    assert_eq!(result.len(), 2);
    assert_eq!(result[0], Item::A { value: 42 });
    assert_eq!(result[1], Item::B { x: 1, y: 2 });

    Ok(())
}

#[test]
fn vec_enum_element_interleaved_variant_fields() -> Result<(), IPanic> {
    // The core #2007/#2010 case: enum variant fields set with interleaving
    #[derive(Facet, Debug, PartialEq)]
    #[repr(u8)]
    enum Item {
        Record { id: u32, name: String, score: i32 },
    }

    #[derive(Facet, Debug, PartialEq)]
    struct Wrapper {
        item: Item,
        tag: String,
    }

    let mut partial = Partial::alloc::<Vec<Wrapper>>()?;
    partial = partial.init_list()?;

    partial = partial.begin_list_item()?;
    partial = partial.begin_deferred()?;

    // Select variant and set first field
    partial = partial.begin_field("item")?;
    partial = partial.select_variant_named("Record")?;
    partial = partial.set_field("id", 1u32)?;
    partial = partial.end()?;

    // Set wrapper field
    partial = partial.set_field("tag", String::from("important"))?;

    // Re-enter enum to set more variant fields
    partial = partial.begin_field("item")?;
    partial = partial.set_field("name", String::from("test"))?;
    partial = partial.set_field("score", 100i32)?;
    partial = partial.end()?;

    partial = partial.finish_deferred()?;
    partial = partial.end()?;

    let result = partial.build()?.materialize::<Vec<Wrapper>>()?;
    assert_eq!(result.len(), 1);
    assert_eq!(result[0].tag, "important");
    assert_eq!(
        result[0].item,
        Item::Record {
            id: 1,
            name: String::from("test"),
            score: 100
        }
    );

    Ok(())
}

// =============================================================================
// Multiple elements with Vec growth (stress test for rope)
// =============================================================================

#[test]
fn vec_many_elements_with_deferred() -> Result<(), IPanic> {
    // Push many elements to force Vec growth
    // Each element uses deferred mode
    #[derive(Facet, Debug, PartialEq)]
    struct Item {
        id: u32,
        data: String,
    }

    let mut partial = Partial::alloc::<Vec<Item>>()?;
    partial = partial.init_list()?;

    // Push 20 elements to force multiple reallocations
    for i in 0..20 {
        partial = partial.begin_list_item()?;
        partial = partial.begin_deferred()?;
        partial = partial.set_field("id", i as u32)?;
        partial = partial.set_field("data", format!("item_{}", i))?;
        partial = partial.finish_deferred()?;
        partial = partial.end()?;
    }

    let result = partial.build()?.materialize::<Vec<Item>>()?;
    assert_eq!(result.len(), 20);
    (0..20).for_each(|i| {
        assert_eq!(result[i].id, i as u32);
        assert_eq!(result[i].data, format!("item_{}", i));
    });

    Ok(())
}

#[test]
fn vec_many_elements_interleaved_fields() -> Result<(), IPanic> {
    // Push many elements, each with interleaved field access
    #[derive(Facet, Debug, PartialEq)]
    struct Item {
        a: u32,
        b: u32,
        c: u32,
    }

    let mut partial = Partial::alloc::<Vec<Item>>()?;
    partial = partial.init_list()?;

    for i in 0..15 {
        partial = partial.begin_list_item()?;
        partial = partial.begin_deferred()?;
        // Interleaved order: c, a, b
        partial = partial.set_field("c", (i * 3 + 2) as u32)?;
        partial = partial.set_field("a", (i * 3) as u32)?;
        partial = partial.set_field("b", (i * 3 + 1) as u32)?;
        partial = partial.finish_deferred()?;
        partial = partial.end()?;
    }

    let result = partial.build()?.materialize::<Vec<Item>>()?;
    assert_eq!(result.len(), 15);
    (0..15).for_each(|i| {
        assert_eq!(result[i].a, (i * 3) as u32);
        assert_eq!(result[i].b, (i * 3 + 1) as u32);
        assert_eq!(result[i].c, (i * 3 + 2) as u32);
    });

    Ok(())
}

// =============================================================================
// Nested Vec inside Vec elements
// =============================================================================

#[test]
fn vec_of_vec_with_deferred() -> Result<(), IPanic> {
    #[derive(Facet, Debug, PartialEq)]
    struct Row {
        items: Vec<u32>,
        label: String,
    }

    let mut partial = Partial::alloc::<Vec<Row>>()?;
    partial = partial.init_list()?;

    // First row
    partial = partial.begin_list_item()?;
    partial = partial.begin_deferred()?;
    partial = partial.begin_field("items")?;
    partial = partial.init_list()?;
    partial = partial.push(1u32)?;
    partial = partial.push(2u32)?;
    partial = partial.end()?;
    partial = partial.set_field("label", String::from("row1"))?;
    partial = partial.finish_deferred()?;
    partial = partial.end()?;

    // Second row
    partial = partial.begin_list_item()?;
    partial = partial.begin_deferred()?;
    partial = partial.set_field("label", String::from("row2"))?;
    partial = partial.begin_field("items")?;
    partial = partial.init_list()?;
    partial = partial.push(3u32)?;
    partial = partial.push(4u32)?;
    partial = partial.push(5u32)?;
    partial = partial.end()?;
    partial = partial.finish_deferred()?;
    partial = partial.end()?;

    let result = partial.build()?.materialize::<Vec<Row>>()?;
    assert_eq!(result.len(), 2);
    assert_eq!(result[0].items, vec![1, 2]);
    assert_eq!(result[0].label, "row1");
    assert_eq!(result[1].items, vec![3, 4, 5]);
    assert_eq!(result[1].label, "row2");

    Ok(())
}

// =============================================================================
// Deep nesting: struct > vec > struct with deferred
// =============================================================================

#[test]
fn container_with_vec_of_deferred_structs() -> Result<(), IPanic> {
    // This test uses a single deferred session at the Container level
    // The key test is whether list elements can have interleaved field access
    // within that single deferred session
    #[derive(Facet, Debug, PartialEq)]
    struct Item {
        x: u32,
        y: u32,
    }

    #[derive(Facet, Debug, PartialEq)]
    struct Container {
        name: String,
        items: Vec<Item>,
        count: u32,
    }

    let mut partial = Partial::alloc::<Container>()?;
    partial = partial.begin_deferred()?;

    partial = partial.set_field("name", String::from("container"))?;

    partial = partial.begin_field("items")?;
    partial = partial.init_list()?;

    // Build item with interleaved fields (no nested begin_deferred)
    // The outer deferred session should handle this
    partial = partial.begin_list_item()?;
    partial = partial.set_field("y", 2u32)?; // y before x
    partial = partial.set_field("x", 1u32)?;
    partial = partial.end()?;

    partial = partial.begin_list_item()?;
    partial = partial.set_field("x", 3u32)?;
    partial = partial.set_field("y", 4u32)?;
    partial = partial.end()?;

    partial = partial.end()?; // items

    partial = partial.set_field("count", 2u32)?;

    partial = partial.finish_deferred()?;

    let result = partial.build()?.materialize::<Container>()?;
    assert_eq!(result.name, "container");
    assert_eq!(result.count, 2);
    assert_eq!(result.items.len(), 2);
    assert_eq!(result.items[0], Item { x: 1, y: 2 });
    assert_eq!(result.items[1], Item { x: 3, y: 4 });

    Ok(())
}

// =============================================================================
// Edge cases
// =============================================================================

#[test]
fn vec_single_element_deferred() -> Result<(), IPanic> {
    #[derive(Facet, Debug, PartialEq)]
    struct Item {
        value: u32,
    }

    let mut partial = Partial::alloc::<Vec<Item>>()?;
    partial = partial.init_list()?;

    partial = partial.begin_list_item()?;
    partial = partial.begin_deferred()?;
    partial = partial.set_field("value", 42u32)?;
    partial = partial.finish_deferred()?;
    partial = partial.end()?;

    let result = partial.build()?.materialize::<Vec<Item>>()?;
    assert_eq!(result.len(), 1);
    assert_eq!(result[0].value, 42);

    Ok(())
}

#[test]
fn vec_empty_with_init() -> Result<(), IPanic> {
    #[derive(Facet, Debug, PartialEq)]
    struct Item {
        value: u32,
    }

    let mut partial = Partial::alloc::<Vec<Item>>()?;
    partial = partial.init_list()?;
    // Don't push any elements

    let result = partial.build()?.materialize::<Vec<Item>>()?;
    assert!(result.is_empty());

    Ok(())
}

#[test]
fn vec_zst_elements() -> Result<(), IPanic> {
    // Zero-sized types have special handling
    #[derive(Facet, Debug, PartialEq)]
    struct Empty;

    let mut partial = Partial::alloc::<Vec<Empty>>()?;
    partial = partial.init_list()?;

    for _ in 0..5 {
        partial = partial.begin_list_item()?;
        partial = partial.set(Empty)?;
        partial = partial.end()?;
    }

    let result = partial.build()?.materialize::<Vec<Empty>>()?;
    assert_eq!(result.len(), 5);

    Ok(())
}

// =============================================================================
// The exact #2010 reproduction at Partial level
// =============================================================================

#[test]
fn issue_2010_reproduction_partial_level() -> Result<(), IPanic> {
    // This reproduces issue #2010 at the Partial API level
    // The scenario: struct with flattened internally-tagged enum inside HashMap inside Vec-like
    //
    // The key aspect: after selecting the enum variant and setting some variant fields,
    // we navigate away to sibling fields, then come back to set more variant fields.
    // This requires the enum frame to be stored for deferred processing.

    #[derive(Facet, Debug, PartialEq)]
    #[repr(u8)]
    enum Inner {
        TypeA { value: f64 },
        TypeB { alpha: f64, beta: f64 },
    }

    #[derive(Facet, Debug, PartialEq)]
    struct Item {
        inner: Inner,
        extra: Option<String>,
    }

    let mut partial = Partial::alloc::<Vec<Item>>()?;
    partial = partial.init_list()?;

    partial = partial.begin_list_item()?;
    partial = partial.begin_deferred()?;

    // Select variant and set first field (alpha)
    partial = partial.begin_field("inner")?;
    partial = partial.select_variant_named("TypeB")?;
    partial = partial.set_field("alpha", 1.0f64)?;
    partial = partial.end()?;

    // Navigate to sibling field (extra)
    partial = partial.begin_field("extra")?;
    partial = partial.begin_some()?;
    partial = partial.set(String::from("test"))?;
    partial = partial.end()?;
    partial = partial.end()?;

    // Re-enter inner to set beta (this is the failing case in #2010)
    partial = partial.begin_field("inner")?;
    partial = partial.set_field("beta", 2.0f64)?;
    partial = partial.end()?;

    partial = partial.finish_deferred()?;
    partial = partial.end()?;

    let result = partial.build()?.materialize::<Vec<Item>>()?;
    assert_eq!(result.len(), 1);
    assert_eq!(
        result[0].inner,
        Inner::TypeB {
            alpha: 1.0,
            beta: 2.0
        }
    );
    assert_eq!(result[0].extra, Some(String::from("test")));

    Ok(())
}
