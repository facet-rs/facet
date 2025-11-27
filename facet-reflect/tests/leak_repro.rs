use facet::Facet;
use facet_reflect::{Partial, Resolution};
use std::collections::HashMap;

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
