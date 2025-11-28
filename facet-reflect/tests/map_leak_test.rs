use facet::Facet;
use facet_reflect::Partial;
use std::collections::HashMap;

#[derive(Facet, Debug)]
struct FuzzTarget {
    name: String,
    count: u32,
    mapping: HashMap<String, u32>,
}

/// Test that a map with a partially completed insert (key initialized, awaiting value)
/// doesn't leak memory when prepare_for_reinitialization is called (e.g., via begin_inner).
///
/// This reproduces a bug found by fuzzing where begin_inner() would call
/// prepare_for_reinitialization(), which would drop the HashMap and reset the tracker
/// to Scalar, but would NOT clean up the allocated key buffer in the PushingValue state.
#[test]
fn test_map_leak_on_reinit_during_partial_insert() {
    let mut partial = Partial::alloc::<FuzzTarget>().unwrap();
    let p = partial.inner_mut();

    // Set up a map with a partial insert
    let _ = p.begin_field("mapping");
    let _ = p.begin_map();
    let _ = p.begin_key();
    let _ = p.set("p".to_string());
    let _ = p.end(); // Transitions to PushingValue { key_ptr, value_ptr: None }

    // Now call begin_inner which triggers prepare_for_reinitialization
    // This used to leak the key buffer because cleanup_partial_state wasn't called
    let _ = p.begin_inner();

    // Drop - should not leak the key "p"
    drop(partial);
}
