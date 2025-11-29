use facet::Facet;
use facet_reflect::{Partial, Resolution};
use std::collections::HashMap;

/// Helper to safely get shape for debug printing - returns "<inactive>" if poisoned
fn shape_str(partial: &Partial<'_>) -> String {
    partial
        .try_shape()
        .map(|s| s.to_string())
        .unwrap_or_else(|| "<inactive>".to_string())
}

#[derive(Facet, Debug)]
struct FuzzTarget {
    name: String,
    count: u32,
    nested: NestedStruct,
    items: Vec<String>,
    mapping: HashMap<String, u32>,
    maybe: Option<String>,
}

#[derive(Facet, Debug)]
struct NestedStruct {
    x: i32,
    y: i32,
    label: String,
}

#[test]
fn test_double_free_repro() {
    let mut typed_partial = Partial::alloc::<FuzzTarget>().unwrap();
    let partial = typed_partial.inner_mut();

    // Reproduce minimized double-free test case:
    // BeginField(Nested), BeginField(Label), End, End, End, SetString("vvv"),
    // End, End, End, End, End, End, End, End, BeginField(Name), SetI32(-774778415),
    // BeginDeferred, BeginSome, BeginSome, BeginField(Label), SetI32(1027422781), End, SetI32(199)

    println!(
        "1. begin_field(nested): {:?}",
        partial.begin_field("nested").err()
    );
    println!("   frames: {}", partial.frame_count());

    println!(
        "2. begin_field(label): {:?}",
        partial.begin_field("label").err()
    );
    println!("   frames: {}", partial.frame_count());

    println!("3. end: {:?}", partial.end().err());
    println!("   frames: {}", partial.frame_count());

    println!("4. end: {:?}", partial.end().err());
    println!("   frames: {}", partial.frame_count());

    println!("5. end: {:?}", partial.end().err());
    println!("   frames: {}", partial.frame_count());

    println!("6. set(vvv): {:?}", partial.set("vvv".to_string()).err());
    println!("   frames: {}", partial.frame_count());

    for i in 7..=14 {
        println!("{}. end: {:?}", i, partial.end().err());
        println!("   frames: {}", partial.frame_count());
    }

    println!(
        "15. begin_field(name): {:?}",
        partial.begin_field("name").err()
    );
    println!("   frames: {}", partial.frame_count());

    println!("16. set(i32): {:?}", partial.set(-774778415i32).err());
    println!("   frames: {}", partial.frame_count());

    println!(
        "17. begin_deferred: {:?}",
        partial.begin_deferred(Resolution::new()).err()
    );
    println!(
        "   frames: {}, deferred: {}",
        partial.frame_count(),
        partial.is_deferred()
    );

    println!("18. begin_some: {:?}", partial.begin_some().err());
    println!("   frames: {}", partial.frame_count());

    println!("19. begin_some: {:?}", partial.begin_some().err());
    println!("   frames: {}", partial.frame_count());

    println!(
        "20. begin_field(label): {:?}",
        partial.begin_field("label").err()
    );
    println!("   frames: {}", partial.frame_count());

    println!("21. set(i32): {:?}", partial.set(1027422781i32).err());
    println!("   frames: {}", partial.frame_count());

    println!("22. end: {:?}", partial.end().err());
    println!("   frames: {}", partial.frame_count());

    println!("23. set(199): {:?}", partial.set(199i32).err());
    println!("   frames: {}", partial.frame_count());

    println!("\n=== About to drop ===");
    println!("Final frame count: {}", partial.frame_count());
    println!("Is deferred: {}", partial.is_deferred());

    // Drop happens here
}

#[test]
fn test_leak_deferred_nested_fields() {
    // Test that a failed finish_deferred properly cleans up and poisons the Partial.
    // After finish_deferred fails, the Partial is poisoned to prevent memory safety
    // issues that could arise from continuing to use partially initialized state.
    let mut typed_partial = Partial::alloc::<FuzzTarget>().unwrap();
    let partial = typed_partial.inner_mut();

    // First deferred session: set up nested.label
    partial.begin_deferred(Resolution::new()).unwrap();
    partial.begin_field("nested").unwrap();
    partial.begin_field("label").unwrap();
    partial.set(String::from("leak_me")).unwrap();
    partial.end().unwrap(); // Store label frame
    partial.end().unwrap(); // Store nested frame
    let result = partial.finish_deferred();
    assert!(result.is_err()); // Fails due to missing fields

    // After a failed finish_deferred, the Partial is poisoned and cannot be used
    // This is intentional: continuing to use a Partial after a failed deferred
    // operation could lead to double-frees or memory leaks due to inconsistent state.
    let result = partial.begin_deferred(Resolution::new());
    assert!(
        result.is_err(),
        "Partial should be poisoned after failed finish_deferred"
    );

    // Drop happens here - should not leak the "leak_me" string
}

#[test]
fn test_leak_from_fuzzer_minimized() {
    // Reproduction of minimized fuzzer artifact:
    // leak-03a7b2d4a13d1c6e0a15424cd2d581cfdc9e1dce
    let mut typed_partial = Partial::alloc::<FuzzTarget>().unwrap();
    let partial = typed_partial.inner_mut();

    // Trace each operation to understand the state
    eprintln!("1. begin_list: {:?}", partial.begin_list().err());
    eprintln!(
        "   frames={}, deferred={}",
        partial.frame_count(),
        partial.is_deferred()
    );

    eprintln!("2. end: {:?}", partial.end().err());
    eprintln!(
        "   frames={}, deferred={}",
        partial.frame_count(),
        partial.is_deferred()
    );

    eprintln!(
        "3. begin_deferred: {:?}",
        partial.begin_deferred(Resolution::new()).err()
    );
    eprintln!(
        "   frames={}, deferred={}",
        partial.frame_count(),
        partial.is_deferred()
    );

    eprintln!("4. set_default: {:?}", partial.set_default().err());
    eprintln!("5. set_default: {:?}", partial.set_default().err());
    eprintln!("6. begin_smart_ptr: {:?}", partial.begin_smart_ptr().err());

    eprintln!(
        "7. begin_field(nested): {:?}",
        partial.begin_field("nested").err()
    );
    eprintln!("   frames={}", partial.frame_count());

    eprintln!("8. set_default: {:?}", partial.set_default().err());
    eprintln!("9. set_default: {:?}", partial.set_default().err());
    eprintln!("10. begin_inner: {:?}", partial.begin_inner().err());
    eprintln!("11. begin_set: {:?}", partial.begin_set().err());

    eprintln!("12. begin_field(x): {:?}", partial.begin_field("x").err());
    eprintln!("   frames={}", partial.frame_count());

    eprintln!("13. begin_inner: {:?}", partial.begin_inner().err());
    eprintln!("14. set_default: {:?}", partial.set_default().err());

    eprintln!("15. end: {:?}", partial.end().err());
    eprintln!("   frames={}", partial.frame_count());

    eprintln!(
        "16. set_nth_field_to_default(35): {:?}",
        partial.set_nth_field_to_default(35).err()
    );
    eprintln!("17. begin_value: {:?}", partial.begin_value().err());
    eprintln!(
        "18. set_nth_field_to_default(1): {:?}",
        partial.set_nth_field_to_default(1).err()
    );

    eprintln!(
        "19. begin_field(label): {:?}",
        partial.begin_field("label").err()
    );
    eprintln!("   frames={}", partial.frame_count());

    eprintln!("20. set_default: {:?}", partial.set_default().err());
    eprintln!("21. begin_smart_ptr: {:?}", partial.begin_smart_ptr().err());
    eprintln!("22. begin_smart_ptr: {:?}", partial.begin_smart_ptr().err());
    eprintln!(
        "23. set(i64): {:?}",
        partial.set(6148914777671155005i64).err()
    );
    eprintln!("24. begin_key: {:?}", partial.begin_key().err());
    eprintln!("25. begin_value: {:?}", partial.begin_value().err());
    eprintln!("26. begin_value: {:?}", partial.begin_value().err());
    eprintln!("27. begin_value: {:?}", partial.begin_value().err());
    eprintln!("28. begin_value: {:?}", partial.begin_value().err());

    eprintln!(
        "29. set(String 'ppvvvvnn'): {:?}",
        partial.set(String::from("ppvvvvnn")).err()
    );
    eprintln!(
        "   frames={}, deferred={}",
        partial.frame_count(),
        partial.is_deferred()
    );

    eprintln!(
        "30. begin_field(count): {:?}",
        partial.begin_field("count").err()
    );

    eprintln!("31. end: {:?}", partial.end().err());
    eprintln!(
        "   frames={}, deferred={}",
        partial.frame_count(),
        partial.is_deferred()
    );

    eprintln!(
        "32. set_nth_field_to_default(219): {:?}",
        partial.set_nth_field_to_default(219).err()
    );
    eprintln!("33. begin_smart_ptr: {:?}", partial.begin_smart_ptr().err());
    eprintln!(
        "34. begin_field(items): {:?}",
        partial.begin_field("items").err()
    );
    eprintln!("35. begin_set: {:?}", partial.begin_set().err());
    eprintln!("36. set_default: {:?}", partial.set_default().err());

    eprintln!("37. end: {:?}", partial.end().err());
    eprintln!(
        "   frames={}, deferred={}",
        partial.frame_count(),
        partial.is_deferred()
    );

    eprintln!(
        "38. set_nth_field_to_default(95): {:?}",
        partial.set_nth_field_to_default(95).err()
    );
    eprintln!("39. begin_value: {:?}", partial.begin_value().err());
    eprintln!(
        "40. set_nth_field_to_default(1): {:?}",
        partial.set_nth_field_to_default(1).err()
    );

    eprintln!("41. finish_deferred: {:?}", partial.finish_deferred().err());
    eprintln!(
        "   frames={}, deferred={}",
        partial.frame_count(),
        partial.is_deferred()
    );

    eprintln!(
        "42. begin_deferred: {:?}",
        partial.begin_deferred(Resolution::new()).err()
    );
    eprintln!(
        "   frames={}, deferred={}",
        partial.frame_count(),
        partial.is_deferred()
    );

    eprintln!(
        "43. begin_deferred (2nd): {:?}",
        partial.begin_deferred(Resolution::new()).err()
    );
    eprintln!("44. set(bool): {:?}", partial.set(true).err());
    eprintln!(
        "45. begin_field(mapping): {:?}",
        partial.begin_field("mapping").err()
    );
    eprintln!("46. begin_list: {:?}", partial.begin_list().err());

    eprintln!("47. end: {:?}", partial.end().err());
    eprintln!(
        "   frames={}, deferred={}",
        partial.frame_count(),
        partial.is_deferred()
    );

    eprintln!(
        "48. begin_deferred: {:?}",
        partial.begin_deferred(Resolution::new()).err()
    );
    eprintln!("49. set_default: {:?}", partial.set_default().err());
    eprintln!("50. set_default: {:?}", partial.set_default().err());
    eprintln!("51. begin_smart_ptr: {:?}", partial.begin_smart_ptr().err());
    eprintln!(
        "52. begin_field(nested): {:?}",
        partial.begin_field("nested").err()
    );
    eprintln!("53. set_default: {:?}", partial.set_default().err());
    eprintln!("54. begin_smart_ptr: {:?}", partial.begin_smart_ptr().err());
    eprintln!("55. begin_smart_ptr: {:?}", partial.begin_smart_ptr().err());
    eprintln!("56. begin_set: {:?}", partial.begin_set().err());
    eprintln!("57. begin_field(x): {:?}", partial.begin_field("x").err());
    eprintln!("58. begin_inner: {:?}", partial.begin_inner().err());
    eprintln!("59. set_default: {:?}", partial.set_default().err());

    eprintln!("60. end: {:?}", partial.end().err());
    eprintln!(
        "   frames={}, deferred={}",
        partial.frame_count(),
        partial.is_deferred()
    );

    eprintln!(
        "61. set_nth_field_to_default(35): {:?}",
        partial.set_nth_field_to_default(35).err()
    );
    eprintln!("62. begin_value: {:?}", partial.begin_value().err());
    eprintln!(
        "63. begin_deferred: {:?}",
        partial.begin_deferred(Resolution::new()).err()
    );
    eprintln!(
        "64. begin_nth_field(85): {:?}",
        partial.begin_nth_field(85).err()
    );

    eprintln!("\n=== About to drop ===");
    eprintln!(
        "Final: frames={}, deferred={}",
        partial.frame_count(),
        partial.is_deferred()
    );
    // Drop happens here - should not leak the String
}

#[test]
fn test_double_free_crash_6711629e() {
    // Reproduction of crash artifact: crash-6711629e8b5d6675a65055df1cef55a1495b3217
    let mut typed_partial = Partial::alloc::<FuzzTarget>().unwrap();
    let partial = typed_partial.inner_mut();

    // SetDefault x 3
    let _ = partial.set_default();
    let _ = partial.set_default();
    let _ = partial.set_default();

    // BeginList, End
    let _ = partial.begin_list();
    let _ = partial.end();

    // BeginDeferred
    let _ = partial.begin_deferred(Resolution::new());

    // SetDefault x 2
    let _ = partial.set_default();
    let _ = partial.set_default();

    // BeginSmartPtr
    let _ = partial.begin_smart_ptr();

    // BeginField(Nested)
    let _ = partial.begin_field("nested");

    // BeginSet
    let _ = partial.begin_set();

    // BeginField(X)
    let _ = partial.begin_field("x");

    // BeginInner
    let _ = partial.begin_inner();

    // SetDefault
    let _ = partial.set_default();

    // End
    let _ = partial.end();

    // SetNthFieldToDefault(35)
    let _ = partial.set_nth_field_to_default(35);

    // BeginValue
    let _ = partial.begin_value();

    // SetNthFieldToDefault(1)
    let _ = partial.set_nth_field_to_default(1);

    // BeginField(Label)
    let _ = partial.begin_field("label");

    // SetDefault
    let _ = partial.set_default();

    // BeginSet x 2
    let _ = partial.begin_set();
    let _ = partial.begin_set();

    // BeginInner x 2
    let _ = partial.begin_inner();
    let _ = partial.begin_inner();

    // SetDefault
    let _ = partial.set_default();

    // End
    let _ = partial.end();

    // SetNthFieldToDefault(35)
    let _ = partial.set_nth_field_to_default(35);

    // BeginValue
    let _ = partial.begin_value();

    // SetNthFieldToDefault(1)
    let _ = partial.set_nth_field_to_default(1);

    // FinishDeferred (first)
    eprintln!(
        "First FinishDeferred: {:?}",
        partial.finish_deferred().err()
    );
    eprintln!(
        "After first finish: frames={}, deferred={}, active={}",
        partial.frame_count(),
        partial.is_deferred(),
        partial.is_active()
    );
    if partial.is_active() {
        eprintln!("Current shape: {}", shape_str(partial));
    } else {
        eprintln!("Partial is no longer active (poisoned or built)");
    }

    // End
    eprintln!("About to call end() after first finish_deferred");
    let end_result = partial.end();
    eprintln!("end() result: {:?}", end_result.err());
    eprintln!("After end: frames={}", partial.frame_count());

    // BeginList, End
    eprintln!("About to begin_list");
    let _ = partial.begin_list();
    eprintln!("begin_list done, frames={}", partial.frame_count());
    let _ = partial.end();
    eprintln!(
        "end after begin_list done, frames={}",
        partial.frame_count()
    );

    // BeginSmartPtr
    eprintln!("About to begin_smart_ptr");
    let _ = partial.begin_smart_ptr();
    eprintln!("begin_smart_ptr done, frames={}", partial.frame_count());

    // SetI32(1027423549)
    eprintln!("About to set i32");
    let _ = partial.set(1027423549i32);
    eprintln!("set i32 done");

    // SetDefault x 4
    eprintln!("set_default 1");
    let _ = partial.set_default();
    eprintln!("set_default 2");
    let _ = partial.set_default();
    eprintln!("set_default 3");
    let _ = partial.set_default();
    eprintln!("set_default 4");
    let _ = partial.set_default();
    eprintln!("All set_defaults done");

    // BeginList, End
    eprintln!("begin_list 2");
    let _ = partial.begin_list();
    eprintln!("end 2");
    let _ = partial.end();

    // BeginDeferred (second)
    eprintln!("begin_deferred 2");
    let _ = partial.begin_deferred(Resolution::new());
    eprintln!("begin_deferred 2 done");

    // SetDefault x 2
    let _ = partial.set_default();
    let _ = partial.set_default();

    // BeginSmartPtr
    let _ = partial.begin_smart_ptr();

    // BeginField(Nested)
    let _ = partial.begin_field("nested");

    // BeginSet
    let _ = partial.begin_set();

    // BeginField(X)
    let _ = partial.begin_field("x");

    // BeginInner
    let _ = partial.begin_inner();

    // SetDefault
    let _ = partial.set_default();

    // End
    let _ = partial.end();

    // SetNthFieldToDefault(35)
    let _ = partial.set_nth_field_to_default(35);

    // BeginValue
    let _ = partial.begin_value();

    // SetNthFieldToDefault(223)
    let _ = partial.set_nth_field_to_default(223);

    // BeginField(Label)
    let _ = partial.begin_field("label");

    // SetDefault
    let _ = partial.set_default();

    // BeginSet x 2
    let _ = partial.begin_set();
    let _ = partial.begin_set();

    // BeginInner
    let _ = partial.begin_inner();

    // SetDefault
    let _ = partial.set_default();

    // BeginSet x 2
    let _ = partial.begin_set();
    let _ = partial.begin_set();

    // SetString("blbbbbbbddrrzrr")
    let _ = partial.set(String::from("blbbbbbbddrrzrr"));

    // SetNthFieldToDefault(35)
    let _ = partial.set_nth_field_to_default(35);

    // BeginValue
    let _ = partial.begin_value();

    // SetI32(66977534)
    let _ = partial.set(66977534i32);

    // SetU32(3076480863)
    let _ = partial.set(3076480863u32);

    // BeginInner
    let _ = partial.begin_inner();

    // End
    let _ = partial.end();

    // SetDefault x 2
    let _ = partial.set_default();
    let _ = partial.set_default();

    // BeginSmartPtr x 2
    let _ = partial.begin_smart_ptr();
    let _ = partial.begin_smart_ptr();

    // BeginField(Items)
    let _ = partial.begin_field("items");

    // BeginListItem
    let _ = partial.begin_list_item();

    // SetDefault
    let _ = partial.set_default();

    // BeginInner
    let _ = partial.begin_inner();

    // BeginSet
    let _ = partial.begin_set();

    // BeginDeferred (third - nested)
    let _ = partial.begin_deferred(Resolution::new());

    // BeginNthField(239)
    let _ = partial.begin_nth_field(239);

    // SetDefault
    let _ = partial.set_default();

    // FinishDeferred (nested)
    eprintln!(
        "Nested FinishDeferred: {:?}",
        partial.finish_deferred().err()
    );

    // End
    let _ = partial.end();

    // SetString("aa")
    let _ = partial.set(String::from("aa"));

    eprintln!("\n=== About to drop ===");
    eprintln!(
        "Final: frames={}, deferred={}",
        partial.frame_count(),
        partial.is_deferred()
    );
    // Drop happens here
}

#[test]
fn test_leak_map_partial_insert_not_in_parent_iset() {
    // Regression test for leak: when a Map has partial insert state (PushingKey/PushingValue)
    // and end() was never called successfully, the Map is not marked in the parent's iset.
    // On Drop, we need to both cleanup partial state AND drop the collection itself.
    //
    // Minimized from fuzzer artifact: leak-6f09ee3ad2f0d3c9ab363e8c2a5b366417d4eb49
    let mut typed_partial = Partial::alloc::<FuzzTarget>().unwrap();
    let partial = typed_partial.inner_mut();

    // Enter the mapping field (HashMap<String, u32>)
    assert!(partial.begin_field("mapping").is_ok());

    // Initialize the map
    assert!(partial.begin_map().is_ok());

    // Start a key, set it to "qce"
    assert!(partial.begin_key().is_ok());
    assert!(partial.set("qce".to_string()).is_ok());
    assert!(partial.end().is_ok()); // Key done, now in PushingValue state

    // Start value but set it
    assert!(partial.begin_value().is_ok());
    assert!(partial.set(42u32).is_ok());
    assert!(partial.end().is_ok()); // Value done, insertion complete

    // Start another key but don't complete it
    assert!(partial.begin_key().is_ok());
    // Key NOT set - map is in PushingKey state with uninitialized key buffer

    // Drop happens here.
    // The mapping field was never "ended", so it's not in FuzzTarget's iset.
    // The fix ensures we both cleanup partial state (key buffer) AND drop
    // the HashMap (which contains the "qce" -> 42 pair).
}

#[test]
fn test_double_free_minimal() {
    // Minimal reproduction of double-free
    // The key from the crash test is that finish_deferred SUCCEEDS (returns Ok)
    // This requires all fields to have defaults or be set

    // Use a simpler struct with all-defaultable fields
    #[derive(Facet, Debug, Default)]
    struct SimpleStruct {
        #[facet(default)]
        name: String,
        #[facet(default)]
        count: u32,
    }

    let mut typed_partial = Partial::alloc::<SimpleStruct>().unwrap();
    let partial = typed_partial.inner_mut();

    // Set up deferred mode
    eprintln!("=== Deferred session ===");
    assert!(partial.begin_deferred(Resolution::new()).is_ok());

    // Navigate to name and set it
    eprintln!("begin_field(name): {:?}", partial.begin_field("name").err());
    eprintln!("frames: {}", partial.frame_count());

    eprintln!(
        "set(String): {:?}",
        partial.set(String::from("test_string")).err()
    );

    // End name frame (stores in stored_frames)
    eprintln!("end: {:?}", partial.end().err());
    eprintln!("frames: {}", partial.frame_count());

    // Finish deferred - should SUCCEED because count has a default
    let result = partial.finish_deferred();
    eprintln!("finish_deferred: {:?}", result.err());
    eprintln!(
        "frames: {}, deferred: {}",
        partial.frame_count(),
        partial.is_deferred()
    );

    eprintln!("\n=== About to drop ===");
    // Drop happens here - does it double-free?
}

#[test]
fn test_double_free_minimal_nested() {
    // Test with nested struct like the original crash
    #[derive(Facet, Debug, Default)]
    struct Outer {
        #[facet(default)]
        inner: Inner,
        #[facet(default)]
        other: u32,
    }

    #[derive(Facet, Debug, Default)]
    struct Inner {
        #[facet(default)]
        label: String,
        #[facet(default)]
        value: i32,
    }

    let mut typed_partial = Partial::alloc::<Outer>().unwrap();
    let partial = typed_partial.inner_mut();

    eprintln!("=== Deferred session ===");
    assert!(partial.begin_deferred(Resolution::new()).is_ok());

    // Navigate to inner.label
    eprintln!(
        "begin_field(inner): {:?}",
        partial.begin_field("inner").err()
    );
    eprintln!("frames: {}", partial.frame_count());

    eprintln!(
        "begin_field(label): {:?}",
        partial.begin_field("label").err()
    );
    eprintln!("frames: {}", partial.frame_count());

    // Set label
    eprintln!(
        "set(String): {:?}",
        partial.set(String::from("test_label")).err()
    );

    // End label
    eprintln!("end (label): {:?}", partial.end().err());
    eprintln!("frames: {}", partial.frame_count());

    // End inner
    eprintln!("end (inner): {:?}", partial.end().err());
    eprintln!("frames: {}", partial.frame_count());

    // Finish deferred - should succeed (all fields have defaults)
    let result = partial.finish_deferred();
    eprintln!("finish_deferred: {:?}", result.err());
    eprintln!(
        "frames: {}, deferred: {}",
        partial.frame_count(),
        partial.is_deferred()
    );

    eprintln!("\n=== About to drop ===");
}

#[test]
fn test_double_free_with_extra_frame_on_stack() {
    // The crash test has frames=2 AFTER successful finish_deferred
    // This means there's still a frame on the stack besides root
    // Let me replicate that scenario
    #[derive(Facet, Debug, Default)]
    struct Outer {
        #[facet(default)]
        inner: Inner,
        #[facet(default)]
        other: u32,
    }

    #[derive(Facet, Debug, Default)]
    struct Inner {
        #[facet(default)]
        label: String,
        #[facet(default)]
        value: i32,
    }

    let mut typed_partial = Partial::alloc::<Outer>().unwrap();
    let partial = typed_partial.inner_mut();

    // Navigate to inner (don't end it - stays on stack)
    eprintln!(
        "begin_field(inner): {:?}",
        partial.begin_field("inner").err()
    );
    eprintln!("frames: {}", partial.frame_count()); // Should be 2

    // Now begin deferred from this position
    eprintln!(
        "begin_deferred: {:?}",
        partial.begin_deferred(Resolution::new()).err()
    );
    eprintln!(
        "frames: {}, deferred: {}",
        partial.frame_count(),
        partial.is_deferred()
    );

    // Navigate to label (child of inner)
    eprintln!(
        "begin_field(label): {:?}",
        partial.begin_field("label").err()
    );
    eprintln!("frames: {}", partial.frame_count()); // Should be 3

    // Set label
    eprintln!(
        "set(String): {:?}",
        partial.set(String::from("test_label")).err()
    );

    // End label (stores in stored_frames)
    eprintln!("end (label): {:?}", partial.end().err());
    eprintln!("frames: {}", partial.frame_count()); // Should be 2

    // Finish deferred - inner frame is on stack, label is in stored_frames
    let result = partial.finish_deferred();
    eprintln!("finish_deferred: {:?}", result.err());
    eprintln!(
        "frames: {}, deferred: {}",
        partial.frame_count(),
        partial.is_deferred()
    );

    // Now we have frames=2 and deferred=false
    // This is similar to the crash test state

    eprintln!("\n=== About to drop ===");
}

#[test]
fn test_double_free_simple_repro() {
    // Minimal reproduction of the double-free issue:
    // 1. Set up a struct with a String field
    // 2. Navigate into the struct, mark it as initialized
    // 3. Navigate into the String field, drop/replace it
    // 4. Call prepare_for_reinitialization on the struct (via begin_inner)
    // 5. prepare_for_reinitialization calls drop_in_place on the whole struct,
    //    which tries to drop the String again -> DOUBLE FREE

    #[derive(Facet, Debug, Default)]
    struct Outer {
        #[facet(default)]
        label: String,
        #[facet(default)]
        value: i32,
    }

    let mut typed_partial = Partial::alloc::<Outer>().unwrap();
    let partial = typed_partial.inner_mut();

    // First, fully initialize Outer with a string
    partial.begin_field("label").unwrap();
    partial.set(String::from("original_string")).unwrap();
    partial.end().unwrap();

    partial.begin_field("value").unwrap();
    partial.set(42i32).unwrap();
    partial.end().unwrap();

    // Now Outer is fully initialized with tracker=Struct{iset: all set}
    eprintln!(
        "After init: frames={}, shape={}",
        partial.frame_count(),
        shape_str(partial)
    );

    // Re-enter the label field
    partial.begin_field("label").unwrap();
    // At this point: label frame has is_init=true, parent Outer.iset[label] is cleared
    eprintln!("After begin_field(label): frames={}", partial.frame_count());

    // Replace the label with a new string
    // set_default calls deinit() which drops "original_string", then writes empty string
    partial.set_default().unwrap();
    eprintln!("After set_default: frames={}", partial.frame_count());

    // End label - returns to Outer frame
    // Parent's iset[label] gets set again
    partial.end().unwrap();
    eprintln!(
        "After end: frames={}, shape={}",
        partial.frame_count(),
        shape_str(partial)
    );

    // Now Outer has is_init=true, tracker=Struct{iset: all set}
    // The label field contains the DEFAULT empty string (not "original_string")

    // Calling begin_inner() will call prepare_for_reinitialization()
    // which checks is_init=true and calls drop_in_place on Outer
    // This should work fine - it drops the empty string and value 42
    eprintln!("About to call begin_inner (should fail but not crash)");
    let result = partial.begin_inner();
    eprintln!("begin_inner result: {:?}", result.err());

    eprintln!("=== About to drop ===");
}

#[test]
fn test_double_free_truncated() {
    // Truncated crash test - stop right after first finish_deferred to see if crash is from drop
    let mut typed_partial = Partial::alloc::<FuzzTarget>().unwrap();
    let partial = typed_partial.inner_mut();

    // SetDefault x 3
    let _ = partial.set_default();
    let _ = partial.set_default();
    let _ = partial.set_default();

    // BeginList, End
    let _ = partial.begin_list();
    let _ = partial.end();

    // BeginDeferred
    let _ = partial.begin_deferred(Resolution::new());

    // SetDefault x 2
    let _ = partial.set_default();
    let _ = partial.set_default();

    // BeginSmartPtr
    let _ = partial.begin_smart_ptr();

    // BeginField(Nested)
    let _ = partial.begin_field("nested");

    // BeginSet
    let _ = partial.begin_set();

    // BeginField(X)
    let _ = partial.begin_field("x");

    // BeginInner
    let _ = partial.begin_inner();

    // SetDefault
    let _ = partial.set_default();

    // End
    let _ = partial.end();

    // SetNthFieldToDefault(35)
    let _ = partial.set_nth_field_to_default(35);

    // BeginValue
    let _ = partial.begin_value();

    // SetNthFieldToDefault(1)
    let _ = partial.set_nth_field_to_default(1);

    // BeginField(Label)
    let _ = partial.begin_field("label");

    // SetDefault
    let _ = partial.set_default();

    // BeginSet x 2
    let _ = partial.begin_set();
    let _ = partial.begin_set();

    // BeginInner x 2
    let _ = partial.begin_inner();
    let _ = partial.begin_inner();

    // SetDefault
    let _ = partial.set_default();

    // End
    let _ = partial.end();

    // SetNthFieldToDefault(35)
    let _ = partial.set_nth_field_to_default(35);

    // BeginValue
    let _ = partial.begin_value();

    // SetNthFieldToDefault(1)
    let _ = partial.set_nth_field_to_default(1);

    // FinishDeferred (first)
    eprintln!(
        "First FinishDeferred: {:?}",
        partial.finish_deferred().err()
    );
    eprintln!(
        "After first finish: frames={}, deferred={}, active={}",
        partial.frame_count(),
        partial.is_deferred(),
        partial.is_active()
    );
    if partial.is_active() {
        eprintln!("Current shape: {}", shape_str(partial));
    } else {
        eprintln!("Partial is no longer active (poisoned or built)");
    }

    // Operations after first finish_deferred - add one by one to find crash
    eprintln!("end()");
    let _ = partial.end();
    eprintln!("frames={}", partial.frame_count());

    eprintln!("begin_list");
    let _ = partial.begin_list();
    eprintln!("end");
    let _ = partial.end();
    eprintln!("frames={}", partial.frame_count());

    eprintln!("begin_smart_ptr");
    let _ = partial.begin_smart_ptr();
    eprintln!("frames={}", partial.frame_count());

    eprintln!("set i32");
    let _ = partial.set(1027423549i32);
    eprintln!("frames={}", partial.frame_count());

    eprintln!("set_default x 4");
    let _ = partial.set_default();
    let _ = partial.set_default();
    let _ = partial.set_default();
    let _ = partial.set_default();
    eprintln!("frames={}", partial.frame_count());

    eprintln!("begin_list");
    let _ = partial.begin_list();
    eprintln!("end");
    let _ = partial.end();
    eprintln!("frames={}", partial.frame_count());

    eprintln!("begin_deferred 2");
    let _ = partial.begin_deferred(Resolution::new());
    eprintln!(
        "frames={}, deferred={}",
        partial.frame_count(),
        partial.is_deferred()
    );

    // Operations in second deferred session
    eprintln!("sd2-1");
    let _ = partial.set_default();
    eprintln!("sd2-2");
    let _ = partial.set_default();
    eprintln!("bsp2");
    let _ = partial.begin_smart_ptr();
    eprintln!("bf nested 2");
    let _ = partial.begin_field("nested");
    eprintln!("frames={}", partial.frame_count());

    // More operations from full test
    eprintln!("bs2");
    let _ = partial.begin_set();
    eprintln!("bf x 2");
    let _ = partial.begin_field("x");
    eprintln!("frames={}", partial.frame_count());

    eprintln!("bi2");
    let _ = partial.begin_inner();
    eprintln!("sd2-3");
    let _ = partial.set_default();
    eprintln!("end2");
    let _ = partial.end();
    eprintln!("frames={}", partial.frame_count());

    eprintln!("sntfd");
    let _ = partial.set_nth_field_to_default(35);
    eprintln!("bv");
    let _ = partial.begin_value();
    eprintln!("sntfd2");
    let _ = partial.set_nth_field_to_default(223);

    eprintln!("bf label 2");
    let _ = partial.begin_field("label");
    eprintln!("frames={}", partial.frame_count());

    eprintln!("sd2-4 shape={}", shape_str(partial));
    let _ = partial.set_default();
    eprintln!(
        "after sd2-4: frames={}, shape={}",
        partial.frame_count(),
        shape_str(partial)
    );

    eprintln!("bs x 2");
    let _ = partial.begin_set();
    let _ = partial.begin_set();

    eprintln!("bi on {}", shape_str(partial));
    let r = partial.begin_inner();
    eprintln!(
        "bi result: {:?}, frames={}, shape={}",
        r.err(),
        partial.frame_count(),
        shape_str(partial)
    );

    eprintln!("sd on {}", shape_str(partial));
    let _ = partial.set_default();
    eprintln!(
        "after sd: frames={}, shape={}",
        partial.frame_count(),
        shape_str(partial)
    );

    eprintln!("bs2-2");
    let _ = partial.begin_set();
    let _ = partial.begin_set();

    eprintln!("set string 'blbbbbbbddrrzrr' on {}", shape_str(partial));
    let r = partial.set(String::from("blbbbbbbddrrzrr"));
    eprintln!(
        "set result: {:?}, frames={}",
        r.err(),
        partial.frame_count()
    );

    eprintln!("sntfd-35 on {}", shape_str(partial));
    let _ = partial.set_nth_field_to_default(35);

    eprintln!("bv on {}", shape_str(partial));
    let _ = partial.begin_value();

    eprintln!("set i32 66977534 on {}", shape_str(partial));
    let _ = partial.set(66977534i32);

    eprintln!("set u32 on {}", shape_str(partial));
    let _ = partial.set(3076480863u32);

    eprintln!("bi on {}", shape_str(partial));
    let r = partial.begin_inner();
    eprintln!("bi result: {:?}, frames={}", r.err(), partial.frame_count());

    eprintln!("end on {}", shape_str(partial));
    let r = partial.end();
    eprintln!(
        "end result: {:?}, frames={}, shape={}",
        r.err(),
        partial.frame_count(),
        shape_str(partial)
    );

    eprintln!("sd x 2 on {}", shape_str(partial));
    let r = partial.set_default();
    eprintln!("sd1 result: {:?}", r.err());
    let r = partial.set_default();
    eprintln!(
        "sd2 result: {:?}, frames={}",
        r.err(),
        partial.frame_count()
    );

    eprintln!("bsp x 2");
    let _ = partial.begin_smart_ptr();
    let _ = partial.begin_smart_ptr();

    eprintln!("bf items on {}", shape_str(partial));
    let r = partial.begin_field("items");
    eprintln!(
        "bf items result: {:?}, frames={}",
        r.err(),
        partial.frame_count()
    );

    eprintln!("bli on {}", shape_str(partial));
    let r = partial.begin_list_item();
    eprintln!("bli result: {:?}", r.err());

    eprintln!("sd on {}", shape_str(partial));
    let r = partial.set_default();
    eprintln!("sd result: {:?}", r.err());

    eprintln!("bi (crash?) on {}", shape_str(partial));
    let r = partial.begin_inner();
    eprintln!("bi result: {:?}", r.err());

    eprintln!("=== About to drop ===");
}

#[test]
fn test_leak_092c76fc() {
    // Reproduction of leak artifact: leak-092c76fcb6f7f6bae36e389cbaa14cc08f1a55d2
    // 1 byte leaked - the "h" string allocated at step 23
    //
    // Root cause: In deferred mode, when a parent frame's tracker gets reset to Scalar
    // (e.g., by a failed set_default calling deinit()), the mark_field_initialized()
    // function couldn't update the parent's iset because there was no iset to update.
    // This meant child fields weren't tracked for cleanup, causing leaks.
    //
    // Fix: In mark_field_initialized(), if the tracker is Scalar but the shape is a
    // struct type, upgrade the tracker to Struct with a fresh iset before setting bits.
    let mut typed_partial = Partial::alloc::<FuzzTarget>().unwrap();
    let p = typed_partial.inner_mut();

    let _ = p.begin_list();
    let _ = p.end();
    let _ = p.begin_deferred(Resolution::new());
    let _ = p.set_default();
    let _ = p.set_default();
    let _ = p.begin_smart_ptr();
    let _ = p.begin_field("nested");
    let _ = p.set_default();
    let _ = p.set_default();
    let _ = p.begin_inner();
    let _ = p.begin_set();
    let _ = p.begin_field("x");
    let _ = p.begin_inner();
    let _ = p.set_default();
    let _ = p.end();
    let _ = p.set_nth_field_to_default(35);
    let _ = p.begin_value();
    let _ = p.set_nth_field_to_default(1);
    let _ = p.begin_field("label");
    let _ = p.set_default();
    let _ = p.begin_deferred(Resolution::new());
    let _ = p.set(false);
    let _ = p.begin_inner();
    let _ = p.set(String::from("h")); // This string was leaking!
    let _ = p.begin_list();
    let _ = p.end();
    let _ = p.end();
    let _ = p.set_default();
    let _ = p.set_default();
    let _ = p.begin_smart_ptr();
    let _ = p.begin_field("nested");
    let _ = p.begin_set();
    let _ = p.begin_deferred(Resolution::new());
    let _ = p.begin_deferred(Resolution::new());
    let _ = p.end();
    let _ = p.begin_value();
    let _ = p.set_nth_field_to_default(223);
    let _ = p.begin_field("y");
    let _ = p.begin_deferred(Resolution::new());
    let _ = p.begin_deferred(Resolution::new());
    let _ = p.begin_value();
    let _ = p.begin_field("mapping");
    let _ = p.begin_list();
    let _ = p.end();
    let _ = p.begin_deferred(Resolution::new());
    let _ = p.set_default();
    let _ = p.set_default();
    let _ = p.begin_smart_ptr();
    let _ = p.begin_field("nested");
    let _ = p.set_default();
    let _ = p.set_default();
    let _ = p.begin_inner();
    let _ = p.begin_set();
    let _ = p.begin_field("x");
    let _ = p.begin_inner();
    let _ = p.set_default();
    let _ = p.end();
    let _ = p.set_nth_field_to_default(35);
    let _ = p.begin_value();
    let _ = p.set_nth_field_to_default(1);
    let _ = p.set(1608474463u32);
    let _ = p.set_nth_field_to_default(223);
    let _ = p.end();
    let _ = p.set_default();
    let _ = p.set_default();
    let _ = p.begin_smart_ptr();
    let _ = p.begin_smart_ptr();
    let _ = p.end();
    let _ = p.end();
    let _ = p.set_nth_field_to_default(223);
    let _ = p.finish_deferred();
    let _ = p.end();
    let _ = p.begin_field("name");
    // Drop - should not leak
}
