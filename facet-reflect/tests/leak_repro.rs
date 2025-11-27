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
fn test_leak_repro() {
    let mut typed_partial = Partial::alloc::<FuzzTarget>().unwrap();
    let partial = typed_partial.inner_mut();
    
    // Reproduce minimized test case
    let _ = partial.begin_field("x");  // Fails - X not on FuzzTarget
    let _ = partial.end();
    let _ = partial.begin_deferred(Resolution::new());
    let _ = partial.begin_field("maybe");
    let _ = partial.begin_field("items");
    let _ = partial.begin_inner();
    let _ = partial.begin_inner();
    let _ = partial.begin_inner();
    let _ = partial.begin_field("items");
    let _ = partial.begin_value();
    let _ = partial.set(788725895u32);
    let _ = partial.set("bfj".to_string());
    let _ = partial.end();
    let _ = partial.begin_field("items");
    let _ = partial.begin_list();
    let _ = partial.begin_field("name");
    let _ = partial.begin_some();
    let _ = partial.begin_list_item();
    let _ = partial.end();
    let _ = partial.set(1767328903u32);
    let _ = partial.begin_field("name");
    
    println!("Frame count: {}", partial.frame_count());
    println!("Is deferred: {}", partial.is_deferred());
    
    // Drop will happen here
}
