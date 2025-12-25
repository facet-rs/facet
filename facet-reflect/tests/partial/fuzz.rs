//! Property-based fuzzing for Partial state machine safety.
//!
//! The goal: no matter what sequence of operations we throw at Partial,
//! it should never cause memory unsafety. Operations may fail with errors,
//! but the Partial must remain in a consistent state that can be safely dropped.
//!
//! These tests are gated behind the `fuzz-tests` feature because they're very slow.
//! Run with: `cargo nextest run -p facet-reflect --features fuzz-tests`

// In ownership-based APIs, the last assignment to `partial` is often unused
// because the value is consumed by `.build()` - this is expected behavior
#![allow(unused_assignments)]

use facet::Facet;
use facet_reflect::{Partial, Resolution};
use proptest::prelude::*;
use std::collections::HashMap;

// =============================================================================
// Target types for fuzzing
// =============================================================================

/// A complex type that exercises many Partial features
#[derive(Facet, Debug, Clone, PartialEq)]
struct FuzzTarget {
    name: String,
    count: u32,
    nested: NestedStruct,
    items: Vec<String>,
    mapping: HashMap<String, u32>,
    maybe: Option<String>,
    status: Status,
}

#[derive(Facet, Debug, Clone, PartialEq)]
struct NestedStruct {
    x: i32,
    y: i32,
    label: String,
}

#[derive(Facet, Debug, Clone, PartialEq)]
#[repr(u8)]
#[allow(dead_code)] // Variants constructed via reflection
enum Status {
    Active,
    Inactive(String),
    Pending(u32),
}

// =============================================================================
// Operation enum - all the things we can do to a Partial
// =============================================================================

#[derive(Debug, Clone)]
enum PartialOp {
    // Field operations
    BeginField(FieldName),
    BeginNthField(u8),
    SetFieldU32(FieldName, u32),
    SetFieldString(FieldName, String),
    SetU32(u32),
    SetString(String),
    SetI32(i32),

    // Navigation
    End,

    // List operations
    BeginList,
    BeginListItem,
    Push(String),

    // Map operations
    BeginMap,
    BeginKey,
    BeginValue,

    // Option operations
    BeginSome,
    BeginInner,

    // Enum operations
    SelectVariant(VariantName),

    // Deferred mode
    BeginDeferred,
    FinishDeferred,

    // Terminal
    Build,
}

/// Field names we'll use in fuzzing
#[derive(Debug, Clone, Copy)]
enum FieldName {
    Name,
    Count,
    Nested,
    Items,
    Mapping,
    Maybe,
    Status,
    X,
    Y,
    Label,
    Bogus, // Invalid field name
}

impl FieldName {
    fn as_str(&self) -> &'static str {
        match self {
            FieldName::Name => "name",
            FieldName::Count => "count",
            FieldName::Nested => "nested",
            FieldName::Items => "items",
            FieldName::Mapping => "mapping",
            FieldName::Maybe => "maybe",
            FieldName::Status => "status",
            FieldName::X => "x",
            FieldName::Y => "y",
            FieldName::Label => "label",
            FieldName::Bogus => "this_field_does_not_exist",
        }
    }
}

#[derive(Debug, Clone, Copy)]
enum VariantName {
    Active,
    Inactive,
    Pending,
    Bogus,
}

impl VariantName {
    fn as_str(&self) -> &'static str {
        match self {
            VariantName::Active => "Active",
            VariantName::Inactive => "Inactive",
            VariantName::Pending => "Pending",
            VariantName::Bogus => "NonExistentVariant",
        }
    }
}

// =============================================================================
// Proptest strategies
// =============================================================================

fn field_name_strategy() -> impl Strategy<Value = FieldName> {
    prop_oneof![
        Just(FieldName::Name),
        Just(FieldName::Count),
        Just(FieldName::Nested),
        Just(FieldName::Items),
        Just(FieldName::Mapping),
        Just(FieldName::Maybe),
        Just(FieldName::Status),
        Just(FieldName::X),
        Just(FieldName::Y),
        Just(FieldName::Label),
        Just(FieldName::Bogus),
    ]
}

fn variant_name_strategy() -> impl Strategy<Value = VariantName> {
    prop_oneof![
        Just(VariantName::Active),
        Just(VariantName::Inactive),
        Just(VariantName::Pending),
        Just(VariantName::Bogus),
    ]
}

fn small_string_strategy() -> impl Strategy<Value = String> {
    prop_oneof![
        Just(String::new()),
        Just(String::from("a")),
        Just(String::from("test")),
        Just(String::from("hello world")),
        "[a-z]{0,10}".prop_map(String::from),
    ]
}

fn partial_op_strategy() -> impl Strategy<Value = PartialOp> {
    prop_oneof![
        // Field operations (weighted higher since they're common)
        3 => field_name_strategy().prop_map(PartialOp::BeginField),
        2 => (0u8..10).prop_map(PartialOp::BeginNthField),
        2 => (field_name_strategy(), any::<u32>())
            .prop_map(|(f, v)| PartialOp::SetFieldU32(f, v)),
        2 => (field_name_strategy(), small_string_strategy())
            .prop_map(|(f, v)| PartialOp::SetFieldString(f, v)),
        2 => any::<u32>().prop_map(PartialOp::SetU32),
        2 => small_string_strategy().prop_map(PartialOp::SetString),
        2 => any::<i32>().prop_map(PartialOp::SetI32),
        // Navigation (weighted higher since it's needed to balance begins)
        5 => Just(PartialOp::End),
        // List operations
        2 => Just(PartialOp::BeginList),
        2 => Just(PartialOp::BeginListItem),
        2 => small_string_strategy().prop_map(PartialOp::Push),
        // Map operations
        1 => Just(PartialOp::BeginMap),
        1 => Just(PartialOp::BeginKey),
        1 => Just(PartialOp::BeginValue),
        // Option operations
        1 => Just(PartialOp::BeginSome),
        1 => Just(PartialOp::BeginInner),
        // Enum operations
        2 => variant_name_strategy().prop_map(PartialOp::SelectVariant),
        // Deferred mode (low weight - typically called once per session)
        1 => Just(PartialOp::BeginDeferred),
        1 => Just(PartialOp::FinishDeferred),
        // Terminal (low weight - we want longer sequences)
        1 => Just(PartialOp::Build),
    ]
}

/// Generate a sequence of operations
fn op_sequence_strategy() -> impl Strategy<Value = Vec<PartialOp>> {
    prop::collection::vec(partial_op_strategy(), 1..50)
}

// =============================================================================
// The actual fuzzing logic
// =============================================================================

/// Apply a sequence of operations to a Partial.
/// Returns Ok if we successfully built something, Err if any operation failed.
/// The key property: this function should NEVER panic or cause UB.
fn apply_ops(ops: &[PartialOp]) -> Result<(), String> {
    let mut partial: Partial<'_> =
        Partial::alloc::<FuzzTarget>().map_err(|e| format!("alloc failed: {e}"))?;

    for (i, op) in ops.iter().enumerate() {
        match apply_single_op(partial, op) {
            Ok(p) => partial = p,
            Err(e) => {
                // Operation failed - that's fine, we just stop here
                // The partial will be dropped and should clean up properly
                return Err(format!("op {i} ({op:?}) failed: {e}"));
            }
        }
    }

    Ok(())
}

fn apply_single_op<'a>(partial: Partial<'a>, op: &'a PartialOp) -> Result<Partial<'a>, String> {
    let partial = match op {
        PartialOp::BeginField(field) => partial
            .begin_field(field.as_str())
            .map_err(|e| e.to_string())?,
        PartialOp::BeginNthField(idx) => partial
            .begin_nth_field(*idx as usize)
            .map_err(|e| e.to_string())?,
        PartialOp::SetFieldU32(field, value) => partial
            .set_field(field.as_str(), *value)
            .map_err(|e| e.to_string())?,
        PartialOp::SetFieldString(field, value) => partial
            .set_field(field.as_str(), value.clone())
            .map_err(|e| e.to_string())?,
        PartialOp::SetU32(value) => partial.set(*value).map_err(|e| e.to_string())?,
        PartialOp::SetString(value) => partial.set(value.clone()).map_err(|e| e.to_string())?,
        PartialOp::SetI32(value) => partial.set(*value).map_err(|e| e.to_string())?,
        PartialOp::End => partial.end().map_err(|e| e.to_string())?,
        PartialOp::BeginList => partial.begin_list().map_err(|e| e.to_string())?,
        PartialOp::BeginListItem => partial.begin_list_item().map_err(|e| e.to_string())?,
        PartialOp::Push(value) => partial.push(value.clone()).map_err(|e| e.to_string())?,
        PartialOp::BeginMap => partial.begin_map().map_err(|e| e.to_string())?,
        PartialOp::BeginKey => partial.begin_key().map_err(|e| e.to_string())?,
        PartialOp::BeginValue => partial.begin_value().map_err(|e| e.to_string())?,
        PartialOp::BeginSome => partial.begin_some().map_err(|e| e.to_string())?,
        PartialOp::BeginInner => partial.begin_inner().map_err(|e| e.to_string())?,
        PartialOp::SelectVariant(variant) => partial
            .select_variant_named(variant.as_str())
            .map_err(|e| e.to_string())?,
        PartialOp::BeginDeferred => {
            // Create a fresh resolution for deferred mode
            let resolution = Resolution::new();
            partial
                .begin_deferred(resolution)
                .map_err(|e| e.to_string())?
        }
        PartialOp::FinishDeferred => partial.finish_deferred().map_err(|e| e.to_string())?,
        PartialOp::Build => {
            // Build consumes the Partial and returns HeapValue
            // Signal success by returning a special error to end the loop
            partial.build().map_err(|e| e.to_string())?;
            return Err("build_succeeded".to_string());
        }
    };
    Ok(partial)
}

// =============================================================================
// Tests
// =============================================================================

proptest! {
    #![proptest_config(ProptestConfig {
        cases: 1000,
        failure_persistence: None,
        ..ProptestConfig::default()
    })]

    /// The core property: no sequence of operations should cause a panic or UB.
    /// Operations may fail, but the Partial should always be safely droppable.
    #[test]
    #[cfg_attr(any(miri, not(feature = "fuzz-tests")), ignore)]
    fn fuzz_partial_safety(ops in op_sequence_strategy()) {
        // This should never panic - errors are expected and fine
        let _ = apply_ops(&ops);
        // If we get here without panicking, the test passes
    }
}

/// Simpler test targets for more focused fuzzing
#[derive(Facet, Debug)]
struct SimpleStruct {
    a: u32,
    b: String,
}

#[derive(Facet, Debug)]
struct WithList {
    items: Vec<u32>,
}

#[derive(Facet, Debug)]
struct WithMap {
    data: HashMap<String, u32>,
}

proptest! {
    #![proptest_config(ProptestConfig {
        cases: 500,
        failure_persistence: None,
        ..ProptestConfig::default()
    })]

    /// Fuzz just struct field operations
    #[test]
    #[cfg_attr(any(miri, not(feature = "fuzz-tests")), ignore)]
    fn fuzz_simple_struct(
        ops in prop::collection::vec(
            prop_oneof![
                Just("begin_a"),
                Just("begin_b"),
                Just("begin_bogus"),
                Just("set_u32"),
                Just("set_string"),
                Just("end"),
                Just("build"),
            ],
            1..30
        ),
        values_u32 in prop::collection::vec(any::<u32>(), 10),
        values_str in prop::collection::vec("[a-z]{0,5}", 10),
    ) {
        let mut partial: Partial<'_> = Partial::alloc::<SimpleStruct>().unwrap();
        let mut u32_idx = 0;
        let mut str_idx = 0;

        for op in ops {
            // Handle build separately since it consumes Partial and returns HeapValue
            if op == "build" {
                let _ = partial.build();
                break;
            }
            let result = match op {
                "begin_a" => partial.begin_field("a"),
                "begin_b" => partial.begin_field("b"),
                "begin_bogus" => partial.begin_field("bogus"),
                "set_u32" => {
                    let v = values_u32[u32_idx % values_u32.len()];
                    u32_idx += 1;
                    partial.set(v)
                }
                "set_string" => {
                    let v = &values_str[str_idx % values_str.len()];
                    str_idx += 1;
                    partial.set(v.clone())
                }
                "end" => partial.end(),
                _ => Ok(partial),
            };
            match result {
                Ok(p) => partial = p,
                Err(_) => break,
            }
        }
        // Partial dropped here - should not leak or crash
    }

    /// Fuzz list operations
    #[test]
    #[cfg_attr(any(miri, not(feature = "fuzz-tests")), ignore)]
    fn fuzz_list_ops(
        ops in prop::collection::vec(
            prop_oneof![
                Just("begin_items"),
                Just("begin_list"),
                Just("begin_list_item"),
                Just("push"),
                Just("set"),
                Just("end"),
                Just("build"),
            ],
            1..40
        ),
        values in prop::collection::vec(any::<u32>(), 20),
    ) {
        let mut partial: Partial<'_> = Partial::alloc::<WithList>().unwrap();
        let mut idx = 0;

        for op in ops {
            // Handle build separately since it consumes Partial and returns HeapValue
            if op == "build" {
                let _ = partial.build();
                break;
            }
            let result = match op {
                "begin_items" => partial.begin_field("items"),
                "begin_list" => partial.begin_list(),
                "begin_list_item" => partial.begin_list_item(),
                "push" => {
                    let v = values[idx % values.len()];
                    idx += 1;
                    partial.push(v)
                }
                "set" => {
                    let v = values[idx % values.len()];
                    idx += 1;
                    partial.set(v)
                }
                "end" => partial.end(),
                _ => Ok(partial),
            };
            match result {
                Ok(p) => partial = p,
                Err(_) => break,
            }
        }
    }

    /// Fuzz map operations
    #[test]
    #[cfg_attr(any(miri, not(feature = "fuzz-tests")), ignore)]
    fn fuzz_map_ops(
        ops in prop::collection::vec(
            prop_oneof![
                Just("begin_data"),
                Just("begin_map"),
                Just("begin_key"),
                Just("begin_value"),
                Just("set_key"),
                Just("set_value"),
                Just("end"),
                Just("build"),
            ],
            1..40
        ),
        keys in prop::collection::vec("[a-z]{1,5}", 10),
        values in prop::collection::vec(any::<u32>(), 10),
    ) {
        let mut partial: Partial<'_> = Partial::alloc::<WithMap>().unwrap();
        let mut key_idx = 0;
        let mut val_idx = 0;

        for op in ops {
            // Handle build separately since it consumes Partial and returns HeapValue
            if op == "build" {
                let _ = partial.build();
                break;
            }
            let result = match op {
                "begin_data" => partial.begin_field("data"),
                "begin_map" => partial.begin_map(),
                "begin_key" => partial.begin_key(),
                "begin_value" => partial.begin_value(),
                "set_key" => {
                    let k = &keys[key_idx % keys.len()];
                    key_idx += 1;
                    partial.set(k.clone())
                }
                "set_value" => {
                    let v = values[val_idx % values.len()];
                    val_idx += 1;
                    partial.set(v)
                }
                "end" => partial.end(),
                _ => Ok(partial),
            };
            match result {
                Ok(p) => partial = p,
                Err(_) => break,
            }
        }
    }
}

// =============================================================================
// Miri-compatible tests (wip_ prefix, no proptest)
// =============================================================================

/// A few hand-picked sequences that exercise edge cases, runnable under Miri
#[::core::prelude::v1::test]
fn wip_fuzz_drop_after_partial_init() {
    // Start building, set some fields, drop without finishing
    let partial: Partial<'_> = Partial::alloc::<FuzzTarget>().unwrap();
    let partial = partial.set_field("name", String::from("test")).ok();
    if let Some(partial) = partial {
        let _ = partial.set_field("count", 42u32);
    }
    // Drop here
}

#[::core::prelude::v1::test]
fn wip_fuzz_drop_mid_nested() {
    // Navigate into nested struct, set a field, drop
    let partial: Partial<'_> = Partial::alloc::<FuzzTarget>().unwrap();
    let partial = partial.begin_field("nested").ok();
    if let Some(partial) = partial {
        let _ = partial.set_field("x", 10i32);
    }
    // Drop with frame stack: [FuzzTarget, NestedStruct]
}

#[::core::prelude::v1::test]
fn wip_fuzz_drop_mid_list() {
    // Start building a list, add some items, drop
    let partial: Partial<'_> = Partial::alloc::<FuzzTarget>().unwrap();
    if let Ok(partial) = partial.begin_field("items")
        && let Ok(partial) = partial.begin_list()
        && let Ok(partial) = partial.push(String::from("item1"))
    {
        let _ = partial.push(String::from("item2"));
    }
    // Drop with list partially built
}

#[::core::prelude::v1::test]
fn wip_fuzz_drop_mid_map() {
    // Start building a map, add a key, drop before value
    let partial: Partial<'_> = Partial::alloc::<FuzzTarget>().unwrap();
    if let Ok(partial) = partial.begin_field("mapping")
        && let Ok(partial) = partial.begin_map()
        && let Ok(partial) = partial.begin_key()
        && let Ok(partial) = partial.set(String::from("key1"))
        && let Ok(partial) = partial.end()
    {
        // end key
        let _ = partial.begin_value();
    }
    // Drop with map in "pushing value" state
}

#[::core::prelude::v1::test]
fn wip_fuzz_invalid_ops_sequence() {
    // Try a bunch of invalid operations - should all return errors, not panic

    // Try to end when there's nothing to end
    let partial: Partial<'_> = Partial::alloc::<FuzzTarget>().unwrap();
    assert!(partial.end().is_err());

    // Try to begin_list on a struct
    let partial: Partial<'_> = Partial::alloc::<FuzzTarget>().unwrap();
    assert!(partial.begin_list().is_err());

    // Try to set a value on a struct (need to select field first)
    let partial: Partial<'_> = Partial::alloc::<FuzzTarget>().unwrap();
    assert!(partial.set(42u32).is_err());

    // Try invalid field name
    let partial: Partial<'_> = Partial::alloc::<FuzzTarget>().unwrap();
    assert!(partial.begin_field("nonexistent").is_err());

    // Try to build incomplete struct
    let partial: Partial<'_> = Partial::alloc::<FuzzTarget>().unwrap();
    assert!(partial.build().is_err());
}

#[::core::prelude::v1::test]
fn wip_fuzz_deferred_drop_without_finish() {
    // Enter deferred mode, do some work, drop without finish_deferred
    let partial: Partial<'_> = Partial::alloc::<FuzzTarget>().unwrap();
    let resolution = Resolution::new();
    let mut partial = partial.begin_deferred(resolution).unwrap();

    partial = partial
        .set_field("name", String::from("test"))
        .ok()
        .unwrap();
    partial = partial.begin_field("nested").ok().unwrap();
    partial = partial.set_field("x", 1i32).ok().unwrap();
    let _ = partial.end();
    // Drop without calling finish_deferred - should clean up properly
}

#[::core::prelude::v1::test]
fn wip_fuzz_deferred_interleaved_fields() {
    // Test the re-entry pattern that deferred mode is designed for
    let partial: Partial<'_> = Partial::alloc::<FuzzTarget>().unwrap();
    let resolution = Resolution::new();
    let mut partial = partial.begin_deferred(resolution).unwrap();

    // First visit to nested
    partial = partial.begin_field("nested").ok().unwrap();
    partial = partial.set_field("x", 1i32).ok().unwrap();
    partial = partial.end().ok().unwrap();

    // Set a top-level field
    partial = partial
        .set_field("name", String::from("test"))
        .ok()
        .unwrap();

    // Re-enter nested
    partial = partial.begin_field("nested").ok().unwrap();
    partial = partial.set_field("y", 2i32).ok().unwrap();
    let _ = partial.end();

    // Drop without finishing - tests stored frame cleanup
}

#[::core::prelude::v1::test]
fn wip_fuzz_deferred_double_begin() {
    // Calling begin_deferred twice should return an error on the second call
    let partial: Partial<'_> = Partial::alloc::<FuzzTarget>().unwrap();
    let resolution1 = Resolution::new();
    let resolution2 = Resolution::new();

    let partial = partial.begin_deferred(resolution1).unwrap();
    assert!(partial.begin_deferred(resolution2).is_err()); // Second call should error (partial consumed)

    // Note: partial was consumed by the error above, so we can't use it anymore
    // Drop
}

#[::core::prelude::v1::test]
fn wip_fuzz_deferred_finish_without_begin() {
    // Calling finish_deferred without begin_deferred
    let partial: Partial<'_> = Partial::alloc::<FuzzTarget>().unwrap();
    let result = partial.finish_deferred();
    // Should return an error, not panic
    assert!(result.is_err());
}

/// Reproducer for fuzz artifact f28ffd5b1dc26c052afaef7f862f83396d8798c5
/// Operations: BeginField(Name), SetString("aaaaaaaaaaaa")
#[::core::prelude::v1::test]
fn wip_fuzz_begin_field_set_string_drop() {
    let partial: Partial<'_> = Partial::alloc::<FuzzTarget>().unwrap();
    // BeginField(Name)
    if let Ok(partial) = partial.begin_field("name") {
        // SetString("aaaaaaaaaaaa")
        let _ = partial.set(String::from("aaaaaaaaaaaa"));
    }
    // Partial dropped here - must not leak or crash
}

#[::core::prelude::v1::test]
fn wip_fuzz_reg_test_1() {
    let mut partial: Partial<'_> = Partial::alloc::<facet_value::Value>().unwrap();
    partial = partial.begin_map().unwrap();
    partial = partial.begin_object_entry("foo").unwrap();
    partial = partial.begin_map().unwrap();
    partial = partial.end().unwrap();
    partial = partial.begin_object_entry("foo").unwrap();
    partial.set_default().unwrap();
}

#[::core::prelude::v1::test]
fn wip_fuzz_reg_test_2() {
    let mut partial: Partial<'_> = Partial::alloc::<FuzzTarget>().unwrap();
    partial = partial.begin_field("mapping").unwrap();
    partial = partial.begin_map().unwrap();
    partial = partial.begin_key().unwrap();
    partial = partial.set(String::from("aaaaaaaaaaaaaaaa")).unwrap();
}

#[::core::prelude::v1::test]
fn wip_fuzz_reg_test_3() {
    let mut partial: Partial<'_> = Partial::alloc::<facet_value::Value>().unwrap();
    partial = partial.begin_map().unwrap();
    partial = partial.begin_object_entry("").unwrap();
    partial = partial.begin_map().unwrap();
    partial = partial.end().unwrap();
    partial = partial.begin_object_entry("").unwrap();
    partial = partial.set(522133289i32).unwrap();
    partial = partial.begin_map().unwrap();
    let _ = partial.begin_field("name");
}

#[::core::prelude::v1::test]
fn wip_fuzz_reg_test_4() {
    let mut partial: Partial<'_> = Partial::alloc::<facet_value::Value>().unwrap();
    partial = partial.set(522133289i32).unwrap();
    partial = partial.begin_map().unwrap();
    partial = partial.begin_object_entry("").unwrap();
    partial = partial.begin_map().unwrap();
    partial = partial.end().unwrap();
    partial = partial.begin_object_entry("").unwrap();
    partial = partial.set(1179662i32).unwrap();
    let _ = partial.begin_list().unwrap();
}

#[::core::prelude::v1::test]
fn wip_fuzz_reg_test_5() {
    let mut partial: Partial<'_> = Partial::alloc::<FuzzTarget>().unwrap();
    partial = partial.begin_field("mapping").unwrap();
    partial = partial.begin_map().unwrap();
    partial = partial.begin_key().unwrap();
    partial = partial.set(String::from("mxwvhqpvvv")).unwrap();
    let _ = partial.begin_inner();
}

#[::core::prelude::v1::test]
fn wip_fuzz_reg_test_6() {
    let mut partial: Partial<'_> = Partial::alloc::<facet_value::Value>().unwrap();
    partial = partial.begin_map().unwrap();
    partial = partial.begin_object_entry("").unwrap();
    partial = partial.set_default().unwrap();
    partial = partial.set(530521897i32).unwrap();
    partial = partial.end().unwrap();
    partial = partial.begin_object_entry("").unwrap();
    let _ = partial.begin_inner();
}
