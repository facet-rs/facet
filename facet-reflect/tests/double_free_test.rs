use facet_reflect::{Partial, Resolution};
use facet_value::Value;

/// Test various scenarios that might trigger double-free in DynamicValue + deferred mode
#[test]
fn test_dynamic_value_double_free_scenario_1() {
    // Scenario: BeginList -> BeginDeferred -> BeginListItem -> End (store element) -> more ops
    let mut partial = Partial::alloc::<Value>().unwrap();
    let p = partial.inner_mut();

    let _ = p.begin_list();
    let _ = p.begin_deferred(Resolution::new());
    let _ = p.begin_list_item();
    let _ = p.set("test".to_string());
    let _ = p.end();
    // Element frame is now stored in stored_frames
    let _ = p.begin_list_item();
    let _ = p.set("test2".to_string());
    let _ = p.end();
    // Another element stored
    drop(partial);
    eprintln!("Scenario 1: Dropped!");
}

#[test]
fn test_dynamic_value_double_free_scenario_2() {
    // Scenario: Multiple deferred attempts and nested operations
    let mut partial = Partial::alloc::<Value>().unwrap();
    let p = partial.inner_mut();

    // Try to enter deferred mode twice (second should fail)
    let _ = p.begin_deferred(Resolution::new());
    let _ = p.begin_deferred(Resolution::new()); // Should fail but continue
    let _ = p.set("test".to_string());
    drop(partial);
    eprintln!("Scenario 2: Dropped!");
}

#[test]
fn test_dynamic_value_double_free_scenario_3() {
    // Scenario: BeginList + BeginDeferred + BeginListItem + BeginInner combo
    let mut partial = Partial::alloc::<Value>().unwrap();
    let p = partial.inner_mut();

    let _ = p.begin_list();
    let _ = p.begin_deferred(Resolution::new());
    let _ = p.begin_list_item();
    let _ = p.begin_inner(); // This might trigger prepare_for_reinitialization
    let _ = p.end();
    drop(partial);
    eprintln!("Scenario 3: Dropped!");
}

#[test]
fn test_dynamic_value_double_free_scenario_4() {
    // Scenario based on fuzzer output: mixing SetDefault, SetString, BeginInner
    let mut partial = Partial::alloc::<Value>().unwrap();
    let p = partial.inner_mut();

    let _ = p.begin_deferred(Resolution::new());
    let _ = p.set_default();
    let _ = p.set_default();
    let _ = p.set_default();
    let _ = p.set_default();
    let _ = p.set(42i32);
    let _ = p.begin_deferred(Resolution::new()); // Should fail
    let _ = p.set("rjbbhgh".to_string());
    let _ = p.begin_inner();
    let _ = p.end();
    drop(partial);
    eprintln!("Scenario 4: Dropped!");
}

#[test]
fn test_dynamic_value_double_free_scenario_5() {
    // Scenario: Deeply nested begin_list operations
    let mut partial = Partial::alloc::<Value>().unwrap();
    let p = partial.inner_mut();

    let _ = p.begin_list();
    let _ = p.begin_list();
    let _ = p.begin_deferred(Resolution::new());
    let _ = p.begin_list();
    let _ = p.begin_list();
    let _ = p.begin_deferred(Resolution::new()); // Should fail
    let _ = p.end();
    let _ = p.end();
    let _ = p.finish_deferred();
    drop(partial);
    eprintln!("Scenario 5: Dropped!");
}

#[test]
fn test_dynamic_value_double_free_scenario_6() {
    // Scenario: BeginValue operations mixed with deferred
    let mut partial = Partial::alloc::<Value>().unwrap();
    let p = partial.inner_mut();

    let _ = p.begin_list();
    let _ = p.begin_list_item();
    let _ = p.begin_list();
    let _ = p.begin_deferred(Resolution::new());
    let _ = p.begin_list_item();
    let _ = p.set("test".to_string());
    let _ = p.end();
    let _ = p.finish_deferred();
    let _ = p.end();
    let _ = p.end();
    drop(partial);
    eprintln!("Scenario 6: Dropped!");
}

#[test]
fn test_dynamic_value_nested_list_with_string() {
    // Focus on the 7-byte string case
    let mut partial = Partial::alloc::<Value>().unwrap();
    let p = partial.inner_mut();

    // Create a nested array structure
    let _ = p.begin_list(); // Outer array
    let _ = p.begin_list_item();
    let _ = p.begin_list(); // Inner array
    let _ = p.begin_deferred(Resolution::new());
    let _ = p.begin_list_item();
    let _ = p.set("1234567".to_string()); // 7 bytes
    let _ = p.end();
    // Drop without finishing deferred
    drop(partial);
    eprintln!("Nested list with string: Dropped!");
}
