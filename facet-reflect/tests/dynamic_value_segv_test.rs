use facet_reflect::{Partial, Resolution};
use facet_value::Value;

/// Test that DynamicValue (Value) works correctly with deferred mode and list operations.
/// This reproduces a SEGV found by fuzzing.
#[test]
fn test_dynamic_value_deferred_list_segv() {
    let mut partial = Partial::alloc::<Value>().unwrap();
    let p = partial.inner_mut();

    // BeginList - initializes as dynamic array
    let r = p.begin_list();
    eprintln!("begin_list: {:?}", r.is_ok());

    // BeginDeferred - enters deferred mode
    let resolution = Resolution::new();
    let r = p.begin_deferred(resolution);
    eprintln!("begin_deferred: {:?}", r.is_ok());

    // BeginListItem - pushes element frame
    let r = p.begin_list_item();
    eprintln!("begin_list_item: {:?}", r.is_ok());

    // End - in deferred mode
    let r = p.end();
    eprintln!("end: {:?}", r.is_ok());

    // Drop
    drop(partial);
    eprintln!("Dropped!");
}
