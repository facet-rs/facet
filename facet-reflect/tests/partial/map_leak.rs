use std::collections::HashMap;

use facet_reflect::Partial;
use facet_testhelpers::test;

// If we partially initialize a map, do we leak memory.unwrap()
#[test]
fn wip_map_leaktest1() {
    let mut wip = Partial::alloc::<HashMap<String, String>>().unwrap();
    wip.begin_map()
        .unwrap()
        .begin_key()
        .unwrap()
        .set("key".to_string())
        .unwrap()
        .end()
        .unwrap()
        .begin_value()
        .unwrap()
        .set("value".to_string())
        .unwrap()
        .end()
        .unwrap();
    let wip = wip.build().unwrap();
    drop(wip);
}

// If we partially initialize a map, do we leak memory.unwrap()
#[test]
fn wip_map_leaktest2() {
    let mut wip = Partial::alloc::<HashMap<String, String>>().unwrap();
    wip.begin_map()
        .unwrap()
        .begin_key()
        .unwrap()
        .set("key".to_string())
        .unwrap()
        .end()
        .unwrap()
        .begin_value()
        .unwrap()
        .set("value".to_string())
        .unwrap()
        .end()
        .unwrap();
    drop(wip);
}

// If we partially initialize a map, do we leak memory.unwrap()
#[test]
fn wip_map_leaktest3() {
    let mut wip = Partial::alloc::<HashMap<String, String>>().unwrap();
    wip.begin_map()
        .unwrap()
        .begin_key()
        .unwrap()
        .set("key".to_string())
        .unwrap()
        .end()
        .unwrap()
        .begin_value()
        .unwrap()
        .set("value".to_string())
        .unwrap();
    drop(wip);
}

// If we partially initialize a map, do we leak memory.unwrap()
#[test]
fn wip_map_leaktest4() {
    let mut wip = Partial::alloc::<HashMap<String, String>>().unwrap();
    wip.begin_map()
        .unwrap()
        .begin_key()
        .unwrap()
        .set("key".to_string())
        .unwrap()
        .end()
        .unwrap()
        .begin_value()
        .unwrap();
    drop(wip);
}

// If we partially initialize a map, do we leak memory.unwrap()
#[test]
fn wip_map_leaktest5() {
    let mut wip = Partial::alloc::<HashMap<String, String>>().unwrap();
    wip.begin_map()
        .unwrap()
        .begin_key()
        .unwrap()
        .set("key".to_string())
        .unwrap();
    drop(wip);
}

// If we partially initialize a map, do we leak memory.unwrap()
#[test]
fn wip_map_leaktest6() {
    let mut wip = Partial::alloc::<HashMap<String, String>>().unwrap();
    wip.begin_map().unwrap().begin_key().unwrap();
    drop(wip);
}

// If we partially initialize a map, do we leak memory.unwrap()
#[test]
fn wip_map_leaktest7() {
    let mut wip = Partial::alloc::<HashMap<String, String>>().unwrap();
    wip.begin_map().unwrap();
    drop(wip);
}

// If we partially initialize a map, do we leak memory.unwrap()
#[test]
fn wip_map_leaktest8() {
    let wip = Partial::alloc::<HashMap<String, String>>().unwrap();
    drop(wip);
}
