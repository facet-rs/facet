use facet::Facet;
use facet_reflect::Partial;
use facet_testhelpers::test;

#[test]
fn wip_option_testleak1() {
    let wip = Partial::alloc::<Option<String>>()
        .unwrap()
        .set(Some(String::from("Hello, world!")))
        .unwrap();
    let _ = wip.build().unwrap();
}

#[test]
fn wip_option_testleak2() {
    let wip = Partial::alloc::<Option<String>>()
        .unwrap()
        .set(Some(String::from("Hello, world!")))
        .unwrap();
    let _wip = wip.build().unwrap();
}

#[test]
fn wip_option_testleak3() {
    let _wip = Partial::alloc::<Option<String>>()
        .unwrap()
        .set(Some(String::from("Hello, world!")))
        .unwrap();
    // Don't call build() to test partial initialization
}

#[test]
fn wip_option_testleak4() {
    let _wip = Partial::alloc::<Option<String>>()
        .unwrap()
        .set(Some(String::from("Hello, world!")))
        .unwrap();
    // Don't call build() to test partial initialization
}

#[test]
fn wip_option_testleak5() {
    let _ = Partial::alloc::<Option<String>>().unwrap();
    // Just allocate without setting a value
}

#[test]
fn wip_option_testleak6() {
    let _ = Partial::alloc::<Option<String>>().unwrap();
}

/// Reproduces issue #1568 - use-after-free in Option cleanup
///
/// The bug: when begin_some() allocates memory for the inner value,
/// and then cleanup happens (drop without finishing), the child frame's
/// memory is freed via dealloc(), but the parent frame's deinit() still
/// tries to drop the Option contents that point to the freed memory.
#[test]
fn wip_option_use_after_free_issue_1568() {
    #[derive(Facet, Debug)]
    struct WithOption {
        name: String,
        opt: Option<String>,
    }

    // Allocate and start building a struct
    let mut partial = Partial::alloc::<WithOption>().unwrap();

    // Set the first field
    partial = partial.set_field("name", String::from("test")).unwrap();

    // Begin the option field
    partial = partial.begin_field("opt").unwrap();

    // Call begin_some - this allocates separate memory for the Option's inner
    partial = partial.begin_some().unwrap();

    // Set the inner string value
    partial = partial.set(String::from("hello")).unwrap();

    // Don't call end() - just drop the partial
    // This triggers the use-after-free: the child frame (Some's inner) gets
    // deallocated, but then the parent frame's deinit tries to drop the Option
    // which still thinks it contains Some pointing to the freed memory
    drop(partial);
}

/// Test deeply nested Option where inner struct deserialization fails
/// This mimics what happens in the HTML deserializer when parsing fails mid-structure
#[test]
fn wip_option_nested_struct_partial_init_issue_1568() {
    #[derive(Facet, Debug)]
    struct Inner {
        x: u32,
        y: String,
    }

    #[derive(Facet, Debug)]
    struct Outer {
        inner: Option<Inner>,
    }

    // Allocate and start building a struct
    let mut partial = Partial::alloc::<Outer>().unwrap();

    // Begin the option field
    partial = partial.begin_field("inner").unwrap();

    // Call begin_some - this allocates separate memory for Option's inner
    partial = partial.begin_some().unwrap();

    // Partially initialize Inner - set one field but not the other
    partial = partial.set_field("x", 42u32).unwrap();

    // Drop without finishing - simulates what happens when deserialization errors out
    drop(partial);
}

/// Test Option<String> directly with begin_some + partial state
#[test]
fn wip_option_string_begin_some_drop_issue_1568() {
    // Allocate Option<String> directly
    let mut partial = Partial::alloc::<Option<String>>().unwrap();

    // Call begin_some - this allocates separate memory for the inner String
    partial = partial.begin_some().unwrap();

    // Set a string value
    partial = partial.set(String::from("hello world")).unwrap();

    // Drop without calling end() - should not crash
    drop(partial);
}

/// Reproduces issue #1568 with deferred mode + flattened struct with Option fields
/// This mimics what facet-html's deserializer does with GlobalAttrs
#[test]
fn wip_option_deferred_flatten_issue_1568() {
    use std::collections::HashMap;

    #[derive(Facet, Debug, Default)]
    struct Html {
        #[facet(flatten)]
        attrs: GlobalAttrs,
    }

    #[derive(Facet, Debug, Default)]
    struct GlobalAttrs {
        id: Option<String>,
        class: Option<String>,
        #[facet(flatten, default)]
        extra: HashMap<String, String>,
    }

    // Allocate the struct
    let mut partial = Partial::alloc::<Html>().unwrap();

    // Enter deferred mode (like FormatDeserializer does for structs with flatten)
    partial = partial.begin_deferred().unwrap();

    // Navigate to the flattened attrs field
    partial = partial.begin_field("attrs").unwrap();

    // Navigate to an Option<String> field inside the flattened struct
    partial = partial.begin_field("id").unwrap();

    // Start a Some value - this allocates separate memory
    partial = partial.begin_some().unwrap();

    // Set the string value
    partial = partial.set(String::from("my-id")).unwrap();

    // Drop without finishing - should trigger the bug
    drop(partial);
}

/// Found by fuzzer: enum begin_nth_field then drop
#[test]
fn fuzz_enum_begin_field_drop() {
    #[derive(Facet, Debug)]
    #[repr(u8)]
    #[allow(dead_code)] // Variants are constructed via reflection, not accessed directly
    enum FuzzEnum {
        Unit,
        Tuple(String, u32),
        Struct { name: String, value: Option<i32> },
    }

    // This sequence was found by the fuzzer
    let partial = Partial::alloc::<FuzzEnum>().unwrap();
    let partial = partial.begin_nth_field(1);
    drop(partial);
}

/// Found by fuzzer: SmartPointerSlice builder leak in Field frames
/// Minimized case: BeginNthField(1) -> BeginSmartPtr on Arc<[u8]>
#[test]
fn fuzz_smart_ptr_slice_field_leak() {
    use std::sync::Arc;

    #[derive(Facet, Debug)]
    struct SmartPtrTarget {
        arc_string: Arc<String>,
        arc_slice: Arc<[u8]>,
    }

    // This sequence was found by the fuzzer - begin_smart_ptr on Arc<[u8]> field then drop
    let partial = Partial::alloc::<SmartPtrTarget>().unwrap();
    let partial = partial.begin_nth_field(1); // arc_slice field
    if let Ok(partial) = partial
        && let Ok(partial) = partial.begin_smart_ptr()
    {
        drop(partial); // Should free the slice builder
    }
}

/// Found by fuzzer: map key leak when parse_from_str is called on key
/// Minimized case: BeginField(Mapping) -> BeginMap -> BeginKey -> ParseFromStr -> ParseFromStr
#[test]
fn fuzz_map_key_leak_minimized() {
    use std::collections::HashMap;

    #[derive(Facet, Debug)]
    struct FuzzTarget {
        mapping: HashMap<String, u32>,
    }

    // This EXACT sequence was found by the fuzzer - TWO parse_from_str calls!
    let partial = Partial::alloc::<FuzzTarget>().unwrap();
    let partial = partial.begin_field("mapping").unwrap();
    let partial = partial.init_map().unwrap();
    let partial = partial.begin_key().unwrap();
    // First parse - initializes the key to "appppvpejv"
    let partial = partial.parse_from_str("appppvpejv").unwrap();
    // Second parse - this drops the old string and parses "pv"
    // The leak happens here!
    let _ = partial.parse_from_str("pv");
}

/// Found by fuzzer: DynamicValue Object leak when nested BeginMap + BeginObjectEntry
/// then error occurs
#[test]
fn fuzz_dynamic_value_nested_object_leak() {
    use facet_value::Value;

    // Create a DynamicValue (facet_value::Value)
    let partial = Partial::alloc::<Value>().unwrap();

    // First BeginMap creates the root Object
    let partial = partial.init_map().unwrap();

    // BeginObjectEntry pushes a value frame and stores the key
    let partial = partial.begin_object_entry("key1").unwrap();

    // BeginMap on the value creates a nested Object
    let partial = partial.init_map().unwrap();

    // Now drop without finishing - should not leak
    drop(partial);
}

/// More complex nested Object case from fuzzer
#[test]
fn fuzz_dynamic_value_deeply_nested_object_leak() {
    use facet_value::Value;

    let partial = Partial::alloc::<Value>().unwrap();
    let partial = partial.init_map().unwrap();
    let partial = partial.begin_object_entry("outer").unwrap();
    let partial = partial.init_map().unwrap();
    let partial = partial.begin_object_entry("inner").unwrap();
    let partial = partial.init_map().unwrap();
    // End two levels
    let partial = partial.end().unwrap();
    let partial = partial.end().unwrap();
    // Start another nested entry
    let partial = partial.begin_object_entry("second").unwrap();
    let partial = partial.init_map().unwrap();

    // Drop without finishing
    drop(partial);
}

/// Exact sequence from fuzzer that was leaking
/// BeginCustomDeserialization on DynamicValue fails but Partial should still clean up
#[test]
fn fuzz_dynamic_value_exact_fuzzer_sequence() {
    use facet_value::Value;

    let partial = Partial::alloc::<Value>().unwrap();
    let partial = partial.init_map().unwrap();
    // Second init_map is early return (already Object)
    let partial = partial.init_map().unwrap();

    let partial = partial.begin_object_entry("rl").unwrap();
    let partial = partial.init_map().unwrap();
    let partial = partial.begin_object_entry("ff").unwrap();
    let partial = partial.init_map().unwrap();
    let partial = partial.end().unwrap();
    let partial = partial.end().unwrap();

    // More init_map (early return)
    let partial = partial.init_map().unwrap();
    let partial = partial.init_map().unwrap();

    let partial = partial.begin_object_entry("rl").unwrap();
    let partial = partial.init_map().unwrap();

    // begin_custom_deserialization will fail for DynamicValue (no parent field)
    // This consumes the Partial and should clean up all frames
    let result = partial.begin_custom_deserialization();
    assert!(result.is_err()); // Should fail
    // Partial is dropped inside begin_custom_deserialization
}

/// Simpler test: just Object with one entry, then drop
#[test]
fn fuzz_dynamic_value_simple_object_with_entry() {
    use facet_value::Value;

    let partial = Partial::alloc::<Value>().unwrap();
    let partial = partial.init_map().unwrap();

    let partial = partial.begin_object_entry("key").unwrap();
    let partial = partial.init_map().unwrap();
    let partial = partial.end().unwrap(); // Insert entry into object

    // Object now has one entry
    // Drop without finishing
    drop(partial);
}

/// Test: Object with entry, then re-enter same key (BorrowedInPlace path)
#[test]
fn fuzz_dynamic_value_reenter_existing_key() {
    use facet_value::Value;

    let partial = Partial::alloc::<Value>().unwrap();
    let partial = partial.init_map().unwrap();

    // First entry "key" -> nested Object
    let partial = partial.begin_object_entry("key").unwrap();
    let partial = partial.init_map().unwrap();
    let partial = partial.end().unwrap(); // Insert entry into object

    // Re-enter same key - uses BorrowedInPlace path
    let partial = partial.begin_object_entry("key").unwrap();
    // init_map on BorrowedInPlace frame with is_init=true should early return
    let partial = partial.init_map().unwrap();

    // Drop - BorrowedInPlace frame should be skipped, root Object should be dropped
    drop(partial);
}

/// Regression test for fuzzer-found crash: BorrowedInPlace frame pointing into parent
/// Object's HashMap entry. When deinit_for_replace drops the Value, the parent's entry
/// still exists but contains garbage. When parent Object drops, use-after-free occurs.
///
/// Fix: After dropping a BorrowedInPlace DynamicValue, reinitialize with set_null so
/// the parent can safely drop it later.
#[test]
fn fuzz_dynamic_value_borrowed_in_place_use_after_free() {
    use facet_value::Value;

    // Minimized from fuzzer crash artifact
    let partial = Partial::alloc::<Value>().unwrap();
    let partial = partial.begin_deferred().unwrap();
    let partial = partial.init_map().unwrap();
    let partial = partial.begin_object_entry("key1").unwrap();
    let partial = partial.init_map().unwrap();
    let partial = partial.init_map().unwrap();
    let partial = partial.end().unwrap();
    let partial = partial.init_map().unwrap();
    let partial = partial.init_map().unwrap();

    // Re-enter same key - BorrowedInPlace frame pointing to existing Object
    let partial = partial.begin_object_entry("key1").unwrap();

    // parse_from_str fails, but deinit_for_replace already dropped the Object.
    // CRITICAL: After dropping, we must reinitialize to Null so the parent
    // can safely drop the entry later. Without this fix, the parent would
    // access freed memory when dropping.
    let _ = partial.parse_from_str("test");

    // The Partial is now dropped (either from error or from this scope).
    // Parent Object cleanup should not crash.
}

/// Regression test for fuzzer-found leak: BorrowedInPlace frame with Number value,
/// then init_list converts to Array without dropping the Number.
///
/// The bug: init_list called deinit() for Tracker::DynamicValue{Scalar} state,
/// but deinit() early-returns for BorrowedInPlace frames without dropping.
/// Fix: use deinit_for_replace() instead of deinit() in init_list.
#[test]
fn fuzz_dynamic_value_borrowed_in_place_begin_list_leak() {
    use facet_value::Value;

    // Minimized from fuzzer artifact
    let partial = Partial::alloc::<Value>().unwrap();
    let partial = partial.set(5570193308531891999_i64).unwrap();
    let partial = partial.init_map().unwrap();
    let partial = partial.init_map().unwrap();
    let partial = partial.begin_object_entry("key1").unwrap();
    let partial = partial.init_list().unwrap();
    let partial = partial.init_list().unwrap();
    let partial = partial.set(1296911643_i32).unwrap();
    let partial = partial.init_list().unwrap();
    let partial = partial.end().unwrap();
    let partial = partial.init_map().unwrap();
    let partial = partial.init_map().unwrap();

    // Re-enter same key - BorrowedInPlace frame pointing to existing Array
    let partial = partial.begin_object_entry("key1").unwrap();
    // This SetI32 creates a Number in BorrowedInPlace frame
    let partial = partial.set(1296911643_i32).unwrap();
    // This init_list converts to Array - MUST drop the Number first!
    let partial = partial.init_list().unwrap();

    drop(partial);
}

/// Regression test for fuzzer-found leak: BorrowedInPlace frame with Number value,
/// then init_map converts to Object without dropping the Number.
///
/// The bug: init_map called deinit() for Tracker::DynamicValue{Scalar} state,
/// but deinit() early-returns for BorrowedInPlace frames without dropping.
/// Fix: use deinit_for_replace() instead of deinit() in init_map.
#[test]
fn fuzz_dynamic_value_borrowed_in_place_begin_map_leak() {
    use facet_value::Value;

    // Minimized from fuzzer artifact leak-bda097fba5becb8df709465989755137abf03116
    let partial = Partial::alloc::<Value>().unwrap();
    let partial = partial.set(2676586811620664615_i64).unwrap();
    let partial = partial.init_map().unwrap();

    // First entry
    let partial = partial.begin_object_entry("key1").unwrap();
    let partial = partial.set(-1145324613_i32).unwrap();
    let partial = partial.init_map().unwrap();
    let partial = partial.init_map().unwrap();
    let partial = partial.init_map().unwrap();
    let partial = partial.init_map().unwrap();
    let partial = partial.end().unwrap();

    // Re-enter same key - BorrowedInPlace frame
    let partial = partial.begin_object_entry("key1").unwrap();
    // This SetI32 creates a Number in BorrowedInPlace frame
    let partial = partial.set(1094786333_i32).unwrap();
    // This init_map converts to Object - MUST drop the Number first!
    let partial = partial.init_map().unwrap();
    let partial = partial.begin_object_entry("key2").unwrap();

    drop(partial);
}

/// Found by fuzzer: SetI64 at root, then BeginMap should drop the Number
/// Then nested SetI32 calls should properly handle replacement
#[test]
fn fuzz_dynamic_value_set_then_begin_map_then_set() {
    use facet_value::Value;

    // Replicate the exact fuzzer sequence
    let partial = Partial::alloc::<Value>().unwrap();

    // SetI64 at root - allocates a Number (16 bytes heap for big int)
    let partial = partial.set(2676586811620664615_i64).unwrap();

    // BeginMap should deinit the Number and create Object
    let partial = partial.init_map().unwrap();

    // BeginObjectEntry
    let partial = partial.begin_object_entry("key1").unwrap();

    // BeginMap creates nested Object
    let partial = partial.init_map().unwrap();

    // End the nested object
    let partial = partial.end().unwrap();

    // More BeginMap calls (no-op since already Object)
    let partial = partial.init_map().unwrap();
    let partial = partial.init_map().unwrap();

    // Another entry
    let partial = partial.begin_object_entry("key2").unwrap();

    // SetI32 multiple times - each should properly drop previous value
    let partial = partial.set(488447261_i32).unwrap();
    let partial = partial.set(488447261_i32).unwrap();
    let partial = partial.set(488447261_i32).unwrap();
    let partial = partial.set(488447261_i32).unwrap();
    let partial = partial.set(-1145324771_i32).unwrap();

    // More nested structure
    let partial = partial.init_map().unwrap();
    let partial = partial.init_map().unwrap();
    let partial = partial.init_map().unwrap();
    let partial = partial.init_map().unwrap();
    let partial = partial.end().unwrap();

    let partial = partial.init_map().unwrap();
    let partial = partial.init_map().unwrap();
    let partial = partial.begin_object_entry("key3").unwrap();

    let partial = partial.set(488447261_i32).unwrap();
    let partial = partial.set(488447261_i32).unwrap();
    let partial = partial.set(488447261_i32).unwrap();
    let partial = partial.set(1092427037_i32).unwrap();

    // More nesting
    let partial = partial.init_map().unwrap();
    let partial = partial.begin_object_entry("key4").unwrap();

    drop(partial);
}
